//! 更新提醒（C1，v0.2.6）。
//!
//! 只做「提醒」：查 GitHub Releases 最新 tag，与当前版本比对，有新版则告知用户
//! （打开 Release 页 / 下载直链）。**不做**自动下载安装（那是 C2，另行评估）。
//!
//! 设计：
//! - 网络请求放后台线程，绝不阻塞启动 / UI
//! - 任何失败都静默（更新检查不该打扰用户）
//! - 结果缓存到进程内，避免每次启动都请求

use serde::Serialize;

/// 更新检查结果（前端据此渲染提示）。
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// 是否有新版本
    pub has_update: bool,
    /// 当前版本
    pub current: String,
    /// 最新版本（查到才有）
    pub latest: Option<String>,
    /// Release 页面 URL
    pub release_url: Option<String>,
    /// 直接下载 exe 的 URL（Release 资产）
    pub download_url: Option<String>,
}

/// GitHub 仓库（用于查询 Releases）
const REPO: &str = "K1-lihongrong/dsh-multiver";

/// 查询最新 Release，与当前版本比对。
///
/// 网络 / 解析失败时返回 `has_update: false`（静默失败，不打扰用户）。
pub fn check() -> UpdateInfo {
    let current = crate::cli::VERSION.to_string();
    let none = UpdateInfo {
        has_update: false,
        current: current.clone(),
        latest: None,
        release_url: None,
        download_url: None,
    };

    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("dsh-multiver")
        .build()
    {
        Ok(c) => c,
        Err(_) => return none,
    };
    let resp = match client.get(&url).send() {
        Ok(r) => r,
        Err(_) => return none,
    };
    if !resp.status().is_success() {
        return none;
    }
    let body: serde_json::Value = match resp.json() {
        Ok(v) => v,
        Err(_) => return none,
    };

    let tag = body.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    let html_url = body.get("html_url").and_then(|v| v.as_str()).map(String::from);
    // 从 assets 里找 exe 资产
    let download_url = body
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|a| {
                let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.ends_with(".exe") {
                    a.get("browser_download_url").and_then(|v| v.as_str()).map(String::from)
                } else {
                    None
                }
            })
        });

    let latest = tag.trim_start_matches('v').to_string();
    if latest.is_empty() {
        return none;
    }
    let has_update = is_newer(&latest, &current);
    UpdateInfo {
        has_update,
        current,
        latest: Some(latest),
        release_url: html_url,
        download_url,
    }
}

/// 语义化版本比较：`candidate` 是否比 `current` 新。
///
/// 只比较数字段（忽略 pre-release 后缀差异，保守判断：同段数字相等则视为不更新）。
/// 无法解析时返回 false（宁可不提示，也不错报）。
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(s: &str) -> Option<Vec<u64>> {
        s.split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect()
    }
    let (Some(c), Some(cur)) = (parse(candidate), parse(current)) else {
        return false;
    };
    for i in 0..c.len().max(cur.len()) {
        let a = c.get(i).copied().unwrap_or(0);
        let b = cur.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_major_minor_patch() {
        assert!(is_newer("0.2.6", "0.2.5"));
        assert!(is_newer("0.3.0", "0.2.5"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.5", "0.2.5"));
        assert!(!is_newer("0.2.4", "0.2.5"));
    }

    #[test]
    fn ignores_prerelease_suffix() {
        // 0.2.6-rc.1 的数字段是 0.2.6 → 比 0.2.5 新
        assert!(is_newer("0.2.6-rc.1", "0.2.5"));
        // 同数字段（忽略后缀）视为不更新，避免"每次都提示"
        assert!(!is_newer("0.2.5-rc.1", "0.2.5"));
    }

    #[test]
    fn garbage_is_not_newer() {
        assert!(!is_newer("abc", "0.2.5"));
        assert!(!is_newer("", "0.2.5"));
    }
}
