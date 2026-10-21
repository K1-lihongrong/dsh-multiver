//! 版本列表与排序（D5 拆分：原 versions.rs 的 list 相关）。
use serde::Serialize;
use std::path::Path;

/// 单个已安装版本的信息
#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    /// 版本号，如 0.1.5
    pub version: String,
    /// 版本目录的绝对路径
    pub path: String,
    /// 是否为当前默认版本
    pub is_default: bool,
    /// 是否开启了数据隔离
    pub isolated: bool,
    /// 安装日期（版本目录创建时间，格式 YYYY-MM-DD；读不到则为空）
    pub installed_at: String,
    /// 是否使用共享 home（= 未开启隔离）。
    /// 非隔离版本共用 `<根>/home`，多版本混用可能导致插件/依赖版本错配
    /// （如 dsh 0.1.7 的 open-in-app 失效会连累旧版本），UI 应提示。
    pub shared_home: bool,
}

/// 列出已安装的版本（扫描 versions 目录）
pub fn list(
    versions_dir: &Path,
    default_version: Option<&str>,
    isolated: &[String],
) -> Vec<VersionInfo> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(versions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // 必须含有 node_modules 才算有效版本
            if !path.join("node_modules").exists() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().to_string();
            let is_default = default_version == Some(version.as_str());
            let is_isolated = isolated.iter().any(|v| v == &version);
            let installed_at = dir_created_date(&path);
            result.push(VersionInfo {
                version,
                path: path.to_string_lossy().to_string(),
                is_default,
                isolated: is_isolated,
                installed_at,
                shared_home: !is_isolated,
            });
        }
    }
    // 按版本号排序
    result.sort_by(|a, b| version_cmp(&a.version, &b.version));
    result
}

/// 读取目录创建时间并格式化为 YYYY-MM-DD（本地时区）。
/// 读不到（权限/文件系统不支持）则返回空字符串。
fn dir_created_date(path: &Path) -> String {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    let created = match meta.created() {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    // 转成本地时间：用 SystemTime -> 时区偏移需要外部 crate；
    // 这里用 std 自带方式（Windows 上 created() 已是本地时间的 UTC 表示），
    // 简化为按 UTC+8 处理（面向国内用户），偏移由 chrono 不存在，故用手工换算。
    format_date_local(created)
}

/// 把 SystemTime 格式化为本地日期（YYYY-MM-DD）。
/// 仅用于展示"安装日期"，对时区误差不敏感；按东八区（UTC+8）处理。
fn format_date_local(t: std::time::SystemTime) -> String {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return String::new(),
    };
    let local = secs + 8 * 3600; // UTC+8
    let days = local.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 由"自 1970-01-01 起的天数"换算为 (年, 月, 日)。
/// 算法来自 Howard Hinnant 的 civil_from_days。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 版本号比较：把数字段拆开按数值比较
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<i64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|x| !x.is_empty())
            .filter_map(|x| x.parse::<i64>().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}
