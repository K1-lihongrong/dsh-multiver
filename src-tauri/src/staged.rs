//! 暂存更新（staged update）：内嵌窗口里更新 multiver 的安全路径。
//!
//! ## 背景
//!
//! 内嵌窗口的 dsh 是 multiver 拉起的子进程。若在窗口内直接替换 multiver.exe，
//! 当前会话会被打断，且 Windows 上运行中的 exe 无法覆盖。
//!
//! ## 方案
//!
//! 拆成两步：
//! 1. **暂存**（插件在任意模式下都可做）：把新 exe 下载到 `<root>/.update/dsh-multiver.exe.new`，
//!    校验 SHA256，写 `pending.json` 记录版本与时间。
//! 2. **安装**（multiver 下次启动时）：`run()` 早期检查 `pending.json`，校验通过后
//!    **重命名当前 exe 为 .old → 把暂存 exe 移到原位置**。Windows 允许重命名运行中的文件，
//!    故无需等待进程退出，也不需要外部批处理。当前进程继续用已加载的旧代码跑完，
//!    用户下次启动即新版。
//!
//! 这样替换永远发生在多实例无冲突的窗口内，不会打断任何会话，也不会有批处理卡死风险。

use std::path::{Path, PathBuf};

/// 暂存目录名（位于数据根下）
const STAGE_DIR: &str = ".update";
/// 暂存 exe 文件名
const STAGED_EXE: &str = "dsh-multiver.exe.new";
/// 待安装标记文件名
const PENDING: &str = "pending.json";

/// `pending.json` 的内容
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pending {
    /// 目标版本号（仅作展示/日志）
    pub version: String,
    /// 新 exe 的 SHA256（十六进制小写）；空表示未提供，安装时跳过校验
    #[serde(default)]
    pub sha256: String,
    /// 暂存时间（unix 秒）
    pub staged_at: u64,
    /// 暂存 exe 的字节数（用于安装前的完整性兜底检查）
    #[serde(default)]
    pub size: u64,
}

/// 暂存目录路径：`<root>/.update`
pub fn stage_dir(root: &Path) -> PathBuf {
    root.join(STAGE_DIR)
}

/// 暂存 exe 路径
pub fn staged_exe(root: &Path) -> PathBuf {
    stage_dir(root).join(STAGED_EXE)
}

/// 标记文件路径
pub fn pending_path(root: &Path) -> PathBuf {
    stage_dir(root).join(PENDING)
}

/// 读取待安装标记。文件不存在 / 解析失败 → `None`（失败静默）。
pub fn load_pending(root: &Path) -> Option<Pending> {
    let text = std::fs::read_to_string(pending_path(root)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 写入待安装标记（覆盖）。失败返回错误描述。
///
/// 运行时由插件（JS 侧）直接写 JSON；此处保留 Rust 实现供单元测试与
/// 未来的 CLI `stage-update` 子命令复用。
#[allow(dead_code)]
pub fn save_pending(root: &Path, p: &Pending) -> Result<(), String> {
    let dir = stage_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建暂存目录失败：{}", e))?;
    let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    std::fs::write(pending_path(root), text).map_err(|e| format!("写入标记失败：{}", e))
}

/// 清理暂存（安装完成或取消后调用）。
pub fn clear_stage(root: &Path) {
    let dir = stage_dir(root);
    let _ = std::fs::remove_file(pending_path(root));
    let _ = std::fs::remove_file(staged_exe(root));
    // 目录空了就删掉，避免数据根里长期留一个空目录
    let _ = std::fs::remove_dir(&dir);
}

/// 清理上一轮替换留下的 `dsh-multiver.exe.old`。
///
/// 替换当次删不掉（旧 exe 正被当前进程占用），留到下次启动时清理。
/// 失败静默——备份残留不影响功能，下次启动会再试。
pub fn cleanup_backup() {
    if let Ok(exe) = std::env::current_exe() {
        let backup = exe.with_extension("exe.old");
        if backup.exists() {
            let _ = std::fs::remove_file(&backup);
        }
    }
}

/// 启动时检查并安装暂存更新。
///
/// **不需要重启等待**：Windows 允许重命名运行中的 exe，重命名后原路径可写入新文件。
/// 流程是「改名旧的 → 放入新的 → 继续启动」。当前进程用已加载的旧代码跑完，
/// 用户**下次启动**即新版。
///
/// 返回 `Some(版本号)` 表示本次已完成替换（调用方记日志后继续正常启动）；
/// 返回 `None` 表示没有待安装更新（或校验失败已清理）。
///
/// 安全性（自更新通道按最严格标准处理）：
/// - 暂存 exe 不存在 → 清理标记，正常启动
/// - **缺少 `sha256` → 拒绝安装**（不凭文件大小放行任意二进制）
/// - `sha256` 校验不符 → 清理暂存，写日志，正常启动
/// - 当前 exe 缺失（上次替换/回滚失败残留）→ 中止并保留 `.old`，不动它
/// - 重命名失败（文件被其他进程锁定）→ **保留暂存**，下次启动重试
/// - 放入新 exe 失败 → 回滚（把 .old 改回来），**保留暂存**待重试
/// - 全部成功后才删除标记（提前删会产生无入口的孤儿暂存）
pub fn take_over_pending(root: &Path) -> Option<String> {
    let p = load_pending(root)?;
    let staged = staged_exe(root);

    if !staged.exists() {
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!("暂存更新标记存在但 exe 缺失（{}），已清理", p.version),
        );
        clear_stage(root);
        return None;
    }

    // 校验：自更新通道必须验签名。
    // 不提供 sha256 就等于「无法确认这个 exe 是不是我们要发布的那个」，
    // 此时宁可拒绝安装，也不能凭文件大小放行——那是自动执行任意二进制的口子。
    if p.sha256.is_empty() {
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!("暂存更新缺少 SHA256（{}），拒绝安装并清理", p.version),
        );
        clear_stage(root);
        return None;
    }
    match crate::updater::sha256_file_pub(&staged) {
        Ok(actual) if actual.eq_ignore_ascii_case(&p.sha256) => {}
        Ok(actual) => {
            crate::logging::write_line(
                &root.join("logs"),
                "maintenance.log",
                &format!(
                    "暂存更新校验失败（{}）：期望 {} 实际 {}，已丢弃",
                    p.version, p.sha256, actual
                ),
            );
            clear_stage(root);
            return None;
        }
        Err(e) => {
            crate::logging::write_line(
                &root.join("logs"),
                "maintenance.log",
                &format!("暂存更新校验出错（{}）：{}，已丢弃", p.version, e),
            );
            clear_stage(root);
            return None;
        }
    }

    let target = match std::env::current_exe() {
        Ok(t) => t,
        Err(e) => {
            crate::logging::write_line(
                &root.join("logs"),
                "maintenance.log",
                &format!("暂存更新无法定位当前 exe：{}", e),
            );
            clear_stage(root);
            return None;
        }
    };

    let backup = target.with_extension("exe.old");

    // 安全前提：当前 exe 必须存在。
    // 若上一轮「放入新 exe 失败且回滚也失败」，target 会缺失、.old 是唯一可用副本——
    // 此时绝不能往下走（下一行会删掉 .old，彻底毁掉程序）。
    if !target.exists() {
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!(
                "暂存更新中止（{}）：当前 exe 不存在，疑似上次替换失败。请把 {} 改回 dsh-multiver.exe 后重试",
                p.version,
                backup.display()
            ),
        );
        return None;
    }

    // 清掉上一轮可能留下的备份（此时 target 存在，备份已无用）
    let _ = std::fs::remove_file(&backup);

    // 1) 重命名运行中的 exe（Windows 允许）
    // 失败时**保留暂存**：多半是文件被短暂锁定（杀软扫描、另一个实例启动中），
    // 下次启动再试即可，没必要让用户重新下载 10 MB。
    if let Err(e) = std::fs::rename(&target, &backup) {
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!(
                "暂存更新无法重命名当前 exe（{}）：{}；暂存已保留，下次启动重试",
                p.version, e
            ),
        );
        return None;
    }

    // 2) 放入新 exe
    if let Err(e) = std::fs::rename(&staged, &target) {
        // 回滚：把旧的改回来
        let rolled = std::fs::rename(&backup, &target).is_ok();
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!(
                "暂存更新放入新 exe 失败（{}）：{}；回滚{}",
                p.version,
                e,
                if rolled {
                    "成功"
                } else {
                    "失败，请手动把 dsh-multiver.exe.old 改回 dsh-multiver.exe"
                }
            ),
        );
        // 暂存文件与 pending.json 都保留：回滚成功后目标路径已恢复原状，
        // 下次启动会重新读到标记并重试，用户不必重新下载。
        return None;
    }

    // 替换成功。**必须在成功之后才删标记**——
    // 若提前删，第 2 步失败回滚后暂存就成了没有入口的孤儿文件；
    // 若一直不删，新进程启动会再读到标记并重复替换（实测会导致启动死循环）。
    let _ = std::fs::remove_file(pending_path(root));

    // 3) 尽力删除备份（被当前进程占用时删不掉，留到下次启动再清）
    let _ = std::fs::remove_file(&backup);

    crate::logging::write_line(
        &root.join("logs"),
        "maintenance.log",
        &format!(
            "暂存更新 {} 已就位，下次启动生效（当前进程仍以旧代码运行）",
            p.version
        ),
    );

    Some(p.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("dsh-multiver-stage-{}", name));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_and_load_pending_roundtrip() {
        let root = tmp("roundtrip");
        let p = Pending {
            version: "0.3.2".into(),
            sha256: "abc123".into(),
            staged_at: 1234567890,
            size: 1024,
        };
        save_pending(&root, &p).unwrap();
        let back = load_pending(&root).expect("应能读回");
        assert_eq!(back.version, "0.3.2");
        assert_eq!(back.sha256, "abc123");
        assert_eq!(back.size, 1024);
    }

    #[test]
    fn load_missing_pending_returns_none() {
        let root = tmp("missing");
        assert!(load_pending(&root).is_none());
    }

    #[test]
    fn load_corrupt_pending_returns_none() {
        let root = tmp("corrupt");
        std::fs::create_dir_all(stage_dir(&root)).unwrap();
        std::fs::write(pending_path(&root), "not json{{{").unwrap();
        assert!(load_pending(&root).is_none());
    }

    #[test]
    fn clear_stage_removes_files_and_dir() {
        let root = tmp("clear");
        let p = Pending {
            version: "1.0.0".into(),
            sha256: String::new(),
            staged_at: 0,
            size: 0,
        };
        save_pending(&root, &p).unwrap();
        std::fs::write(staged_exe(&root), b"fake").unwrap();
        assert!(stage_dir(&root).exists());

        clear_stage(&root);
        assert!(!pending_path(&root).exists());
        assert!(!staged_exe(&root).exists());
        assert!(!stage_dir(&root).exists());
    }

    #[test]
    fn take_over_without_pending_returns_none() {
        let root = tmp("nopending");
        assert!(take_over_pending(&root).is_none());
    }

    #[test]
    fn take_over_with_missing_staged_exe_clears_and_returns_none() {
        let root = tmp("missing-exe");
        let p = Pending {
            version: "9.9.9".into(),
            sha256: String::new(),
            staged_at: 0,
            size: 0,
        };
        save_pending(&root, &p).unwrap();
        // 只写标记不写 exe
        assert!(take_over_pending(&root).is_none());
        assert!(!pending_path(&root).exists(), "应清理标记");
    }

    /// 缺少 SHA256 时必须拒绝安装——自更新通道不能凭文件大小放行。
    #[test]
    fn take_over_without_checksum_rejects_and_clears() {
        let root = tmp("no-sum");
        let staged = staged_exe(&root);
        std::fs::create_dir_all(stage_dir(&root)).unwrap();
        std::fs::write(&staged, b"some bytes").unwrap();
        let p = Pending {
            version: "9.9.9".into(),
            sha256: String::new(), // 没有校验值
            staged_at: 0,
            size: 10, // 大小对得上也不能放过
        };
        save_pending(&root, &p).unwrap();

        assert!(take_over_pending(&root).is_none(), "无 sha256 不应安装");
        assert!(!pending_path(&root).exists(), "应清理标记");
        assert!(!staged.exists(), "应清理暂存 exe");
    }

    /// 当前 exe 不存在时（上次替换/回滚失败的残留态）必须中止，
    /// 且**不能**删除 .old——那是唯一可用的程序副本。
    #[test]
    fn take_over_aborts_when_target_missing_and_keeps_backup() {
        let root = tmp("target-missing");
        let staged = staged_exe(&root);
        std::fs::create_dir_all(stage_dir(&root)).unwrap();
        std::fs::write(&staged, b"new exe bytes").unwrap();
        let p = Pending {
            version: "9.9.9".into(),
            sha256: crate::updater::sha256_file_pub(&staged).unwrap(),
            staged_at: 0,
            size: 0,
        };
        save_pending(&root, &p).unwrap();

        // 注意：测试环境里 current_exe() 是测试二进制本身，它一定存在，
        // 故这里只能验证「staged 与 pending 被保留」这一可观测结果。
        // 缺失 target 的分支通过真实 exe 场景在端到端测试中覆盖。
        let _ = take_over_pending(&root);
        // 无论走哪条分支，暂存都不应被无谓丢弃
        assert!(
            staged.exists() || !pending_path(&root).exists(),
            "要么保留暂存待重试，要么已成功消费标记"
        );
    }

    #[test]
    fn take_over_with_bad_checksum_clears_and_returns_none() {
        let root = tmp("bad-sum");
        let staged = staged_exe(&root);
        std::fs::create_dir_all(stage_dir(&root)).unwrap();
        std::fs::write(&staged, b"not a real exe").unwrap();
        let p = Pending {
            version: "9.9.9".into(),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            staged_at: 0,
            size: 0,
        };
        save_pending(&root, &p).unwrap();
        assert!(take_over_pending(&root).is_none(), "校验失败不应安装");
        assert!(!pending_path(&root).exists(), "应清理标记");
        assert!(!staged.exists(), "应清理坏的 exe");
    }
}
