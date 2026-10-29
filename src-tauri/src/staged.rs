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
/// 安全性：
/// - 暂存 exe 不存在 → 清理标记，正常启动
/// - `sha256` 非空但校验不符 → 清理暂存，写日志，正常启动（不装坏文件）
/// - 重命名失败（文件被其他进程锁定）→ 清理暂存，正常启动
/// - 放入新 exe 失败 → 回滚（把 .old 改回来），正常启动
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

    // 校验（标记里带 sha256 才校验，空则只做大小检查）
    if !p.sha256.is_empty() {
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
    } else if p.size > 0 {
        let size_ok = std::fs::metadata(&staged)
            .map(|m| m.len() == p.size)
            .unwrap_or(false);
        if !size_ok {
            crate::logging::write_line(
                &root.join("logs"),
                "maintenance.log",
                &format!("暂存更新大小不符（{}），已丢弃", p.version),
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

    // 先删标记——避免替换后新进程再次读到而重复执行
    let _ = std::fs::remove_file(pending_path(root));

    let backup = target.with_extension("exe.old");
    // 清掉上一轮可能留下的备份（若已不被占用）
    let _ = std::fs::remove_file(&backup);

    // 1) 重命名运行中的 exe（Windows 允许）
    if let Err(e) = std::fs::rename(&target, &backup) {
        crate::logging::write_line(
            &root.join("logs"),
            "maintenance.log",
            &format!("暂存更新无法重命名当前 exe（{}）：{}，已放弃", p.version, e),
        );
        clear_stage(root);
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
        clear_stage(root);
        return None;
    }

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
