//! 数据备份/导出（B2，v0.2.7）。
//!
//! 导出**配置 + 版本清单**（小体积），供换机迁移参考。
//! **不导出**依赖（大）、**不导出**会话/凭据（敏感）。

use crate::config::{Config, CURRENT_SCHEMA_VERSION};
use crate::domain::VersionId;
use crate::versions;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 导出的清单内容。
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupManifest {
    /// 导出时间（unix 秒）
    pub exported_at: u64,
    /// 生成此备份的管理器版本
    pub manager_version: String,
    /// 配置快照（root_dir / default_version / isolated_versions / use_official_dsh_home 等）
    pub config: Config,
    /// 已安装版本清单（不含依赖体积）
    pub versions: Vec<VersionEntry>,
}

/// 单个版本的清单条目。
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: String,
    pub is_default: bool,
    pub isolated: bool,
    pub installed_at: String,
    /// 该版本挂载的实例数（旧备份缺省为 0）。
    #[serde(default)]
    pub instance_count: usize,
    /// 该版本主按钮目标（旧备份缺省为 None）。
    #[serde(default)]
    pub selected_target: Option<String>,
}

/// 构造备份清单（不写盘）。
pub fn build_manifest(cfg: &Config, versions_dir: &Path) -> BackupManifest {
    let list = versions::list(
        versions_dir,
        cfg.default_version.as_deref(),
        &cfg.isolated_versions,
    );
    let versions = list
        .into_iter()
        .map(|v| {
            let version = v.version;
            let vid = VersionId::new(&version).ok();
            VersionEntry {
                instance_count: vid
                    .as_ref()
                    .and_then(|id| cfg.version_instances.get(id))
                    .map(|ids| ids.len())
                    .unwrap_or(0),
                selected_target: vid
                    .as_ref()
                    .and_then(|id| cfg.selected_target.get(id))
                    .and_then(|t| t.as_ref())
                    .map(|id| id.to_string()),
                version,
                is_default: v.is_default,
                isolated: v.isolated,
                installed_at: v.installed_at,
            }
        })
        .collect();
    BackupManifest {
        exported_at: crate::logging::now_secs(),
        manager_version: crate::cli::VERSION.to_string(),
        config: cfg.clone(),
        versions,
    }
}

/// 导出到指定文件（JSON，pretty）。返回写入路径。
pub fn export_to_file(cfg: &Config, versions_dir: &Path, out: &Path) -> Result<String, String> {
    let manifest = build_manifest(cfg, versions_dir);
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| format!("序列化失败：{}", e))?;
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, text).map_err(|e| format!("写入失败：{}", e))?;
    Ok(out.to_string_lossy().to_string())
}

/// 导入预览：读备份文件，返回"将改动的字段"（不应用）。
#[derive(Debug, Serialize, specta::Type)]
pub struct ImportPreview {
    /// 备份里的管理器版本
    pub backup_manager_version: String,
    /// 备份里记录的已安装版本数
    pub version_count: usize,
    /// 将改动的配置字段（人类可读的 "字段: 旧 -> 新" 列表）
    pub changes: Vec<String>,
    /// 备份里"本机未安装"的版本（仅提示，不影响配置导入）
    pub missing_versions: Vec<String>,
}

/// 归一化备份配置：
/// - 旧快照（schema<2，只有 isolated_versions）为每个隔离版本合成“默认隔离”实例；
/// - 新快照清理实例/挂载/主按钮中的悬空引用。
fn normalize_backup_config(source: &Config) -> (Config, Vec<String>) {
    let mut out = source.clone();
    let mut notes = Vec::new();
    if out.schema_version < CURRENT_SCHEMA_VERSION {
        let legacy = out.isolated_versions.clone();
        out.instances.clear();
        out.version_instances.clear();
        out.selected_target.clear();
        out.instance_seq = 0;
        for raw in &legacy {
            let Ok(vid) = VersionId::new(raw) else {
                notes.push(format!("旧备份：跳过非法版本号 {raw}"));
                continue;
            };
            let Ok(id) = out.create_instance("默认隔离", 0) else {
                notes.push(format!("旧备份：{} 的默认实例创建失败", vid));
                continue;
            };
            if let Err(e) = out.attach_instance(&vid, &id) {
                notes.push(format!("旧备份：{} 挂载失败：{e}", vid));
                continue;
            }
            let _ = out.set_selected_target(&vid, Some(&id));
        }
        out.schema_version = CURRENT_SCHEMA_VERSION;
        if !legacy.is_empty() {
            notes.push(format!(
                "旧备份：为 {} 个隔离版本生成默认实例（不搬 home 数据）",
                legacy.len()
            ));
        }
    } else {
        notes.extend(out.sanitize_refs());
    }
    (out, notes)
}

/// 读取并校验备份文件，生成导入预览（不应用）。
pub fn preview_import(
    file: &Path,
    current: &Config,
    versions_dir: &Path,
) -> Result<ImportPreview, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("读取文件失败：{}", e))?;
    let manifest: BackupManifest = serde_json::from_str(&text)
        .map_err(|e| format!("解析失败（不是有效的备份文件）：{}", e))?;

    let (b, notes) = normalize_backup_config(&manifest.config);
    let mut changes = notes;
    if b.root_dir != current.root_dir {
        changes.push(format!(
            "数据根目录：{} -> {}",
            current.root_dir.as_deref().unwrap_or("(默认)"),
            b.root_dir.as_deref().unwrap_or("(默认)")
        ));
    }
    if b.default_version != current.default_version {
        changes.push(format!(
            "默认版本：{} -> {}",
            current.default_version.as_deref().unwrap_or("(无)"),
            b.default_version.as_deref().unwrap_or("(无)")
        ));
    }
    if b.isolated_versions != current.isolated_versions {
        changes.push(format!(
            "隔离版本：{:?} -> {:?}",
            current.isolated_versions, b.isolated_versions
        ));
    }
    if b.use_official_dsh_home != current.use_official_dsh_home {
        changes.push(format!(
            "复用官方配置：{} -> {}",
            current.use_official_dsh_home, b.use_official_dsh_home
        ));
    }
    if b.instances.len() != current.instances.len() {
        changes.push(format!(
            "实例：{} -> {}",
            current.instances.len(),
            b.instances.len()
        ));
    }
    let current_links: usize = current.version_instances.values().map(Vec::len).sum();
    let backup_links: usize = b.version_instances.values().map(Vec::len).sum();
    if current_links != backup_links {
        changes.push(format!("实例挂载：{} -> {}", current_links, backup_links));
    }

    // 备份里本机未安装的版本
    let missing_versions = manifest
        .versions
        .iter()
        .filter(|v| !versions::exists(versions_dir, &v.version))
        .map(|v| v.version.clone())
        .collect();

    Ok(ImportPreview {
        backup_manager_version: manifest.manager_version,
        version_count: manifest.versions.len(),
        changes,
        missing_versions,
    })
}

/// 应用导入：把备份文件的 config 字段写入当前配置。返回改动摘要。
///
/// 只应用**配置字段**（root_dir / default_version / isolated_versions /
/// broken_versions / use_official_dsh_home / maintenance）；
/// **不安装版本**（版本需用户另行安装）。
pub fn apply_import(file: &Path, current: &mut Config) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("读取文件失败：{}", e))?;
    let manifest: BackupManifest =
        serde_json::from_str(&text).map_err(|e| format!("解析失败：{}", e))?;
    let (b, _notes) = normalize_backup_config(&manifest.config);

    let mut applied = Vec::new();
    if b.root_dir != current.root_dir {
        applied.push("数据根目录".to_string());
        current.root_dir = b.root_dir;
    }
    if b.default_version != current.default_version {
        applied.push("默认版本".to_string());
        current.default_version = b.default_version;
    }
    if b.isolated_versions != current.isolated_versions {
        applied.push("隔离版本列表".to_string());
        current.isolated_versions = b.isolated_versions.clone();
    }
    if b.use_official_dsh_home != current.use_official_dsh_home {
        applied.push("复用官方配置".to_string());
        current.use_official_dsh_home = b.use_official_dsh_home;
    }
    let instances_changed = b.instances.len() != current.instances.len()
        || b.version_instances.len() != current.version_instances.len()
        || b.selected_target.len() != current.selected_target.len();
    current.instances = b.instances.clone();
    current.version_instances = b.version_instances.clone();
    current.selected_target = b.selected_target.clone();
    current.instance_seq = b.instance_seq;
    current.schema_version = CURRENT_SCHEMA_VERSION;
    if instances_changed {
        applied.push("实例表".to_string());
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-backup-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn manifest_includes_config_and_versions() {
        let base = tmp("manifest");
        let versions_dir = base.join("versions");
        // versions::list 要求目录内含 node_modules 才算有效版本
        std::fs::create_dir_all(versions_dir.join("0.1.7").join("node_modules")).unwrap();
        let cfg = Config {
            default_version: Some("0.1.7".to_string()),
            ..Default::default()
        };
        let m = build_manifest(&cfg, &versions_dir);
        assert_eq!(m.manager_version, crate::cli::VERSION);
        assert_eq!(m.versions.len(), 1);
        assert_eq!(m.versions[0].version, "0.1.7");
        assert!(m.versions[0].is_default);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn preview_and_apply_import() {
        let base = tmp("import");
        let versions_dir = base.join("versions");
        std::fs::create_dir_all(versions_dir.join("0.2.0").join("node_modules")).unwrap();

        // 造一个备份文件：默认版本 0.1.7（本机未装）、复用官方=true
        let src = Config {
            default_version: Some("0.1.7".to_string()),
            use_official_dsh_home: true,
            ..Default::default()
        };
        let file = base.join("backup.json");
        export_to_file(&src, &versions_dir, &file).unwrap();

        // 当前配置：默认版本 0.2.0、复用官方=false
        let mut cur = Config {
            default_version: Some("0.2.0".to_string()),
            ..Default::default()
        };

        let preview = preview_import(&file, &cur, &versions_dir).unwrap();
        assert!(preview.changes.iter().any(|c| c.contains("默认版本")));
        assert!(preview.changes.iter().any(|c| c.contains("复用官方")));
        // 备份的 versions 来自实际安装（0.2.0，本机已装）→ 无缺失
        assert!(preview.missing_versions.is_empty());

        // 应用
        let applied = apply_import(&file, &mut cur).unwrap();
        assert!(applied.contains(&"默认版本".to_string()));
        assert_eq!(cur.default_version.as_deref(), Some("0.1.7"));
        assert!(cur.use_official_dsh_home);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn instance_tables_roundtrip_through_export_import() {
        let base = tmp("instance-roundtrip");
        let versions_dir = base.join("versions");
        std::fs::create_dir_all(versions_dir.join("0.1.0").join("node_modules")).unwrap();
        let version = VersionId::new("0.1.0").unwrap();
        let mut src = Config::default();
        let id = src.create_instance("A", 0).unwrap();
        src.attach_instance(&version, &id).unwrap();
        let file = base.join("backup.json");
        export_to_file(&src, &versions_dir, &file).unwrap();

        let mut cur = Config::default();
        let preview = preview_import(&file, &cur, &versions_dir).unwrap();
        assert!(preview.changes.iter().any(|c| c.contains("实例")));
        apply_import(&file, &mut cur).unwrap();
        assert_eq!(cur.instances.len(), 1);
        assert_eq!(cur.instances[0].name, "A");
        assert_eq!(cur.version_instances.get(&version), Some(&vec![id.clone()]));
        assert_eq!(cur.selected_target.get(&version), Some(&Some(id)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn old_backup_generates_default_instances() {
        let base = tmp("old-backup");
        let versions_dir = base.join("versions");
        std::fs::create_dir_all(versions_dir.join("0.1.0").join("node_modules")).unwrap();
        let src = Config {
            schema_version: 1,
            isolated_versions: vec!["0.1.0".to_string()],
            ..Default::default()
        };
        let file = base.join("old.json");
        export_to_file(&src, &versions_dir, &file).unwrap();

        let mut cur = Config::default();
        let preview = preview_import(&file, &cur, &versions_dir).unwrap();
        assert!(preview.changes.iter().any(|c| c.contains("旧备份")));
        apply_import(&file, &mut cur).unwrap();
        assert_eq!(cur.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(cur.instances.len(), 1);
        assert_eq!(cur.instances[0].name, "默认隔离");
        assert_eq!(
            cur.selected_target.get(&VersionId::new("0.1.0").unwrap()),
            Some(&Some(cur.instances[0].id.clone()))
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn import_rejects_invalid_file() {
        let base = tmp("import-bad");
        let file = base.join("bad.json");
        std::fs::write(&file, "{ not json").unwrap();
        let mut cur = Config::default();
        assert!(preview_import(&file, &cur, &base).is_err());
        assert!(apply_import(&file, &mut cur).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn export_writes_readable_json() {
        let base = tmp("export");
        let versions_dir = base.join("versions");
        std::fs::create_dir_all(versions_dir.join("0.2.0").join("node_modules")).unwrap();
        let out = base.join("backup.json");
        let cfg = Config::default();
        let path = export_to_file(&cfg, &versions_dir, &out).unwrap();
        assert!(out.exists());
        let text = std::fs::read_to_string(&path).unwrap();
        let back: BackupManifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.versions.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }
}
