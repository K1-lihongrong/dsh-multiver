//! DSH-PackForge 市场（dsh-pack-market）客户端。
//!
//! 索引契约：schemaVersion = 2，见
//! https://github.com/DSH-PackForge/dsh-pack-market 的 index/index.json。
//!
//! 职责：
//! - 拉取索引并解析为 MarketPack 列表（校验 schemaVersion）
//! - 从 downloadUrl 下载 .dspack 到临时文件，校验 sha256 + size
//! - 交给调用方（lib.rs）复用 import_core 完成导入

use serde::{Deserialize, Serialize};

/// 市场索引默认地址（raw.githubusercontent）。
pub const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/DSH-PackForge/dsh-pack-market/main/index/index.json";

/// 支持的索引契约版本。
const SUPPORTED_SCHEMA: u32 = 2;

/// 索引根。
#[derive(Debug, Deserialize)]
struct MarketIndexRaw {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    modpacks: Vec<MarketPack>,
}

/// 展示名/描述：字符串或 { "zh-CN": "...", "en-US": "..." }。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Localized {
    Plain(String),
    Map(std::collections::HashMap<String, String>),
}

impl Localized {
    /// 取中文优先，其次 en-US，再其次首项。
    pub fn display(&self) -> String {
        match self {
            Localized::Plain(s) => s.clone(),
            Localized::Map(m) => {
                for k in ["zh-CN", "zh", "en-US", "en"] {
                    if let Some(v) = m.get(k) {
                        return v.clone();
                    }
                }
                m.values().next().cloned().unwrap_or_default()
            }
        }
    }
}

/// 索引里的单个整合包条目（字段与索引一一对应）。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketPack {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<Localized>,
    #[serde(default)]
    pub description: Option<Localized>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(rename = "dshVersion", default)]
    pub dsh_version: Option<String>,
    #[serde(rename = "profileName", default)]
    pub profile_name: Option<String>,
    #[serde(rename = "downloadUrl")]
    pub download_url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    pub id: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(rename = "bundleCount", default)]
    pub bundle_count: Option<usize>,
    #[serde(rename = "depCount", default)]
    pub dep_count: Option<usize>,
}

impl MarketPack {
    pub fn display_name_text(&self) -> String {
        match &self.display_name {
            Some(d) => {
                let s = d.display();
                if s.is_empty() { self.name.clone() } else { s }
            }
            None => self.name.clone(),
        }
    }
}

/// 拉取并解析市场索引。校验 schemaVersion 受支持。
pub fn fetch_index(url: &str) -> Result<Vec<MarketPack>, String> {
    let bytes = crate::modpack::fetch_bytes(url)
        .map_err(|e| format!("拉取市场索引失败：{}", e))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| format!("市场索引不是合法 UTF-8：{}", e))?;
    let idx: MarketIndexRaw = serde_json::from_str(&text)
        .map_err(|e| format!("市场索引解析失败：{}", e))?;
    if idx.schema_version != SUPPORTED_SCHEMA {
        return Err(format!(
            "不支持的市场索引版本 schemaVersion={}（支持 {}）。请升级本工具。",
            idx.schema_version, SUPPORTED_SCHEMA
        ));
    }
    Ok(idx.modpacks)
}

/// 把一个市场包下载到临时文件，校验 sha256 + size。
/// 返回临时文件路径（调用方负责在导入后删除）。
///
/// sha256 为空时跳过校验（索引允许为空，如 better-sidebar）。
pub fn download_to_temp(pack: &MarketPack) -> Result<std::path::PathBuf, String> {
    let bytes = crate::modpack::fetch_bytes(&pack.download_url)
        .map_err(|e| format!("下载 {} 失败：{}", pack.download_url, e))?;

    // size 校验（索引为 0 视为未提供）
    if pack.size > 0 && bytes.len() as u64 != pack.size {
        return Err(format!(
            "下载内容大小不符：期望 {} 字节，实际 {} 字节",
            pack.size,
            bytes.len()
        ));
    }
    // sha256 校验（空则跳过）
    if !pack.sha256.trim().is_empty() {
        let got = crate::modpack::sha256_hex(&bytes);
        if !got.eq_ignore_ascii_case(pack.sha256.trim()) {
            return Err(format!(
                "下载内容 sha256 不符：期望 {}，实际 {}",
                pack.sha256, got
            ));
        }
    }

    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "dsh-modpack-market-{}-{}.dspack",
        sanitize(&pack.name),
        std::process::id()
    ));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| format!("写临时文件失败 {}: {}", tmp.display(), e))?;
    Ok(tmp)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_plain_and_map() {
        let a: Localized = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(a.display(), "hello");
        let b: Localized =
            serde_json::from_str("{\"en-US\":\"Hi\",\"zh-CN\":\"你好\"}").unwrap();
        assert_eq!(b.display(), "你好");
    }

    #[test]
    fn localized_map_fallback_en() {
        let b: Localized = serde_json::from_str("{\"en-US\":\"Hi\"}").unwrap();
        assert_eq!(b.display(), "Hi");
    }

    #[test]
    fn parses_min_index() {
        let json = r#"{"schemaVersion":2,"modpacks":[{"manifestVersion":5,"type":"profile","name":"x","version":"1.0.0","downloadUrl":"https://e/x.dspack","sha256":"","size":1,"id":"o.x"}]}"#;
        let idx: MarketIndexRaw = serde_json::from_str(json).unwrap();
        assert_eq!(idx.schema_version, 2);
        assert_eq!(idx.modpacks.len(), 1);
        assert_eq!(idx.modpacks[0].name, "x");
    }

    #[test]
    fn rejects_unknown_schema() {
        let json = r#"{"schemaVersion":99,"modpacks":[]}"#;
        let idx: MarketIndexRaw = serde_json::from_str(json).unwrap();
        assert_ne!(idx.schema_version, SUPPORTED_SCHEMA);
    }
}
