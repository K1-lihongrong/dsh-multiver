//! Tauri 命令层（D6 技术债拆分，v0.3.0）。
//!
//! 原 `commands.rs`（1687 行）按域拆分为子模块，经本模块 `pub use` 重导出，
//! **对外路径 `crate::commands::X` 完全不变**（lib.rs 的命令注册表零改动）。
//!
//! | 模块 | 内容 |
//! | :--- | :--- |
//! | `state` | 应用状态查询（get_state）/ 数据隔离开关（set_isolated） |
//! | `version` | 版本列表 / 远端列表 / 安装 / 卸载 / 默认版本 |
//! | `launch` | 启动窗口 / 运行 / 浏览器打开 / 重启 / 快捷方式 / 顶部栏脚本 |
//! | `scan` | 占用扫描（scan_versions / cancel_scan / scan_isolated_size）/ 维护 |
//! | `isolated` | 复制共享 home / 清理隔离数据 / 打开隔离目录 |
//! | `plugins` | dsh 插件管理（双向桥 B1） |
//! | `env` | 环境检查 / 运行时状态 / 便携运行时准备 |
//! | `backup` | 配置导出 / 导入预览 / 导入应用 |
//! | `config_ops` | 切换数据根 / 复用官方配置目录 |
//! | `update` | 检查更新 / 执行更新 / 退出 |
//! | `misc` | 打开 URL / 打开目录 / 前端日志上报 |
//!
//! 共享的进程映射类型与 `manager_dir` 仍在 crate 根（lib.rs）。

mod backup;
mod config_ops;
mod env;
mod isolated;
mod launch;
mod misc;
mod plugins;
mod scan;
mod state;
mod update;
mod version;

pub(crate) use backup::*;
pub(crate) use config_ops::*;
pub(crate) use env::*;
pub(crate) use isolated::*;
pub(crate) use launch::*;
pub(crate) use misc::*;
pub(crate) use plugins::*;
pub(crate) use scan::*;
pub(crate) use state::*;
pub(crate) use update::*;
pub(crate) use version::*;

use crate::config::{Config, Dirs};
use crate::logging;
use crate::window;
use crate::{actions, cli, config, envcheck, launcher, maintenance, runtime, sizecache, versions};
use crate::{lock_procs, manager_dir, ProcMap, NEXT_GEN};
use serde::Serialize;
use std::sync::atomic::AtomicBool;

/// v0.2.13：扫描取消标志。scan_versions 开头清零，cancel_scan 置位。
pub(crate) static SCAN_CANCEL: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_topbar_script：注入脚本的占位符替换与转义 ──

    #[test]
    fn topbar_script_substitutes_placeholders() {
        let s = build_topbar_script("0.2.0-rc.2", "http://127.0.0.1:1234/?token=abc");
        // 占位符应被替换
        assert!(!s.contains("__VER__"));
        assert!(!s.contains("__URL__"));
        assert!(s.contains("0.2.0-rc.2"));
        assert!(s.contains("http://127.0.0.1:1234/?token=abc"));
    }

    #[test]
    fn topbar_script_escapes_quote_and_backslash() {
        // 单引号、反斜杠应被转义，防注入 JS 单引号字符串字面量
        let s = build_topbar_script("v'1", "http://x/'y\\z");
        assert!(s.contains("v\\'1"), "单引号应被转义: {}", s);
        assert!(s.contains("http://x/\\'y"), "URL 中单引号应被转义");
    }

    /// topbar 与 Rust 侧的契约：事件名、通道、拖拽属性必须两边一致。
    ///
    /// 为什么需要这条：topbar.js 是纯 JS，Rust 编译器检查不到它。
    /// 改事件名时若只改了 JS（或只改 Rust 监听器），编译与单测全绿，
    /// 但运行时按钮静默失效——只能靠实机发现。这条断言把"两边名字对不上"
    /// 提前到 `cargo test`。
    ///
    /// 维护方式：改 `topbar.js` 的事件名或 `lib.rs` 的 `listen_any` 时，
    /// 同步改这里的期望值；三者（JS / 监听器 / 本断言）必须同时更新。
    #[test]
    fn topbar_script_emits_events_matching_rust_listeners() {
        let s = build_topbar_script("0.2.0", "http://127.0.0.1:1/");

        // 通道：remote 上下文只能走插件命令
        assert!(
            s.contains("plugin:event|emit"),
            "topbar 必须用 plugin:event|emit（remote 下应用命令被 ACL 拒）"
        );

        // 四个事件名：必须与 lib.rs 的 listen_any 一一对应
        for ev in [
            "topbar-log",
            "topbar-restart",
            "topbar-ctrl",
            "topbar-theme",
        ] {
            assert!(
                s.contains(ev),
                "topbar.js 缺少事件 {}（Rust 侧 listen_any 会收不到）",
                ev
            );
        }

        // 拖拽：Tauri 2 只认按下元素自身的属性，标题栏与 URL 空白区都要有
        let drag_count = s.matches("data-tauri-drag-region").count();
        assert!(
            drag_count >= 2,
            "data-tauri-drag-region 至少 2 处（标题栏 + urlWrap），实际 {}",
            drag_count
        );

        // 主题观察点：必须是 body 的属性。
        // 注意：不能用 s.contains("html[style]") 做反向断言——注释里就写着这句，
        // 会命中注释而非代码。改查真实的观察调用：documentElement（<html>）。
        assert!(
            s.contains("data-ds-dark-theme"),
            "主题观察必须基于 body[data-ds-dark-theme]"
        );
        assert!(
            !s.contains("documentElement"),
            "不得触碰 documentElement（观察 <html> 会与 dsh 渲染抢时序导致闪退）"
        );
    }

    // ── resolve_home：隔离 / 非隔离版本的 DSH_HOME 解析 ──

    #[test]
    fn resolve_home_shared_when_not_isolated() {
        let cfg = Config::default();
        let dirs = Dirs::new(std::env::temp_dir().join("dsh-rh-shared"));
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        assert_eq!(h, dirs.home);
    }

    #[test]
    fn resolve_home_isolated_uses_version_subdir() {
        let cfg = Config {
            isolated_versions: vec!["0.1.0".to_string()],
            ..Default::default()
        };
        let base = std::env::temp_dir().join(format!("dsh-rh-iso-{}", crate::logging::now_secs()));
        let _ = std::fs::remove_dir_all(&base);
        let dirs = Dirs::new(base.clone());
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        // 隔离版本 → <root>/versions/<ver>/home
        assert_eq!(h, dirs.versions.join("0.1.0").join("home"));
        // 该目录应被创建
        assert!(h.exists(), "隔离 home 应被创建");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_home_isolated_only_for_listed_version() {
        let cfg = Config {
            isolated_versions: vec!["0.1.0".to_string()],
            ..Default::default()
        };
        let dirs = Dirs::new(std::env::temp_dir().join("dsh-rh-mixed"));
        // 未列入隔离的版本 → 共享 home
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.2.0");
        assert_eq!(h, dirs.home);
    }

    #[test]
    fn resolve_home_official_when_flag_on() {
        // 开启 use_official_dsh_home → 非隔离版本解析到官方 ~/.dsh
        let cfg = Config {
            use_official_dsh_home: true,
            ..Default::default()
        };
        let dirs = Dirs::new(std::env::temp_dir().join("dsh-rh-official"));
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        let expected = crate::paths::official_dsh_home().expect("应能解析出用户主目录");
        assert_eq!(h, expected);
        assert_ne!(h, dirs.home, "不应再指向管理器共享 home");
    }

    #[test]
    fn resolve_home_isolated_beats_official_flag() {
        // 隔离优先级高于"复用官方"：即使全局开了复用，隔离版本仍用版本目录
        let cfg = Config {
            use_official_dsh_home: true,
            isolated_versions: vec!["0.1.0".to_string()],
            ..Default::default()
        };
        let base = std::env::temp_dir().join(format!("dsh-rh-prio-{}", crate::logging::now_secs()));
        let _ = std::fs::remove_dir_all(&base);
        let dirs = Dirs::new(base.clone());
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        assert_eq!(h, dirs.versions.join("0.1.0").join("home"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
