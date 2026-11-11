//! 后台维护任务：清理孤儿 webview 目录、回收 pnpm store。
//!
//! 都在独立线程里跑，不阻塞 UI；store prune 额外做了延迟 + 7 天节流 + 低优先级。

use crate::config::Dirs;
use crate::domain::VersionId;
use std::path::{Path, PathBuf};
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

/// 把旧布局 `webview/<版本>` 迁到 `webview/<版本>/_shared`。
///
/// 旧布局的 WebView2 数据直接在版本目录下；新布局要求 `_shared` 子目录。
/// 三步同卷改名（rename base -> legacy -> base/_shared），失败时保留旧目录并写日志，
/// 接受首次打开丢失 cookie/偏好。
pub fn ensure_webview_layout(dirs: &Dirs, version: &VersionId) -> PathBuf {
    let base = dirs.webview.join(version.as_str());
    let shared = base.join("_shared");
    if shared.exists() {
        return base;
    }
    if !base.exists() {
        let _ = std::fs::create_dir_all(&shared);
        return base;
    }

    let legacy = dirs
        .webview
        .join(format!("{}.legacy-{}", version, std::process::id()));
    if std::fs::rename(&base, &legacy).is_err() {
        let _ = std::fs::create_dir_all(&shared);
        crate::logging::write_line(
            &dirs.root.join("logs"),
            "app.log",
            &format!("迁移 WebView2 布局失败（rename {version}）：改按新布局打开"),
        );
        return base;
    }
    let _ = std::fs::create_dir_all(&base);
    if std::fs::rename(&legacy, &shared).is_err() {
        let _ = std::fs::create_dir_all(&shared);
        crate::logging::write_line(
            &dirs.root.join("logs"),
            "app.log",
            &format!(
                "迁移 WebView2 布局失败（legacy -> _shared：{version}）：旧目录保留在 {}",
                legacy.display()
            ),
        );
        return base;
    }
    base
}

fn alive_versions(versions_dir: &Path) -> std::collections::HashSet<String> {
    let mut alive = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(versions_dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                alive.insert(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    alive
}

/// 新布局的判定锚点：存在 `_shared` 或任一已挂载实例子目录。
/// 旧布局（文件直接位于版本目录）不参与子目录清理，避免误删 WebView2 profile。
fn has_new_layout(base: &Path, mounted: &[String]) -> bool {
    base.join("_shared").exists() || mounted.iter().any(|id| base.join(id).exists())
}

/// 清理由版本级/实例级构成的孤儿 webview 缓存。
///
/// - 顶层版本不存在 -> 删整个 `webview/<版本>`；
/// - 版本存在 -> 只删未挂载的实例子目录，保留 `_shared`；
/// - 旧布局（无 `_shared`、无已挂载实例子目录）整目录跳过。
pub fn cleanup_orphan_webviews(
    versions_dir: &Path,
    webview_dir: &Path,
    mounted: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<String> {
    let alive = alive_versions(versions_dir);
    let mut removed = Vec::new();
    if let Ok(rd) = std::fs::read_dir(webview_dir) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if !alive.contains(&name) {
                if std::fs::remove_dir_all(e.path()).is_ok() {
                    removed.push(name);
                }
                continue;
            }
            let ids = mounted.get(&name).cloned().unwrap_or_default();
            let base = e.path();
            if !has_new_layout(&base, &ids) {
                continue;
            }
            if let Ok(children) = std::fs::read_dir(&base) {
                for c in children.flatten() {
                    if !c.path().is_dir() {
                        continue;
                    }
                    let child = c.file_name().to_string_lossy().to_string();
                    if child == "_shared" || ids.contains(&child) {
                        continue;
                    }
                    if std::fs::remove_dir_all(c.path()).is_ok() {
                        removed.push(format!("{name}/{child}"));
                    }
                }
            }
        }
    }
    removed
}

/// 列出孤立 webview（版本不存在 + 未挂载实例子目录），**只列不删**。
pub fn list_orphan_webviews(
    versions_dir: &Path,
    webview_dir: &Path,
    mounted: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<String> {
    let alive = alive_versions(versions_dir);
    let mut orphans = Vec::new();
    if let Ok(rd) = std::fs::read_dir(webview_dir) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if !alive.contains(&name) {
                orphans.push(name);
                continue;
            }
            let ids = mounted.get(&name).cloned().unwrap_or_default();
            let base = e.path();
            if !has_new_layout(&base, &ids) {
                continue;
            }
            if let Ok(children) = std::fs::read_dir(&base) {
                for c in children.flatten() {
                    if !c.path().is_dir() {
                        continue;
                    }
                    let child = c.file_name().to_string_lossy().to_string();
                    if child == "_shared" || ids.contains(&child) {
                        continue;
                    }
                    orphans.push(format!("{name}/{child}"));
                }
            }
        }
    }
    orphans.sort();
    orphans
}

/// 清理「回收站」里残留的目录（卸载时 rename 进来的，后台删除可能被中断）。
///
/// 启动时调用一次，把 `trash/` 下所有条目删掉。返回删除的条目数。
/// 失败静默（下次启动再试）。
pub fn cleanup_trash(trash_dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(rd) = std::fs::read_dir(trash_dir) {
        for e in rd.flatten() {
            if std::fs::remove_dir_all(e.path()).is_ok() {
                count += 1;
            }
        }
    }
    count
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

    // spawn 前：平台配置（Unix 新进程组；Windows 无操作）
    crate::jobobj::configure_command(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 store prune 失败: {}", e))?;
    // spawn 后：绑定进程守卫（Windows Job / Unix 进程组），管理器退出即杀，避免孤儿
    let _job = crate::jobobj::attach(&child);
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

    use std::collections::HashMap;

    fn mounted(pairs: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(v, ids)| (v.to_string(), ids.iter().map(|id| id.to_string()).collect()))
            .collect()
    }

    #[test]
    fn list_orphan_webviews_reports_without_deleting() {
        let base = tmp("list-orphan");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("0.1.7")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7")).unwrap(); // 存活
        std::fs::create_dir_all(webview.join("0.1.6")).unwrap(); // 孤儿
        let orphans = list_orphan_webviews(&versions, &webview, &HashMap::new());
        assert_eq!(orphans, vec!["0.1.6".to_string()]);
        // 只列不删
        assert!(webview.join("0.1.6").exists(), "不应删除");
        assert!(webview.join("0.1.7").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_removes_only_orphans() {
        let base = tmp("webview");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.2")).unwrap();
        std::fs::create_dir_all(webview.join("0.1.7-rc.1")).unwrap(); // 孤儿

        let removed = cleanup_orphan_webviews(&versions, &webview, &HashMap::new());
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
        std::fs::create_dir_all(webview.join("v1").join("_shared")).unwrap();
        let removed = cleanup_orphan_webviews(&versions, &webview, &HashMap::new());
        assert!(removed.is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_removes_unmounted_instance_dirs() {
        let base = tmp("webview-instance");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("v1")).unwrap();
        std::fs::create_dir_all(webview.join("v1").join("_shared")).unwrap();
        std::fs::create_dir_all(webview.join("v1").join("i1")).unwrap();
        std::fs::create_dir_all(webview.join("v1").join("i2")).unwrap();

        let removed = cleanup_orphan_webviews(&versions, &webview, &mounted(&[("v1", &["i1"])]));
        assert_eq!(removed, vec!["v1/i2"]);
        assert!(webview.join("v1").join("_shared").exists());
        assert!(webview.join("v1").join("i1").exists());
        assert!(!webview.join("v1").join("i2").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_skips_legacy_layout() {
        let base = tmp("webview-legacy");
        let versions = base.join("versions");
        let webview = base.join("webview");
        std::fs::create_dir_all(versions.join("v1")).unwrap();
        std::fs::create_dir_all(webview.join("v1").join("EBWebView")).unwrap();
        let removed = cleanup_orphan_webviews(&versions, &webview, &HashMap::new());
        assert!(removed.is_empty());
        assert!(webview.join("v1").join("EBWebView").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ensure_webview_layout_moves_legacy_into_shared() {
        let base = tmp("layout");
        let dirs = Dirs::new(base.clone());
        dirs.ensure().unwrap();
        let legacy = dirs.webview.join("v1");
        std::fs::create_dir_all(legacy.join("EBWebView")).unwrap();
        std::fs::write(legacy.join("marker"), b"x").unwrap();
        let version = VersionId::new("v1").unwrap();
        let out = ensure_webview_layout(&dirs, &version);
        assert_eq!(out, legacy);
        assert!(dirs
            .webview
            .join("v1")
            .join("_shared")
            .join("marker")
            .exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
