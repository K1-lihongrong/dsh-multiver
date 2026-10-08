//! Unix 残留进程清理（GAP-010 的兜底机制）。
//!
//! ## 背景
//!
//! Unix 侧不再用 \`PR_SET_PDEATHSIG\`（它绑线程、会在池线程回收时误杀子进程，见 GAP-008）。
//! 代价是：**管理器被 \`kill -9\`（或崩溃）时来不及清理 dsh**，会留下孤儿进程
//! （直到下次启动管理器）。
//!
//! 本模块把该代价降到"下次启动即清"：
//! - dsh 启动成功后，把它的**进程组 ID + 身份令牌**登记到 \`<root>/logs/dsh-procs.json\`
//! - 管理器启动时，清理"写入者已死"的条目（且校验目标确实是那个 dsh）
//! - 正常退出/关窗时移除对应条目
//!
//! ## 安全（两层校验，杜绝误杀）
//!
//! 1. **写入者存活校验**：条目记录写入者（管理器实例）的 pid + 身份令牌。清理时若
//!    写入者仍存活 → **跳过**（避免同一数据根下多开的管理器互相杀掉对方的 dsh）。
//! 2. **目标身份校验**：条目记录 dsh 组长（= pgid）的身份令牌。只有当前该 pgid 的
//!    身份令牌与登记时**一致**才发 SIGKILL —— 杜绝 PGID 被内核回收复用后误杀无关进程组。
//!
//! 身份令牌由 \`proc_ident\` 提供（Linux 用 \`/proc/<pid>/stat\` 的 starttime）。
//! 令牌读不到（进程已死 / 平台不支持）时一律**保守跳过**——宁漏杀，不误杀。
//!
//! ## 平台
//!
//! 仅 Unix 需要（Windows 有 Job Object，OS 级保障，无需此机制）。
//! 整个模块由 \`lib.rs\` 用 \`#[cfg(unix)]\` 门控，Windows 不编译。

use std::path::{Path, PathBuf};

/// 登记文件路径：\`<root>/logs/dsh-procs.json\`
pub fn registry_path(root: &Path) -> PathBuf {
    root.join("logs").join("dsh-procs.json")
}

/// 锁文件路径：\`<root>/logs/dsh-procs.lock\`
///
/// 独立于登记文件——登记文件在空表时会被删除，锁文件必须常驻（否则删除时锁丢失）。
fn lock_path(root: &Path) -> PathBuf {
    root.join("logs").join("dsh-procs.lock")
}

// ─────────────────────── 进程身份令牌 ───────────────────────

/// 取进程身份令牌：**同一 pid 的不同进程 → 不同令牌**。
///
/// - Linux：\`/proc/<pid>/stat\` 第 22 字段（starttime，自开机起的时钟滴答数）
/// - 其它 Unix（含 macOS）：返回 \`None\`（macOS 未适配，见 docs/征求协助.md；走保守跳过）
///
/// \`None\` 语义 = "读不到"（进程不存在 / 平台不支持）。
/// **调用方必须在 None 时保守跳过，绝不下杀。**
pub fn proc_ident(pid: i32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
        parse_starttime(&stat).map(|t| t.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// 从 \`/proc/<pid>/stat\` 的内容解析 starttime（第 22 字段）。
///
/// **坑**：第 2 字段 \`comm\` 用括号包裹且**可含空格和括号**，
/// 因此必须先定位**最后一个 \`)\`**，再对其后的部分按空格切分。
/// 切分后第 1 个是 state（字段 3），故 starttime（字段 22）是切分后的**第 20 个**（下标 19）。
#[cfg(target_os = "linux")]
pub fn parse_starttime(stat: &str) -> Option<u64> {
    let after_comm = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    // after_comm 从 state(字段3) 开始；starttime 是字段22 → 下标 22-3 = 19
    fields.get(19)?.parse::<u64>().ok()
}

// ─────────────────────── 登记表读写 ───────────────────────

/// 一条登记记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// dsh 进程组 ID（= 子进程 pid，因它被设为组长）
    pub pgid: i32,
    /// 版本号（仅供日志/排查）
    pub version: String,
    /// 登记时间（unix 秒）
    pub at: u64,
    /// 写入者（管理器实例）的 pid
    #[serde(default)]
    pub owner_pid: i32,
    /// 写入者的身份令牌（空 = 采集失败 → 清理时保守跳过）
    #[serde(default)]
    pub owner_ident: String,
    /// dsh 组长（= pgid）的身份令牌（空 = 采集失败 → 不杀）
    #[serde(default)]
    pub dsh_ident: String,
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
///
/// 调用方须持锁（见 \`with_registry_lock\`）。
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

/// 在登记表文件锁保护下执行 \`f\`（读-改-写的原子性）。
///
/// - 用 \`flock(LOCK_EX)\` 排他锁；进程退出/被杀时锁由内核自动释放
/// - 锁文件是独立的 \`dsh-procs.lock\`（登记文件可能被删除，锁文件常驻）
/// - 任何环节失败都**静默降级**（拿不到锁也执行 f，只是失去互斥；不阻塞主流程）
fn with_registry_lock<T>(root: &Path, f: impl FnOnce() -> T) -> T {
    use std::os::unix::io::AsRawFd;

    let dir = root.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock_path(root));

    match lock_file {
        Ok(file) => {
            // LOCK_EX：排他；失败也不阻塞（降级为无锁执行）
            unsafe {
                libc::flock(file.as_raw_fd(), libc::LOCK_EX);
            }
            let result = f();
            unsafe {
                libc::flock(file.as_raw_fd(), libc::LOCK_UN);
            }
            result
        }
        Err(_) => f(), // 打不开锁文件 → 降级无锁
    }
}

// ─────────────────────── 登记 / 注销 / 清理 ───────────────────────

/// 登记一个 dsh 进程组（记录写入者与 dsh 双方的身份令牌）。
pub fn register(root: &Path, pgid: i32, version: &str) {
    with_registry_lock(root, || {
        let mut list = load(root);
        // 去重（同 pgid 只留一条）
        list.retain(|e| e.pgid != pgid);
        let me = std::process::id() as i32;
        list.push(Entry {
            pgid,
            version: version.to_string(),
            at: crate::logging::now_secs(),
            owner_pid: me,
            owner_ident: proc_ident(me).unwrap_or_default(),
            dsh_ident: proc_ident(pgid).unwrap_or_default(),
        });
        save(root, &list);
    });
}

/// 注销一个进程组（正常关窗/退出时调用）。
pub fn unregister(root: &Path, pgid: i32) {
    with_registry_lock(root, || {
        let mut list = load(root);
        let before = list.len();
        list.retain(|e| e.pgid != pgid);
        if list.len() != before {
            save(root, &list);
        }
    });
}

/// 写入者（管理器实例）是否仍存活：身份令牌一致才算。
///
/// 令牌读不到或为空 → 视为**不存活**（但后续 dsh 身份校验仍会兜底，不会误杀）。
fn owner_alive(e: &Entry) -> bool {
    if e.owner_ident.is_empty() {
        return false;
    }
    matches!(proc_ident(e.owner_pid), Some(id) if id == e.owner_ident)
}

/// 目标进程组是否确实是登记时那个 dsh：身份令牌一致才算。
///
/// 令牌读不到或为空 → **否**（不杀）。
fn target_is_ours(e: &Entry) -> bool {
    if e.dsh_ident.is_empty() {
        return false;
    }
    matches!(proc_ident(e.pgid), Some(id) if id == e.dsh_ident)
}

/// 启动时清理：仅清理"写入者已死"且"目标确为登记的 dsh"的条目。
///
/// 存活实例的条目**原样保留**（避免多开管理器互杀）。返回被清理的条数。
pub fn cleanup_stale(root: &Path) -> usize {
    with_registry_lock(root, || {
        let list = load(root);
        let mut keep: Vec<Entry> = Vec::new();
        let mut killed = 0usize;
        for e in list {
            if owner_alive(&e) {
                // 写入者还在跑 → 这是活跃实例的 dsh，不动
                keep.push(e);
                continue;
            }
            // 写入者已死 → 尝试清理（但先校验目标身份，防 PGID 复用误杀）
            if target_is_ours(&e) {
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
            // 无论是否杀掉，该条目都已处理 → 不保留
        }
        save(root, &keep);
        killed
    })
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
        assert_eq!(list[0].owner_pid, std::process::id() as i32);
        // 本进程身份可读（Linux）→ 非空；其它 Unix 为 None → 空
        // 只断言"不 panic"与"结构正确"

        unregister(&root, 12345);
        assert!(load(&root).is_empty());
        assert!(!registry_path(&root).exists());
    }

    #[test]
    fn register_dedups_same_pgid() {
        let root = tmp("dedup");
        register(&root, 999, "a");
        register(&root, 999, "b");
        let list = load(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].version, "b");
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

    /// 旧版（0.2.4）文件：无 owner_pid/owner_ident/dsh_ident 字段 → 仍能解析（serde default）。
    #[test]
    fn load_legacy_file_without_new_fields() {
        let root = tmp("legacy");
        let p = registry_path(&root);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, r#"[{"pgid":42,"version":"old","at":1}]"#).unwrap();
        let list = load(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pgid, 42);
        assert_eq!(list[0].owner_ident, "");
        assert_eq!(list[0].dsh_ident, "");
        // 旧条目：owner_ident 空 → owner_alive=false；dsh_ident 空 → target_is_ours=false
        // → 清理时保守跳过（不杀）
        assert!(!owner_alive(&list[0]));
        assert!(!target_is_ours(&list[0]));
    }

    /// #2 回归：目标身份不符（模拟 PGID 被复用）→ 绝不发 SIGKILL。
    /// 用一个真实存在但身份不符的 pgid（本进程 pid 对应的组）来断言"不杀"。
    #[test]
    fn cleanup_skips_when_target_identity_mismatch() {
        let root = tmp("mismatch");
        // 用一个**独立假 pgid**（不指向本进程组），避免身份判定若回归时
        // killpg(自己) 连带干掉整个 cargo test 进程组。
        // 取一个几乎不可能存在的 pid 作为 pgid。
        let fake_pgid = 2147483000i32;
        let list = vec![Entry {
            pgid: fake_pgid,
            version: "fake".into(),
            at: 0,
            owner_pid: 2147483000,          // 不存在的 owner → owner_alive=false
            owner_ident: "nonexistent".into(),
            dsh_ident: "definitely-not-my-ident".into(), // 与真实身份不符
        }];
        save(&root, &list);
        let killed = cleanup_stale(&root);
        // 身份不符 → 不发 kill；且条目被清掉
        assert_eq!(killed, 0, "身份不符时不应杀任何进程");
        assert!(load(&root).is_empty());
    }

    /// #1 回归：写入者（本进程）存活 → 其条目必须被保留，不被清理。
    ///
    /// 平台门控：非 Linux 上 `proc_ident` 返回 None → `owner_ident` 为空 →
    /// `owner_alive` 恒 false → 该断言在 macOS 上必失败。仅 Linux 有意义。
    #[cfg(target_os = "linux")]
    #[test]
    fn cleanup_keeps_entries_of_live_owner() {
        let root = tmp("live-owner");
        register(&root, 4242, "keep-me");
        // 本进程即写入者，且存活 → cleanup 应保留它
        let killed = cleanup_stale(&root);
        assert_eq!(killed, 0, "活跃实例的条目不应被杀");
        let list = load(&root);
        assert_eq!(list.len(), 1, "活跃实例的条目应保留");
        assert_eq!(list[0].pgid, 4242);
    }

    /// #3 回归：**并发** register 多个不同 pgid → 全部保留（无丢更新）。
    ///
    /// 用真并发（多线程同时写）验证 flock 的互斥；若去掉锁，读-改-写交错会丢条目。
    #[test]
    fn register_concurrent_no_lost_update() {
        let root = tmp("no-lost-concurrent");
        let mut handles = Vec::new();
        for i in 0..8 {
            let r = root.clone();
            handles.push(std::thread::spawn(move || {
                register(&r, 1000 + i, &format!("v{}", i));
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let list = load(&root);
        assert_eq!(list.len(), 8, "并发 register 不应丢任何条目");
        let pgids: Vec<i32> = list.iter().map(|e| e.pgid).collect();
        for i in 0..8 {
            assert!(pgids.contains(&(1000 + i)), "缺少 pgid {}", 1000 + i);
        }
    }
}

/// Linux 专属：\`parse_starttime\` 的解析测试（含 comm 含空格/括号的坑）。
#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;

    #[test]
    fn parse_starttime_handles_spaces_and_parens_in_comm() {
        // 真实格式：pid (comm) state ppid ... starttime(22) ...
        // 构造：comm 含空格与括号
        let stat = "1234 (my (weird) proc) S 1 1234 1234 0 -1 4194560                     100 0 0 0 5 3 0 0 20 0 10 0 987654 123456 789                     0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        // 第 22 字段应为 987654（构造时故意放这个值）
        assert_eq!(parse_starttime(stat), Some(987654));
    }

    #[test]
    fn parse_starttime_rejects_malformed() {
        assert_eq!(parse_starttime("not a stat line"), None);
        assert_eq!(parse_starttime("1 (x) S 2 3"), None); // 字段不足
    }

    #[test]
    fn proc_ident_of_self_is_some_on_linux() {
        let me = std::process::id() as i32;
        assert!(proc_ident(me).is_some(), "Linux 上本进程身份应可读");
    }

    #[test]
    fn proc_ident_of_dead_pid_is_none() {
        // 一个几乎不可能存在的 pid
        assert_eq!(proc_ident(2147483646), None);
    }
}
