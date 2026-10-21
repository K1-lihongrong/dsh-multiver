//! 版本管理核心逻辑。
//!
//! D5 技术债拆分（v0.3.0）：原 `versions.rs`（1079 行）按域拆成 4 个子模块，
//! 通过本模块 `pub use` 重导出，**对外路径 `crate::versions::X` 完全不变**。
//!
//! - `list`：版本列表 / 排序 / 安装日期
//! - `install`：安装、卸载、进度事件、错误分类
//! - `size`：占用统计（两段式扫描）
//! - `isolated`：数据隔离（复制共享 home / 清理 / junction 保留）

pub mod install;
pub mod isolated;
pub mod list;
pub mod size;

// 重导出保持与原 `versions.rs` 完全一致的公开 API（`crate::versions::X`）。
// 部分符号当前 crate 内暂无调用方（如 parse_stage 供整合包分支），仍保留导出。
#[allow(unused_imports)]
pub use install::{
    classify_error, exists, install, parse_stage, uninstall_fast, ProgressEvent, ProgressParser,
    INSTALL_TOTAL_STEPS,
};
#[allow(unused_imports)]
pub use isolated::{clear_isolated, copy_shared_to_isolated, isolated_home};
#[allow(unused_imports)]
pub use list::{list, VersionInfo};
#[allow(unused_imports)]
pub use size::{
    collect_files, detail_from_files, dir_size, version_size_detail, CancelCheck, FileList,
    ScanProgress, SizeInfo,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 progress 供测试（`progress` 是 pub(crate)，经 install 模块导出）。
    use super::install::progress;

    #[test]
    fn progress_has_stage_names() {
        let p1 = progress(1, String::new(), 0.0);
        assert_eq!(p1.stage, "准备目录");
        assert_eq!(p1.step, 1);
        assert_eq!(p1.total, 8);
        let p8 = progress(8, String::new(), 1.0);
        assert_eq!(p8.stage, "完成");
        assert_eq!(p8.fraction, 1.0);
    }

    #[test]
    fn parses_pnpm_progress_lines() {
        let mut p = ProgressParser::new();
        // Packages 行不再产生事件，只记下总数
        assert!(p.parse("Packages: +445").is_none());
        // resolved 阶段
        let (s, d, _) = p
            .parse("Progress: resolved 234, reused 222, downloaded 0, added 0")
            .unwrap();
        assert_eq!(s, 4);
        assert!(d.contains("234"), "detail={}", d);
        // downloaded 阶段
        let (s, d, _) = p
            .parse("Progress: resolved 234, reused 222, downloaded 5, added 0")
            .unwrap();
        assert_eq!(s, 5);
        assert!(d.contains("5"));
        // added 阶段：fraction = 12/445
        let (s, d, f) = p
            .parse("Progress: resolved 234, reused 222, downloaded 5, added 12")
            .unwrap();
        assert_eq!(s, 6);
        assert!(d.contains("12"));
        assert!((f - 12.0 / 445.0).abs() < 0.001, "frac={}", f);
        // Done
        let (s, _, _) = p.parse("Done in 47.6s using pnpm v11").unwrap();
        assert_eq!(s, 8);
    }

    #[test]
    fn step_never_goes_backwards() {
        let mut p = ProgressParser::new();
        // 先到 step 5（下载）
        let (s, _, _) = p
            .parse("Progress: resolved 100, reused 0, downloaded 5, added 0")
            .unwrap();
        assert_eq!(s, 5);
        // 交错出现 resolved 行（raw step 4）——不应回退
        let (s2, _, _) = p
            .parse("Progress: resolved 200, reused 0, downloaded 0, added 0")
            .unwrap();
        assert_eq!(s2, 5, "阶段不应回退");
        // 再到 step 6
        let (s3, _, _) = p
            .parse("Progress: resolved 200, reused 0, downloaded 5, added 10")
            .unwrap();
        assert_eq!(s3, 6);
        // 又出现 resolved 行（raw 4）——仍不回退
        let (s4, _, _) = p
            .parse("Progress: resolved 300, reused 0, downloaded 0, added 0")
            .unwrap();
        assert_eq!(s4, 6, "阶段不应回退");
    }

    #[test]
    fn extract_num_works() {
        use super::install::extract_num;
        assert_eq!(
            extract_num("resolved 234, reused 222", "resolved"),
            Some(234)
        );
        assert_eq!(extract_num("resolved 234, reused 222", "reused"), Some(222));
        assert_eq!(extract_num("no number here", "xyz"), None);
    }

    #[test]
    fn classify_private_package() {
        let raw =
            "ERR_PNPM_FETCH_404  GET https://registry.npmjs.org/@deepseek-ai/dsh: Not Found - 404";
        assert_eq!(classify_error(raw), "private-package");
        // 需要同时含 @deepseek-ai 才归为 private-package
        assert_eq!(classify_error("ERR_PNPM_FETCH_404 other-pkg"), "network");
    }

    #[test]
    fn classify_incomplete_version() {
        assert_eq!(
            classify_error("ERR_PNPM_NO_MATCHING_VERSION No matching version found"),
            "incomplete-version"
        );
        assert_eq!(
            classify_error("No matching version found for x"),
            "incomplete-version"
        );
    }

    #[test]
    fn classify_network() {
        assert_eq!(classify_error("ERR_PNPM_FETCH something failed"), "network");
        assert_eq!(classify_error("connect ETIMEDOUT 1.2.3.4:443"), "network");
        assert_eq!(classify_error("ECONNRESET"), "network");
        assert_eq!(classify_error("getaddrinfo ENOTFOUND registry"), "network");
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify_error("some totally unrelated failure"), "unknown");
        assert_eq!(classify_error(""), "unknown");
    }

    // ── uninstall_fast：rename 到回收站 ──

    fn uninstall_tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-uninstall-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn uninstall_fast_moves_version_to_trash() {
        let base = uninstall_tmp("fast");
        let versions = base.join("versions");
        let trash = base.join("trash");
        // 造一个"版本目录"
        std::fs::create_dir_all(versions.join("0.1.0").join("node_modules")).unwrap();
        assert!(versions.join("0.1.0").exists());

        let (ok, msg) = uninstall_fast(&versions, &trash, "0.1.0");
        assert!(ok, "msg={}", msg);
        // 版本目录应立即消失
        assert!(!versions.join("0.1.0").exists(), "版本目录应已移走");
        // 回收站里应有一份（后台可能已删，故只验证"移走"而非"存在"）
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn uninstall_fast_missing_version_fails() {
        let base = uninstall_tmp("missing");
        let versions = base.join("versions");
        let trash = base.join("trash");
        std::fs::create_dir_all(&versions).unwrap();
        let (ok, msg) = uninstall_fast(&versions, &trash, "9.9.9");
        assert!(!ok);
        assert!(msg.contains("不存在"), "msg={}", msg);
        let _ = std::fs::remove_dir_all(&base);
    }
}
