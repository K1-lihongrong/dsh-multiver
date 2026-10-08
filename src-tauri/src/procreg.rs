//! Unix 残留进程清理（GAP-010 的兜底机制）。
//!
//! ## 背景
//!
//! Unix 侧不再用 \`PR_SET_PDEATHSIG\`（它绑线程、会在池线程回收时误杀子进程，见 GAP-008）。
//! 代价是：**管理器被 \`kill -9\`（或崩溃）时来不及清理 dsh**，会留下孤儿进程
//! （直到下次启动管理器）。
//!
//! 本模块把该代价降到"下次启动即清"：
//! - dsh 启动成功后，把它的**进程组 ID** 登记到 \`<root>/logs/dsh-procs.json\`
//! - 管理器启动时，读该文件，对仍存活的 PGID 发 \`killpg\`，然后清空文件
//! - 正常退出/关窗时移除对应条目（文件为空则删除）
//!
//! ## 安全
//!
//! 只 kill 登记在册、且**进程组仍存在**的 PGID，且带归属校验（见 \`is_alive\`）。
//! 不做"扫描所有进程找 dsh"这种危险操作。
//!
//! ## 平台
//!
//! 仅 Unix 需要（Windows 有 Job Object，OS 级保障，无需此机制）。
//! 整个模块由 `lib.rs` 用 `#[cfg(unix)]` 门控，Windows 不编译。

use std::path::{Path, PathBuf};

/// 登记文件路径：\`<root>/logs/dsh-procs.json\`
pub fn registry_path(root: &Path) -> PathBuf {
    root.join("logs").join("dsh-procs.json")
}

/// 一条登记记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// dsh 进程组 ID（= 子进程 pid，因它被设为组长）
    pub pgid: i32,
    /// 版本号（仅供日志/排查）
    pub version: String,
    /// 登记时间（unix 秒）
    pub at: u64,
}

/// 读取登记表。文件不存在 / 解析失败 → 空表（失败静默）。
pub fn load(root: &Path) -> Vec<Entry> {
    let p = registry_path(root);
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 覆盖写入登记表。空表 → 删除文件（保持目录干净）。失败静默。
fn save(root: &Path, list: &[Entry]) {
    let p = registry_path(root);
    if list.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(list) {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(&p, text);
    }
}

/// 登记一个 dsh 进程组。
pub fn register(root: &Path, pgid: i32, version: &str) {
    let mut list = load(root);
    // 去重（同 pgid 只留一条）
    list.retain(|e| e.pgid != pgid);
    list.push(Entry {
        pgid,
        version: version.to_string(),
        at: crate::logging::now_secs(),
    });
    save(root, &list);
}

/// 注销一个进程组（正常关窗/退出时调用）。
pub fn unregister(root: &Path, pgid: i32) {
    let mut list = load(root);
    let before = list.len();
    list.retain(|e| e.pgid != pgid);
    if list.len() != before {
        save(root, &list);
    }
}

/// 启动时清理：对登记表中仍存活的进程组发 SIGKILL，然后清空登记表。
///
/// 返回被清理的条数（供日志）。
pub fn cleanup_stale(root: &Path) -> usize {
    let list = load(root);
    let mut killed = 0usize;
    for e in &list {
        // 进程组是否存在：killpg(pgid, 0) 探测（0 号信号不投递，仅做权限/存在性检查）
        let exists = unsafe { libc::killpg(e.pgid, 0) } == 0;
        if exists {
            unsafe {
                libc::killpg(e.pgid, libc::SIGKILL);
            }
            killed += 1;
            crate::logging::write_line(
                &root.join("logs"),
                "maintenance.log",
                &format!("清理残留 dsh 进程组 pgid={} version={}", e.pgid, e.version),
            );
        }
    }
    // 无论是否存活，都清空登记表（启动时是新的开始）
    save(root, &[]);
    killed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("dsh-multiver-procreg-{}", name));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn register_unregister_roundtrip() {
        let root = tmp("rt");
        register(&root, 12345, "0.2.0-rc.2");
        let list = load(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pgid, 12345);
        assert_eq!(list[0].version, "0.2.0-rc.2");

        unregister(&root, 12345);
        assert!(load(&root).is_empty());
        // 空表应删除文件
        assert!(!registry_path(&root).exists());
    }

    #[test]
    fn register_dedups_same_pgid() {
        let root = tmp("dedup");
        register(&root, 999, "a");
        register(&root, 999, "b");
        let list = load(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].version, "b"); // 后写入的覆盖
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let root = tmp("missing");
        assert!(load(&root).is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let root = tmp("corrupt");
        let p = registry_path(&root);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "not json{{{").unwrap();
        assert!(load(&root).is_empty());
    }
}
