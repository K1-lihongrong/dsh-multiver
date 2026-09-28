//! DSH 整合包（.dspack）支持。
//!
//! 一期实现「导入」：容器解包、manifest 解析校验、四阶段导入与回滚。
//! 协议见 DSH-PackForge（`protocol-2026-09`），方案见项目文档
//! `.cuckooCode/兼容DSH-PackForge-方案.md`。
//!
//! 本模块第一步只做：容器识别（`dspack.json`）与解包。

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `.dspack` 根部的容器标记文件内容。
#[derive(Debug, Clone, Deserialize)]
pub struct DspackMarker {
    pub format: String,
    pub version: u32,
}

/// 支持的容器版本：现行 v3，兼容 v2（manifest v4）。
pub const SUPPORTED_CONTAINER_VERSIONS: &[u32] = &[2, 3];

/// 解包并预检的结果。此时尚未落地任何用户文件。
#[derive(Debug)]
pub struct Extracted {
    /// 解包到的临时目录（调用方负责在结束后清理）
    pub dir: PathBuf,
    /// 容器标记
    pub marker: DspackMarker,
}

impl Extracted {
    /// 清理临时目录。忽略失败（临时目录残留无害）。
    pub fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 解包 `.dspack` 到系统临时目录，并校验容器标记。
///
/// 失败时自动清理临时目录；成功后由调用方持有 `Extracted`，用完调 `cleanup()`。
pub fn extract(dspack_path: &Path) -> Result<Extracted, String> {
    if !dspack_path.is_file() {
        return Err(format!("文件不存在: {}", dspack_path.display()));
    }

    // 临时目录：dsh-multiver-import-<纳秒>，避免并发冲突
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("dsh-multiver-import-{}", stamp));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;

    // 用闭包包住解包+校验，失败时统一清理临时目录
    let result = (|| -> Result<DspackMarker, String> {
        let file = std::fs::File::open(dspack_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            // 非 ZIP（普通压缩包 / 损坏文件）走这里
            format!("这不是 dspack 整合包（无法作为 ZIP 打开）: {}", e)
        })?;

        // 逐条解压。zip crate 的 by_index 保证 name 是相对路径，
        // 但仍要防御性地拒绝越界路径（`..` / 绝对路径）。
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取归档条目失败: {}", e))?;
            let rel = match entry.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => {
                    return Err(format!("归档内存在非法路径: {}", entry.name()));
                }
            };
            let out = tmp.join(&rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)
                    .map_err(|e| format!("创建目录失败 {}: {}", out.display(), e))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("读取条目内容失败 {}: {}", rel.display(), e))?;
            std::fs::write(&out, &buf)
                .map_err(|e| format!("写入文件失败 {}: {}", out.display(), e))?;
        }

        // 容器标记必须在归档根
        let marker_path = tmp.join("dspack.json");
        if !marker_path.is_file() {
            return Err("这不是 dspack 整合包（根目录缺少 dspack.json）".to_string());
        }
        let text = std::fs::read_to_string(&marker_path)
            .map_err(|e| format!("读取 dspack.json 失败: {}", e))?;
        let marker: DspackMarker = serde_json::from_str(&text)
            .map_err(|e| format!("dspack.json 格式非法: {}", e))?;

        if marker.format != "dspack" {
            return Err(format!(
                "这不是 dspack 整合包（format 为 \"{}\"）",
                marker.format
            ));
        }
        if !SUPPORTED_CONTAINER_VERSIONS.contains(&marker.version) {
            return Err(format!(
                "不支持的容器版本 v{}（本管理器支持 v2 / v3），请升级管理器",
                marker.version
            ));
        }

        Ok(marker)
    })();

    match result {
        Ok(marker) => Ok(Extracted { dir: tmp, marker }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// 读取解包目录内的 manifest.json 原文（解析留给下一步）。
pub fn read_manifest_text(dir: &Path) -> Result<String, String> {
    let p = dir.join("manifest.json");
    if !p.is_file() {
        return Err("整合包内缺少 manifest.json".to_string());
    }
    std::fs::read_to_string(&p).map_err(|e| format!("读取 manifest.json 失败: {}", e))
}

// ===================== 路径安全与落盘原语 =====================

/// 判断某个归档内相对路径是否命中「敏感文件」过滤。
///
/// 打包侧已做五类过滤，导入侧仍要防御式拒绝——防止恶意包写敏感文件。
/// 返回 Some(原因) 表示应拒绝。
pub fn sensitive_reason(rel: &Path) -> Option<String> {
    use std::path::Component;

    // 逐段检查
    let segs: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();

    let basename = segs.last().cloned().unwrap_or_default();
    let lower = basename.to_lowercase();

    // 精确名（任意路径段命中）
    for seg in &segs {
        let s = seg.to_lowercase();
        if matches!(
            s.as_str(),
            "node_modules"
                | ".env"
                | ".credentials.yaml"
                | ".anonymous-user-id"
                | "settings.yaml"
                | ".dshpkcfg"
                | "credentials.yaml"
                | "credentials.yml"
                | "id_rsa"
                | "id_ed25519"
        ) {
            return Some(format!("含敏感文件/目录 \"{}\"", seg));
        }
        // attachments/ 与安装基线模板
        if s == "attachments" || s == ".system" {
            return Some(format!("含运行时/系统目录 \"{}\"", seg));
        }
    }

    // 扩展名
    if let Some(ext) = rel.extension().and_then(|e| e.to_str()) {
        let e = ext.to_lowercase();
        if matches!(
            e.as_str(),
            "key" | "pem" | "p12" | "pfx" | "crt" | "der" | "asc"
        ) {
            return Some(format!("含密钥/证书文件 \"{}\"", basename));
        }
        // 嵌套压缩包
        if matches!(e.as_str(), "zip" | "dspack" | "tgz" | "gz" | "tar") {
            return Some(format!("含嵌套压缩包 \"{}\"", basename));
        }
    }

    // 文件名正则类（用简单前缀/包含判断，避免引入 regex 依赖）
    let is_cred = (lower.starts_with("credentials") && (lower.ends_with(".yml") || lower.ends_with(".yaml")))
        || lower.ends_with(".credentials")
        || (lower.starts_with("secrets") && (lower.ends_with(".json") || lower.ends_with(".yml") || lower.ends_with(".yaml")))
        || lower.contains("token")
        || lower.contains("api_key")
        || lower.starts_with("id_rsa")
        || lower.starts_with("id_ed25519")
        || (lower.starts_with(".env"));
    if is_cred {
        return Some(format!("含疑似凭据文件 \"{}\"", basename));
    }

    None
}

/// 安全拼接：把归档内相对路径 rel 拼到 base 下，拒绝越界。
///
/// rel 必须是纯相对路径（不含 \`..\`、不以盘符/根开头）。
pub fn safe_join(base: &Path, rel: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!("非法路径（含 ..）: {}", rel.display()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("非法路径（绝对路径）: {}", rel.display()));
            }
        }
    }
    Ok(base.join(rel))
}

/// 递归把 src 下的内容复制到 dst（保持相对结构），逐条施加安全校验。
///
/// - 跳过命中 sensitive_reason 的条目（记录到 skipped）
/// - 目标已存在则覆盖
/// - 返回 (复制的文件数, 被跳过的条目列表)
pub fn copy_tree_checked(
    src: &Path,
    dst: &Path,
    skipped: &mut Vec<String>,
) -> Result<u64, String> {
    if !src.exists() {
        return Ok(0);
    }
    let mut count = 0u64;
    copy_tree_inner(src, src, dst, skipped, &mut count)?;
    Ok(count)
}

fn copy_tree_inner(
    root: &Path,
    cur: &Path,
    dst: &Path,
    skipped: &mut Vec<String>,
    count: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(cur).map_err(|e| format!("读取目录失败 {}: {}", cur.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| format!("路径解析失败: {}", e))?;

        if let Some(reason) = sensitive_reason(rel) {
            skipped.push(format!("{}（{}）", rel.display(), reason));
            continue;
        }

        let out = safe_join(dst, rel)?;
        if path.is_dir() {
            std::fs::create_dir_all(&out)
                .map_err(|e| format!("创建目录失败 {}: {}", out.display(), e))?;
            copy_tree_inner(root, &path, dst, skipped, count)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
            }
            std::fs::copy(&path, &out)
                .map_err(|e| format!("复制失败 {} → {}: {}", path.display(), out.display(), e))?;
            *count += 1;
        }
    }
    Ok(())
}

// ===================== manifest 解析与校验 =====================

/// 支持的 manifest 版本：现行 v5，兼容 v4。
/// （协议另列 v3 / v2 / v1，但它们绑定 .tgz 历史格式，一期不支持。）
pub const SUPPORTED_MANIFEST_VERSIONS: &[u32] = &[4, 5];

/// 多语言文本：既可能是普通字符串，也可能是「语言码 → 文本」的 map。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocalizedText {
    Plain(String),
    Localized(std::collections::HashMap<String, String>),
}

impl LocalizedText {
    /// 取展示文本：优先 zh-CN，其次 en-US，再取 map 中字典序最小的项，最后空串。
    pub fn display(&self) -> String {
        match self {
            LocalizedText::Plain(s) => s.clone(),
            LocalizedText::Localized(m) => {
                for key in ["zh-CN", "zh", "en-US", "en"] {
                    if let Some(v) = m.get(key) {
                        return v.clone();
                    }
                }
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                keys.first().and_then(|k| m.get(*k)).cloned().unwrap_or_default()
            }
        }
    }

    /// 是否为空（无任何文本）
    pub fn is_empty(&self) -> bool {
        match self {
            LocalizedText::Plain(s) => s.trim().is_empty(),
            LocalizedText::Localized(m) => m.is_empty(),
        }
    }
}

/// dshhome 形态的单个 profile 单元（= v4 单 profile 契约去掉 profileName）。
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileUnit {
    pub bundles: Vec<String>,
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub patch: Option<String>,
}

/// files[] 条目（重内容下载清单）。
#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub urls: Vec<String>,
}

/// manifest 的原始结构。缺失的必填项在 validate() 里统一收集报告
/// （而非 serde 遇第一个错误就退出）。
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    #[serde(rename = "type")]
    pub kind: String,

    // 必填字段用 Option 承接：缺失时走 validate() 统一报告，
    // 而不是让 serde 在解析阶段就失败（那样只能报第一个错误）。
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,

    #[serde(default, rename = "displayName")]
    pub display_name: Option<LocalizedText>,
    #[serde(default)]
    pub description: Option<LocalizedText>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub category: Option<String>,

    #[serde(default, rename = "dshVersion")]
    pub dsh_version: Option<String>,
    #[serde(default, rename = "dshVersions")]
    pub dsh_versions: Option<Vec<String>>,

    #[serde(default)]
    pub files: Vec<FileRef>,

    // ---- profile 形态 ----
    #[serde(default, rename = "profileName")]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub bundles: Vec<String>,
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub patch: Option<String>,

    // ---- dshhome 形态 ----
    #[serde(default, rename = "defaultProfile")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, ProfileUnit>,

    /// 保留原始 JSON，供后续未建模字段（如 vendored / launchers）按需读取。
    #[serde(skip)]
    pub raw: serde_json::Value,
}

impl Manifest {
    /// 该 manifest 是否为 dshhome 形态。
    pub fn is_dshhome(&self) -> bool {
        self.kind == "dshhome"
    }

    /// 展示名（回退到 name）。
    pub fn display_name_text(&self) -> String {
        match &self.display_name {
            Some(t) if !t.is_empty() => t.display(),
            _ => self.name.clone().unwrap_or_default(),
        }
    }

    /// 描述（可能为空）。
    pub fn description_text(&self) -> String {
        self.description.as_ref().map(|t| t.display()).unwrap_or_default()
    }

    /// 启动该包应使用的 profile 名。
    /// profile 形态取 profileName（缺省 "pack"）；dshhome 形态取 defaultProfile。
    pub fn launch_profile(&self) -> Option<String> {
        if self.is_dshhome() {
            self.default_profile.clone()
        } else {
            Some(
                self.profile_name
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "pack".to_string()),
            )
        }
    }

    /// 解析并校验 manifest 原文。返回所有问题（一次性收集）。
    pub fn parse(text: &str) -> Result<Manifest, Vec<String>> {
        let mut m: Manifest = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => return Err(vec![format!("manifest.json 解析失败: {}", e)]),
        };
        m.raw = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);

        let errs = m.validate();
        if errs.is_empty() {
            Ok(m)
        } else {
            Err(errs)
        }
    }

    /// 字段校验，返回全部问题。
    fn validate(&self) -> Vec<String> {
        let mut e: Vec<String> = Vec::new();

        if !SUPPORTED_MANIFEST_VERSIONS.contains(&self.manifest_version) {
            if self.manifest_version == 1 {
                e.push("已废弃的包格式（manifest v1），无法安装".to_string());
            } else if self.manifest_version < 1 || self.manifest_version > 5 {
                e.push(format!(
                    "不支持的 manifest 版本 v{}（支持 v4 / v5）",
                    self.manifest_version
                ));
            } else {
                e.push(format!(
                    "该包为历史格式（manifest v{}，.tgz），一期暂不支持",
                    self.manifest_version
                ));
            }
        }

        match &self.name {
            None => e.push("缺少必填字段 name".to_string()),
            Some(n) if n.trim().is_empty() => e.push("必填字段 name 为空".to_string()),
            _ => {}
        }
        match &self.version {
            None => e.push("缺少必填字段 version".to_string()),
            Some(v) if v.trim().is_empty() => e.push("必填字段 version 为空".to_string()),
            _ => {}
        }

        match self.kind.as_str() {
            "profile" => {
                if self.bundles.is_empty() {
                    e.push("缺少必填字段 bundles（profile 形态必须有层栈）".to_string());
                }
            }
            "dshhome" => {
                if self.profiles.is_empty() {
                    e.push("dshhome 形态必须有非空的 profiles".to_string());
                }
                for bad in ["web", "headless"] {
                    if self.profiles.contains_key(bad) {
                        e.push(format!(
                            "profiles 不得包含安装基线模板 \"{}\"（由 dshVersion 决定，不进包）",
                            bad
                        ));
                    }
                }
                match &self.default_profile {
                    None => e.push("dshhome 形态必须有 defaultProfile".to_string()),
                    Some(dp) => {
                        if !self.profiles.contains_key(dp) {
                            e.push(format!(
                                "defaultProfile 指向的 \"{}\" 不在 profiles 中",
                                dp
                            ));
                        }
                    }
                }
            }
            "collection" => {
                e.push("type: \"collection\" 为预留形态，暂不支持".to_string());
            }
            other => {
                e.push(format!(
                    "字段 type 值非法：\"{}\"（应为 \"profile\" 或 \"dshhome\"）",
                    other
                ));
            }
        }

        if let Some(set) = &self.dsh_versions {
            if set.is_empty() {
                e.push("dshVersions 若出现则不得为空数组".to_string());
            }
            let mut seen = std::collections::HashSet::new();
            for v in set {
                if v.trim().is_empty() {
                    e.push("dshVersions 含空字符串项".to_string());
                }
                if !seen.insert(v) {
                    e.push(format!("dshVersions 含重复项 \"{}\"", v));
                }
            }
            if let Some(dv) = &self.dsh_version {
                if !set.iter().any(|v| v == dv) {
                    e.push(format!("dshVersion \"{}\" 不在 dshVersions 集合中", dv));
                }
            }
        }

        for (i, f) in self.files.iter().enumerate() {
            if f.path.trim().is_empty() {
                e.push(format!("files[{}].path 为空", i));
            }
            if f.urls.is_empty() {
                e.push(format!("files[{}].urls 为空（至少一个下载地址）", i));
            }
        }

        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用代码动态构造一个最小 `.dspack`（ZIP + dspack.json + manifest.json）。
    /// 只用于本地构造的测试，不依赖网络。
    fn build_min_dspack(dir: &Path, marker_json: &str, with_manifest: bool) -> PathBuf {
        let path = dir.join("test.dspack");
        let file = std::fs::File::create(&path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("dspack.json", opts).unwrap();
        use std::io::Write;
        zw.write_all(marker_json.as_bytes()).unwrap();
        if with_manifest {
            zw.start_file("manifest.json", opts).unwrap();
            zw.write_all(b"{\"manifestVersion\":5,\"type\":\"profile\"}").unwrap();
        }
        zw.finish().unwrap();
        path
    }

    #[test]
    fn rejects_non_zip() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nonzip");
        let _ = std::fs::create_dir_all(&tmp);
        let p = tmp.join("not-a-zip.dspack");
        std::fs::write(&p, b"hello world").unwrap();
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不是 dspack 整合包"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_missing_marker() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nomarker");
        let _ = std::fs::create_dir_all(&tmp);
        // 一个合法 ZIP，但根没有 dspack.json
        let p = tmp.join("empty.dspack");
        let file = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("readme.txt", opts).unwrap();
        use std::io::Write;
        zw.write_all(b"hi").unwrap();
        zw.finish().unwrap();

        let err = extract(&p).unwrap_err();
        assert!(err.contains("缺少 dspack.json"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_unsupported_container_version() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-badver");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":9}"#, true);
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不支持的容器版本 v9"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- 路径安全与落盘原语 ----------

    #[test]
    fn safe_join_rejects_parent_dir() {
        let base = Path::new("C:/base");
        assert!(safe_join(base, Path::new("a/b.txt")).is_ok());
        assert!(safe_join(base, Path::new("../evil.txt")).is_err());
        assert!(safe_join(base, Path::new("a/../../evil.txt")).is_err());
    }

    #[test]
    fn sensitive_reason_blocks_common_cases() {
        assert!(sensitive_reason(Path::new(".env")).is_some());
        assert!(sensitive_reason(Path::new("config/.env")).is_some());
        assert!(sensitive_reason(Path::new("node_modules/x.js")).is_some());
        assert!(sensitive_reason(Path::new("keys/server.pem")).is_some());
        assert!(sensitive_reason(Path::new("creds/credentials.yaml")).is_some());
        assert!(sensitive_reason(Path::new("nested/inner.zip")).is_some());
        assert!(sensitive_reason(Path::new("settings.yaml")).is_some());
        assert!(sensitive_reason(Path::new(".dshpkcfg")).is_some());

        // 正常文件应放行
        assert!(sensitive_reason(Path::new("cordis.patch.yml")).is_none());
        assert!(sensitive_reason(Path::new("skills/demo/SKILL.md")).is_none());
        assert!(sensitive_reason(Path::new("package.json")).is_none());
    }

    #[test]
    fn copy_tree_skips_sensitive_files() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-copytree");
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(src.join("skills/demo")).unwrap();
        std::fs::write(src.join("cordis.patch.yml"), b"[]").unwrap();
        std::fs::write(src.join("skills/demo/SKILL.md"), b"# demo").unwrap();
        std::fs::write(src.join(".env"), b"SECRET=1").unwrap();
        std::fs::write(src.join("server.pem"), b"key").unwrap();

        let mut skipped = Vec::new();
        let n = copy_tree_checked(&src, &dst, &mut skipped).unwrap();

        assert_eq!(n, 2, "应只复制 2 个正常文件");
        assert_eq!(skipped.len(), 2, "应跳过 2 个敏感文件");
        assert!(dst.join("cordis.patch.yml").is_file());
        assert!(dst.join("skills/demo/SKILL.md").is_file());
        assert!(!dst.join(".env").exists());
        assert!(!dst.join("server.pem").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- manifest 解析 ----------

    const MIN_PROFILE: &str = r#"{
        "manifestVersion": 5,
        "type": "profile",
        "name": "demo",
        "version": "1.0.0",
        "bundles": ["@deepseek-ai/dsh-base"],
        "dependencies": {"dsh-pet": "0.2.0"}
    }"#;

    #[test]
    fn parses_min_profile() {
        let m = Manifest::parse(MIN_PROFILE).unwrap();
        assert_eq!(m.kind, "profile");
        assert!(!m.is_dshhome());
        assert_eq!(m.launch_profile().as_deref(), Some("pack"));
        assert_eq!(m.display_name_text(), "demo");
    }

    #[test]
    fn localized_text_plain_and_map() {
        let plain: LocalizedText = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(plain.display(), "hello");

        let map: LocalizedText =
            serde_json::from_str(r#"{"en-US":"Hi","zh-CN":"你好"}"#).unwrap();
        assert_eq!(map.display(), "你好");
    }

    #[test]
    fn collects_all_errors() {
        // 缺 name、bundles 为空、type 非法 —— 应一次全报出来
        let bad = r#"{
            "manifestVersion": 5,
            "type": "banana",
            "version": "1.0.0"
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        let joined = errs.join("\n");
        assert!(joined.contains("name"), "应报缺 name: {}", joined);
        assert!(joined.contains("type"), "应报 type 非法: {}", joined);
    }

    #[test]
    fn rejects_deprecated_v1() {
        let bad = r#"{"manifestVersion":1,"type":"profile","name":"x","version":"1.0.0","bundles":["a"]}"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("v1")), "应报 v1 已废弃: {:?}", errs);
    }

    #[test]
    fn rejects_dshhome_without_default() {
        let bad = r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "profiles": {"a": {"bundles": ["b"]}}
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("defaultProfile")),
            "应报缺 defaultProfile: {:?}",
            errs
        );
    }

    #[test]
    fn rejects_dshhome_containing_web() {
        let bad = r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "defaultProfile": "web",
            "profiles": {"web": {"bundles": ["b"]}}
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("基线模板")),
            "应报含 web 模板: {:?}",
            errs
        );
    }

    #[test]
    fn accepts_v4_manifest() {
        let v4 = r#"{
            "manifestVersion": 4,
            "type": "profile",
            "name": "old",
            "version": "1.0.0",
            "bundles": ["a"],
            "dependencies": {}
        }"#;
        let m = Manifest::parse(v4).unwrap();
        assert_eq!(m.manifest_version, 4);
    }

    #[test]
    fn dshhome_launch_profile_is_default_profile() {
        let m = Manifest::parse(r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "defaultProfile": "whale",
            "profiles": {"whale": {"bundles": ["b"]}}
        }"#).unwrap();
        assert_eq!(m.launch_profile().as_deref(), Some("whale"));
    }

    #[test]
    fn accepts_v3_container() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-ok");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":3}"#, true);
        let ex = extract(&p).unwrap();
        assert_eq!(ex.marker.version, 3);
        assert_eq!(ex.marker.format, "dspack");
        assert!(ex.dir.join("manifest.json").is_file());
        ex.cleanup();
        assert!(!ex.dir.exists(), "cleanup 后临时目录应被删除");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

