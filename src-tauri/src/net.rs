//! 带镜像回退的下载（设计见 docs/交互设计约定.md §2.8）。
//!
//! 项目依赖 GitHub 的地方有三处：multiver 自身更新、插件部署/暂存更新、推荐列表。
//! 国内网络下 GitHub 的可达性不稳定，且 `github.com` 通不代表 `raw.githubusercontent.com` 通
//! （2026-10-30 实测：前者 2 秒通，后者 10 秒超时）。
//!
//! 本模块提供统一的回退链：直连优先，失败按序切镜像，任一成功即返回。
//! 失败判定不只看状态码——公共反代常返回 200 但内容是 HTML 错误页，
//! 因此下完后还要校验响应形状。

use std::time::Duration;

/// 一个下载源。
pub struct Source {
    /// 源名，用于日志与「上次成功」记忆。
    pub name: &'static str,
    /// 把原始 URL 变换为本源的 URL。None 表示直连。
    pub rewrite: Option<fn(&str) -> String>,
    /// 本源的超时。
    pub timeout: Duration,
}

impl Source {
    pub const fn direct(timeout_secs: u64) -> Self {
        Self {
            name: "direct",
            rewrite: None,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub const fn proxy(name: &'static str, rewrite: fn(&str) -> String, timeout_secs: u64) -> Self {
        Self {
            name,
            rewrite: Some(rewrite),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    fn apply(&self, url: &str) -> String {
        match self.rewrite {
            Some(f) => f(url),
            None => url.to_string(),
        }
    }
}

/// 响应内容的预期形状。
#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    /// 可执行文件：PE（MZ）或 ELF。
    Binary,
    /// JSON：以 { 或 [ 开头。
    Json,
}

/// 一次下载的结果。
pub struct Downloaded {
    pub bytes: Vec<u8>,
    /// 实际使用的源名。
    pub source: &'static str,
}

fn looks_like(shape: Shape, body: &[u8]) -> bool {
    if body.is_empty() {
        return false;
    }
    match shape {
        Shape::Binary => {
            let pe = body.len() >= 2 && body[0] == 0x4d && body[1] == 0x5a;
            let elf = body.len() >= 4
                && body[0] == 0x7f
                && body[1] == 0x45
                && body[2] == 0x4c
                && body[3] == 0x46;
            pe || elf
        }
        Shape::Json => {
            let head: String = String::from_utf8_lossy(&body[..body.len().min(64)]).into_owned();
            let t = head.trim_start();
            t.starts_with('{') || t.starts_with('[')
        }
    }
}

/// 按顺序尝试各源下载，返回第一个成功且形状符合预期的结果。
///
/// 全部失败时返回聚合的错误描述（含每个源的失败原因），便于排查。
pub fn fetch(url: &str, chain: &[Source], shape: Shape) -> Result<Downloaded, String> {
    let mut errors: Vec<String> = Vec::new();
    for src in chain {
        let target = src.apply(url);
        let client = match crate::runtime::blocking_client_ua(src.timeout, Some("dsh-multiver")) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: 创建客户端失败 {}", src.name, e));
                continue;
            }
        };
        match client.get(&target).send() {
            Ok(resp) => {
                if !resp.status().is_success() {
                    errors.push(format!("{}: HTTP {}", src.name, resp.status()));
                    continue;
                }
                match resp.bytes() {
                    Ok(body) => {
                        if !looks_like(shape, &body) {
                            errors
                                .push(format!("{}: 响应形状不符（可能是镜像的错误页）", src.name));
                            continue;
                        }
                        return Ok(Downloaded {
                            bytes: body.to_vec(),
                            source: src.name,
                        });
                    }
                    Err(e) => errors.push(format!("{}: 读取响应失败 {}", src.name, e)),
                }
            }
            Err(e) => {
                let reason = if e.is_timeout() {
                    "超时".to_string()
                } else {
                    e.to_string()
                };
                errors.push(format!("{}: {}", src.name, reason));
            }
        }
    }
    Err(format!("所有下载源均失败：{}", errors.join("；")))
}

/// exe 下载的默认源链：直连优先，失败切 gh-proxy。
pub fn exe_chain() -> Vec<Source> {
    vec![Source::direct(20), Source::proxy("gh-proxy", gh_proxy, 20)]
}

/// 推荐列表的源链：直连超时短（非关键路径，拉不到就用内置副本）。
pub fn catalog_chain() -> Vec<Source> {
    vec![
        Source::direct(5),
        Source::proxy("gh-proxy", gh_proxy, 15),
        Source::proxy("jsdelivr", jsdelivr, 15),
    ]
}

fn gh_proxy(url: &str) -> String {
    format!("https://gh-proxy.com/{}", url)
}

/// GitHub raw → jsDelivr。
///
/// `https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`
/// → `https://cdn.jsdelivr.net/gh/<owner>/<repo>@<ref>/<path>`
///
/// 非 raw 形式时原样返回（该源会因无法访问而失败，等效跳过）。
fn jsdelivr(url: &str) -> String {
    const PREFIX: &str = "https://raw.githubusercontent.com/";
    let Some(rest) = url.strip_prefix(PREFIX) else {
        return url.to_string();
    };
    let mut parts = rest.splitn(4, '/');
    let (Some(owner), Some(repo), Some(git_ref), Some(path)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return url.to_string();
    };
    format!(
        "https://cdn.jsdelivr.net/gh/{}/{}@{}/{}",
        owner, repo, git_ref, path
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_rejects_html_as_binary() {
        assert!(!looks_like(Shape::Binary, b"<!DOCTYPE html>"));
    }

    #[test]
    fn looks_like_accepts_pe_and_elf() {
        assert!(looks_like(Shape::Binary, &[0x4d, 0x5a, 0x00]));
        assert!(looks_like(Shape::Binary, &[0x7f, 0x45, 0x4c, 0x46]));
    }

    #[test]
    fn looks_like_rejects_empty() {
        assert!(!looks_like(Shape::Json, b""));
        assert!(!looks_like(Shape::Binary, b""));
    }

    #[test]
    fn looks_like_json_accepts_object_and_array() {
        assert!(looks_like(Shape::Json, b"  {\"a\":1}"));
        assert!(looks_like(Shape::Json, b"[]"));
        assert!(!looks_like(Shape::Json, b"<html>"));
    }

    #[test]
    fn jsdelivr_maps_raw_url() {
        assert_eq!(
            jsdelivr("https://raw.githubusercontent.com/o/r/main/docs/x.json"),
            "https://cdn.jsdelivr.net/gh/o/r@main/docs/x.json"
        );
    }

    #[test]
    fn jsdelivr_passes_through_non_raw() {
        let u = "https://github.com/o/r/releases/download/v1/x.exe";
        assert_eq!(jsdelivr(u), u);
    }

    #[test]
    fn source_apply_respects_rewrite() {
        let d = Source::direct(5);
        assert_eq!(d.apply("https://x"), "https://x");
        let p = Source::proxy("gh-proxy", gh_proxy, 5);
        assert_eq!(p.apply("https://x"), "https://gh-proxy.com/https://x");
    }
}
