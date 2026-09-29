//! 后台维护任务：清理孤儿 webview 目录、回收 pnpm store。
//!
//! 都在独立线程里跑，不阻塞 UI；store prune 额外做了延迟 + 7 天节流 + 低优先级。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 距上次 store prune 的最小间隔（秒）：7 天。
pub const PRUNE_INTERVAL_SECS: u64 = 7 * 24 * 3600;

/// 记录上次 prune 时间的文件（放数据根下，换根自动重置）。
const LAST_PRUNE_FILE: &str = ".dsh-multiver-last-prune";

/// 清理孤立的 webview 目录：`webview/` 下存在、但对应版本/实例已不存在的。
///
/// 只删目录（WebView2 缓存，删了无害）。返回被删除的目录名。
/// 判定「存活」以 versions/ 与 modpacks/ 下的**目录名**为准（存在即保留，避免误删安装中的）。
pub fn cleanup_orphan_webviews(
    versions_dir: &Path,
    modpacks_dir: &Path,
    webview_dir: &Path,
) -> Vec<String> {
    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    for dir in [versions_dir, modpacks_dir] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    alive.insert(e.file_name().to_string_lossy().to_string());
                }
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

/// 是否应当运行 store prune（距上次 >= 7 天，或从未跑过）。
pub fn should_prune_store(root: &Path) -> bool {
    let f = root.join(LAST_PRUNE_FILE);
    match std::fs::metadata(&f).and_then(|m| m.modified()) {
        Ok(t) => match SystemTime::now().duration_since(t) {
            Ok(d) => d.as_secs() >= PRUNE_INTERVAL_SECS,
            Err(_) => true, // 时间倒退，保守地跑一次
        },
        Err(_) => true, // 无记录 → 首次
    }
}

/// 记录「刚刚 prune 过」。
pub fn mark_pruned(root: &Path) {
    let f = root.join(LAST_PRUNE_FILE);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(f, now.to_string());
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
        let modpacks = base.join("modpacks");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(modpacks.join("modpack-a-1.0.0")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(webview.join("modpack-a-1.0.0")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.1")).unwrap(); // 孤儿
        std::fs::create_dir_all(webview.join("modpack-gone-1.0.0")).unwrap(); // 孤儿

        let removed = cleanup_orphan_webviews(&versions, &modpacks, &webview);
        let mut r = removed.clone();
        r.sort();
        assert_eq!(r, vec!["0.1.7-rc.1", "modpack-gone-1.0.0"]);
        assert!(webview.join("0.1.7-rc.2").exists(), "存活版本不应被删");
        assert!(webview.join("modpack-a-1.0.0").exists(), "存活实例不应被删");
        assert!(!webview.join("0.1.7-rc.1").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_noop_when_all_alive() {
        let base = tmp("webview2");
        let versions = base.join("versions");
        let modpacks = base.join("modpacks");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("v1")).unwrap();
        std::fs::create_dir_all(&webview).unwrap();
        std::fs::create_dir_all(webview.join("v1")).unwrap();
        let removed = cleanup_orphan_webviews(&versions, &modpacks, &webview);
        assert!(removed.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prune_schedule_first_time_true() {
        let base = tmp("prune1");
        assert!(should_prune_store(&base), "无记录 → 应跑");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prune_schedule_marked_then_false() {
        let base = tmp("prune2");
        mark_pruned(&base);
        assert!(!should_prune_store(&base), "刚跑过 → 不应再跑");
        assert!(base.join(LAST_PRUNE_FILE).exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}