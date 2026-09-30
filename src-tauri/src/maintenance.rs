//! 后台维护任务：清理孤儿 webview 目录、回收 pnpm store。
//!
//! 都在独立线程里跑，不阻塞 UI；store prune 额外做了延迟 + 7 天节流 + 低优先级。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 距上次 store prune 的最小间隔（秒）：7 天。
pub const PRUNE_INTERVAL_SECS: u64 = 7 * 24 * 3600;

/// 当前 unix 时间戳（秒）。
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 清理孤立的 webview 目录：`webview/` 下存在、但对应版本已不存在的。
///
/// 只删目录（WebView2 缓存，删了无害）。返回被删除的目录名。
/// 判定「存活」以 versions/ 下的**目录名**为准（存在即保留，避免误删安装中的）。
pub fn cleanup_orphan_webviews(versions_dir: &Path, webview_dir: &Path) -> Vec<String> {
    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(versions_dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                alive.insert(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    let mut removed = Vec::new();
    if let Ok(rd) = std::fs::read_dir(webview_dir) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if !alive.contains(&name) && std::fs::remove_dir_all(e.path()).is_ok() {
                removed.push(name);
            }
        }
    }
    removed
}

/// 运行 `pnpm store prune`，回收未被引用的包。
///
/// - 低优先级（IDLE_PRIORITY_CLASS），让用户操作优先
/// - 绑定 Job Object，管理器退出时子进程被 OS 清理，不留孤儿
pub fn run_store_prune(store: &Path, cache: &Path, state: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const IDLE_PRIORITY_CLASS: u32 = 0x00000040;
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg("pnpm").arg("store").arg("prune");
        c.creation_flags(CREATE_NO_WINDOW | IDLE_PRIORITY_CLASS);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("pnpm");
        c.arg("store").arg("prune");
        c
    };

    cmd.arg(format!("--config.store-dir={}", store.to_string_lossy()))
        .arg(format!("--config.cache-dir={}", cache.to_string_lossy()))
        .arg(format!("--config.state-dir={}", state.to_string_lossy()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("启动 store prune 失败: {}", e))?;
    // 绑定 Job：管理器退出即杀，避免孤儿
    let _job = crate::jobobj::assign_to_new_job(&child);
    let status = child.wait();
    if matches!(status, Ok(s) if s.success()) {
        Ok(())
    } else {
        Err(format!("store prune 退出异常: {:?}", status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("dsh-multiver-maint-{}", name));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn cleanup_removes_only_orphans() {
        let base = tmp("webview");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.1")).unwrap(); // 孤儿

        let removed = cleanup_orphan_webviews(&versions, &webview);
        assert_eq!(removed, vec!["0.1.7-rc.1"]);
        assert!(webview.join("0.1.7-rc.2").exists(), "存活版本不应被删");
        assert!(!webview.join("0.1.7-rc.1").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_noop_when_all_alive() {
        let base = tmp("webview2");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("v1")).unwrap();
        std::fs::create_dir_all(&webview).unwrap();
        std::fs::create_dir_all(webview.join("v1")).unwrap();
        let removed = cleanup_orphan_webviews(&versions, &webview);
        assert!(removed.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

}
