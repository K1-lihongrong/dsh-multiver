//! 数据备份/导出（B2，v0.2.7）。
//!
//! 导出**配置 + 版本清单**（小体积），供换机迁移参考。
//! **不导出**依赖（大）、**不导出**会话/凭据（敏感）。

use crate::config::Config;
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
}

/// 构造备份清单（不写盘）。
pub fn build_manifest(cfg: &Config, versions_dir: &Path) -> BackupManifest {
    let list = versions::list(versions_dir, cfg.default_version.as_deref(), &cfg.isolated_versions);
    let versions = list
        .into_iter()
        .map(|v| VersionEntry {
            version: v.version,
            is_default: v.is_default,
            isolated: v.isolated,
            installed_at: v.installed_at,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-backup-{}-{}",
            tag,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
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
        let mut cfg = Config::default();
        cfg.default_version = Some("0.1.7".to_string());
        let m = build_manifest(&cfg, &versions_dir);
        assert_eq!(m.manager_version, crate::cli::VERSION);
        assert_eq!(m.versions.len(), 1);
        assert_eq!(m.versions[0].version, "0.1.7");
        assert!(m.versions[0].is_default);
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
