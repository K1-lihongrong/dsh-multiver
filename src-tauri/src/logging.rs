//! 统一日志模块（阶段一：可观测性）。
//!
//! 设计原则（贴合本项目「零依赖、便携、自包含」的哲学）：
//! - **零外部依赖**：不引入 tauri-plugin-log / chrono 等，手写最小实现
//! - **失败静默**：任何日志写入失败都不得影响主流程
//! - **时间戳**：UTC 的 yyyyMMdd-HHmmss，便于排序与人工阅读
//!
//! 日志落点（数据根 <root>/logs/）：
//! - app.log           —— 主进程错误、前端错误汇总
//! - frontend.log      —— WebView 侧 JS 错误
//! - session-*.log     —— 每次启动 dsh 的完整 stdout+stderr（见 launcher.rs）
//! - dsh-stderr.log    —— 历史沿用，保留
//! - launch-error.log / maintenance.log —— 历史沿用，保留

use std::io::Write;
use std::path::{Path, PathBuf};

/// 当前时间戳（unix 秒）。系统时间早于 epoch 时返回 0。
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 把 unix 秒格式化为 UTC 的 `yyyyMMdd-HHmmss`。
pub fn stamp_compact(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", y, mo, d, h, mi, s)
}

/// unix 秒 → (年,月,日,时,分,秒)，UTC。
/// 日期换算用 Howard Hinnant 的 civil_from_days 算法（公知、无依赖）。
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = (secs % 86400) as u32;
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h, mi, s)
}

/// 确保目录存在（失败静默）。
pub fn ensure_dir(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
}

/// 把一行消息追加写入 `dir/file`，前缀 UTC 时间戳。失败静默。
pub fn write_line(dir: &Path, file: &str, msg: &str) {
    ensure_dir(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(file))
    {
        let _ = writeln!(f, "[{}] {}", stamp_compact(now_secs()), msg);
    }
}

/// 打开一个可追加的文件句柄（用于会话日志持续写入）。失败返回 None。
pub fn open_append(path: &Path) -> Option<std::fs::File> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent);
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

/// 限制目录内「前缀 prefix + 后缀 suffix」的旧文件数量：按文件名排序，保留最新 keep 个。
/// 用于防止会话日志无限增长。失败静默。
pub fn prune_old(dir: &Path, prefix: &str, suffix: &str, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(prefix) && n.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    let remove = files.len() - keep;
    for p in files.into_iter().take(remove) {
        let _ = std::fs::remove_file(p);
    }
}
