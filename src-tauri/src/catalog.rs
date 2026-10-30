//! 推荐插件列表（设计见 docs/交互设计约定.md §2.8）。
//!
//! 列表源是仓库的 `docs/plugins.json`，运行时拉取。拉取失败时回退到编译期嵌入的副本，
//! 保证断网或镜像全挂时推荐页仍有内容（标注「可能不是最新」）。
//!
//! 插件不缓存到磁盘：列表很小（几 KB），每次打开推荐页拉一次即可，
//! 进程内缓存 30 分钟避免频繁请求。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 推荐列表地址（raw 形式，jsDelivr 源据此映射）。
const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/K1-lihongrong/dsh-multiver/main/docs/plugins.json";

/// 编译期嵌入的副本，拉取失败时使用。
const BUILTIN: &str = include_str!("../../docs/plugins.json");

/// 支持的 schema 版本。
const SUPPORTED_VERSION: u32 = 2;

/// 进程内缓存有效期。
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

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

/// 返回给前端的列表。
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub plugins: Vec<CatalogPlugin>,
    /// 实际使用的源：网络源名（direct / gh-proxy / jsdelivr）或 "builtin"。
    pub source: String,
    /// 是否来自内置副本（即拉取失败）。
    pub stale: bool,
    /// 拉取失败时的原因，供排查与提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

struct CacheEntry {
    at: Instant,
    value: Catalog,
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

/// 拉取推荐列表。force=true 时忽略缓存。
///
/// 拉取、解析、校验任一环节失败 → 回退内置副本（stale=true）并记录失败原因。
/// 内置副本本身解析失败（打包异常）→ 返回错误。
pub fn load(force: bool) -> Result<Catalog, String> {
    if !force {
        if let Ok(guard) = CACHE.lock() {
            if let Some(e) = guard.as_ref() {
                if e.at.elapsed() < CACHE_TTL {
                    return Ok(e.value.clone());
                }
            }
        }
    }

    let result = match fetch_remote() {
        Ok((plugins, source)) => Catalog {
            plugins,
            source,
            stale: false,
            failure: None,
        },
        Err(e) => {
            let plugins = parse_builtin()?;
            Catalog {
                plugins,
                source: "builtin".to_string(),
                stale: true,
                failure: Some(e),
            }
        }
    };

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CacheEntry {
            at: Instant::now(),
            value: result.clone(),
        });
    }
    Ok(result)
}

fn fetch_remote() -> Result<(Vec<CatalogPlugin>, String), String> {
    let chain = crate::net::catalog_chain();
    let got = crate::net::fetch(CATALOG_URL, &chain, crate::net::Shape::Json)?;
    let text = String::from_utf8(got.bytes).map_err(|e| format!("推荐列表不是 UTF-8：{}", e))?;
    let plugins = parse_catalog(&text)?;
    Ok((plugins, got.source.to_string()))
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

    #[test]
    fn builtin_parses() {
        let plugins = parse_builtin().expect("内置副本应能解析");
        assert!(!plugins.is_empty(), "内置副本不应为空");
    }

    #[test]
    fn builtin_entries_have_required_fields() {
        let plugins = parse_builtin().unwrap();
        for p in &plugins {
            assert!(!p.name.trim().is_empty(), "name 不能为空");
            assert!(
                !p.description.trim().is_empty(),
                "description 不能为空：{}",
                p.name
            );
            assert!(
                !p.category.trim().is_empty(),
                "category 不能为空：{}",
                p.name
            );
            assert!(p.repo.starts_with("http"), "repo 必须是 URL：{}", p.name);
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
        let text = r#"{"version":99,"plugins":[]}"#;
        let err = parse_catalog(text).unwrap_err();
        assert!(err.contains("版本"), "应提示版本过高：{}", err);
    }

    #[test]
    fn optional_fields_are_preserved() {
        let text = r#"{"version":2,"plugins":[
            {"name":"a","description":"d","category":"c","repo":"https://x/y",
             "spec":"github:o/r","minDshVersion":"0.2.0","tags":["same-author"],"noNpm":true}
        ]}"#;
        let p = &parse_catalog(text).unwrap()[0];
        assert_eq!(p.spec.as_deref(), Some("github:o/r"));
        assert_eq!(p.min_dsh_version.as_deref(), Some("0.2.0"));
        assert_eq!(p.tags.as_ref().unwrap(), &vec!["same-author".to_string()]);
        assert_eq!(p.no_npm, Some(true));
    }
}
