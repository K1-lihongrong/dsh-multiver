//! 推荐插件列表（设计见 docs/交互设计约定.md §2.8）。
//!
//! 数据源分三层：
//!   1. 编译期嵌入的副本（`include_str!`）—— 首次打开时的初始内容
//!   2. 磁盘缓存 `<数据根>/cache/plugins.json` —— 上次成功拉取的内容
//!   3. 远端 `docs/plugins.json` —— 每次检查更新时的比对源
//!
//! 打开推荐页的流程：
//!   a. `load_local()` 立即返回（磁盘缓存 → 内置），不联网
//!   b. `check_remote()` 后台拉远端，与本地做 sha256 比对，不同则写盘
//!   c. 前端拿到 `updated=true` 后再次 `load_local()` 渲染新内容

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 推荐列表地址（raw 形式，jsDelivr 源据此映射）。
const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/K1-lihongrong/dsh-multiver/main/docs/plugins.json";

/// 编译期嵌入的副本，无磁盘缓存时使用。
const BUILTIN: &str = include_str!("../../docs/plugins.json");

/// 支持的 schema 版本。
const SUPPORTED_VERSION: u32 = 2;

/// 检查更新的节流窗口：窗口内重复打开推荐页不重新检查。
const CHECK_TTL: Duration = Duration::from_secs(30 * 60);

/// 缓存文件名（位于 `<数据根>/cache/`）。
const CACHE_FILE: &str = "plugins.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPlugin {
    pub name: String,
    pub description: String,
    pub category: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    #[serde(
        rename = "minDshVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub min_dsh_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "noNpm", default, skip_serializing_if = "Option::is_none")]
    pub no_npm: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawCatalog {
    version: u32,
    plugins: Vec<CatalogPlugin>,
}

/// 当前展示的列表。
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub plugins: Vec<CatalogPlugin>,
}

/// 一次远端检查的结果。
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// 是否真的发起了检查。false = 节流窗口内跳过。
    pub checked: bool,
    /// 远端内容与本地不同、已写入磁盘缓存。
    pub updated: bool,
    /// 检查失败的原因（网络、解析、写入）。成功或跳过时为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 上次检查时间（进程级节流）。
static LAST_CHECK: Mutex<Option<Instant>> = Mutex::new(None);

fn cache_path(root: &Path) -> PathBuf {
    root.join("cache").join(CACHE_FILE)
}

/// 读本地列表：磁盘缓存优先，缺失或损坏则回退内置副本。
///
/// 不联网，立即返回。
pub fn load_local(root: &Path) -> Result<Catalog, String> {
    if let Ok(text) = std::fs::read_to_string(cache_path(root)) {
        if let Ok(plugins) = parse_catalog(&text) {
            return Ok(Catalog { plugins });
        }
        // 缓存损坏：静默回退内置，下次 check 会重写
    }
    Ok(Catalog {
        plugins: parse_builtin()?,
    })
}

/// 检查远端是否有更新。
///
/// 比对基准：有磁盘缓存就用缓存，否则用内置副本。内容字节的 sha256 不同即视为有更新。
/// 有更新时校验远端能解析、写盘，然后返回 `updated=true`。
///
/// `force=true` 时跳过节流窗口（用户手动刷新）。
pub fn check_remote(root: &Path, force: bool) -> CheckResult {
    if !force && within_throttle() {
        return CheckResult {
            checked: false,
            updated: false,
            error: None,
        };
    }

    let remote = match fetch_remote() {
        Ok(bytes) => bytes,
        Err(e) => {
            mark_checked();
            return CheckResult {
                checked: true,
                updated: false,
                error: Some(e),
            };
        }
    };

    let baseline = baseline_bytes(root);
    if sha256_hex(&remote) == sha256_hex(&baseline) {
        mark_checked();
        return CheckResult {
            checked: true,
            updated: false,
            error: None,
        };
    }

    // 远端内容必须先能解析，否则不写盘——避免把坏数据固化到缓存
    let text = match String::from_utf8(remote.clone()) {
        Ok(t) => t,
        Err(e) => {
            mark_checked();
            return CheckResult {
                checked: true,
                updated: false,
                error: Some(format!("远端列表不是 UTF-8：{}", e)),
            };
        }
    };
    if let Err(e) = parse_catalog(&text) {
        mark_checked();
        return CheckResult {
            checked: true,
            updated: false,
            error: Some(format!("远端列表解析失败：{}", e)),
        };
    }

    let path = cache_path(root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, &remote) {
        mark_checked();
        return CheckResult {
            checked: true,
            updated: false,
            error: Some(format!("写入缓存失败：{}", e)),
        };
    }

    mark_checked();
    CheckResult {
        checked: true,
        updated: true,
        error: None,
    }
}

fn within_throttle() -> bool {
    LAST_CHECK
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|at| at.elapsed() < CHECK_TTL)
        .unwrap_or(false)
}

fn mark_checked() {
    if let Ok(mut g) = LAST_CHECK.lock() {
        *g = Some(Instant::now());
    }
}

/// 比对基准：有磁盘缓存用缓存，否则用内置副本。
fn baseline_bytes(root: &Path) -> Vec<u8> {
    std::fs::read(cache_path(root)).unwrap_or_else(|_| BUILTIN.as_bytes().to_vec())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

fn fetch_remote() -> Result<Vec<u8>, String> {
    let chain = crate::net::catalog_chain();
    crate::net::fetch(CATALOG_URL, &chain, crate::net::Shape::Json).map(|got| got.bytes)
}

fn parse_builtin() -> Result<Vec<CatalogPlugin>, String> {
    parse_catalog(BUILTIN).map_err(|e| format!("内置推荐列表解析失败（打包异常）：{}", e))
}

fn parse_catalog(text: &str) -> Result<Vec<CatalogPlugin>, String> {
    let raw: RawCatalog = serde_json::from_str(text).map_err(|e| format!("解析失败：{}", e))?;
    if raw.version > SUPPORTED_VERSION {
        return Err(format!(
            "推荐列表版本 {} 高于本程序支持的 {}，请更新 dsh-multiver",
            raw.version, SUPPORTED_VERSION
        ));
    }
    Ok(raw
        .plugins
        .into_iter()
        .filter(|p| p.hidden != Some(true))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("catalog-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("cache")).unwrap();
        p
    }

    #[test]
    fn builtin_parses() {
        let plugins = parse_builtin().expect("内置副本应能解析");
        assert!(!plugins.is_empty());
    }

    #[test]
    fn builtin_entries_have_required_fields() {
        for p in parse_builtin().unwrap() {
            assert!(!p.name.trim().is_empty());
            assert!(
                !p.description.trim().is_empty(),
                "description 为空：{}",
                p.name
            );
            assert!(!p.category.trim().is_empty(), "category 为空：{}", p.name);
            assert!(p.repo.starts_with("http"), "repo 非 URL：{}", p.name);
        }
    }

    #[test]
    fn hidden_entries_are_filtered() {
        let text = r#"{"version":2,"plugins":[
            {"name":"a","description":"d","category":"c","repo":"https://x/y"},
            {"name":"b","description":"d","category":"c","repo":"https://x/y","hidden":true}
        ]}"#;
        let plugins = parse_catalog(text).unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "a");
    }

    #[test]
    fn newer_schema_version_is_rejected() {
        let err = parse_catalog(r#"{"version":99,"plugins":[]}"#).unwrap_err();
        assert!(err.contains("版本"));
    }

    #[test]
    fn optional_fields_are_preserved() {
        let text = r#"{"version":2,"plugins":[
            {"name":"a","description":"d","category":"c","repo":"https://x/y",
             "spec":"github:o/r","minDshVersion":"0.2.0","tags":["featured"],"noNpm":true}
        ]}"#;
        let p = &parse_catalog(text).unwrap()[0];
        assert_eq!(p.spec.as_deref(), Some("github:o/r"));
        assert_eq!(p.min_dsh_version.as_deref(), Some("0.2.0"));
        assert_eq!(p.tags.as_ref().unwrap(), &vec!["featured".to_string()]);
        assert_eq!(p.no_npm, Some(true));
    }

    #[test]
    fn load_local_falls_back_to_builtin_without_cache() {
        let root = tmp_root("no-cache");
        let c = load_local(&root).unwrap();
        assert!(!c.plugins.is_empty());
    }

    #[test]
    fn load_local_uses_cache_when_present() {
        let root = tmp_root("with-cache");
        let custom = r#"{"version":2,"plugins":[
            {"name":"only-one","description":"d","category":"c","repo":"https://x/y"}
        ]}"#;
        std::fs::write(cache_path(&root), custom).unwrap();
        let c = load_local(&root).unwrap();
        assert_eq!(c.plugins.len(), 1);
        assert_eq!(c.plugins[0].name, "only-one");
    }

    #[test]
    fn load_local_ignores_corrupt_cache() {
        let root = tmp_root("corrupt");
        std::fs::write(cache_path(&root), "not json").unwrap();
        let c = load_local(&root).unwrap();
        // 缓存损坏 → 回退内置（内置条目数 > 1）
        assert!(c.plugins.len() > 1);
    }

    #[test]
    fn sha256_hex_is_stable() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
