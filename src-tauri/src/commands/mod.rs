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
mod instances;
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
pub(crate) use instances::*;
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
#[allow(unused_imports)]
use crate::{lock_procs, manager_dir, AppCtx, ProcMap};
use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_topbar_script：注入脚本的占位符替换与转义 ──

    #[test]
    fn topbar_script_substitutes_placeholders() {
        let s = build_topbar_script(
            "0.2.0-rc.2",
            "0.2.0-rc.2 · 对比用",
            "http://127.0.0.1:1234/?token=abc",
            "deadbeef",
        );
        // 占位符应被替换
        assert!(!s.contains("__VER__"));
        assert!(!s.contains("__LABEL__"));
        assert!(!s.contains("__URL__"));
        assert!(!s.contains("__TOKEN__"));
        assert!(s.contains("0.2.0-rc.2"));
        assert!(s.contains("0.2.0-rc.2 · 对比用"));
        assert!(s.contains("deadbeef"));
        assert!(s.contains("http://127.0.0.1:1234/?token=abc"));
    }

    #[test]
    fn topbar_script_escapes_quote_and_backslash() {
        // 单引号、反斜杠应被转义，防注入 JS 单引号字符串字面量
        let s = build_topbar_script("v'1", "v'1 · A", "http://x/'y\\z", "tok'en");
        assert!(s.contains("v\\'1"), "单引号应被转义: {}", s);
        assert!(s.contains("http://x/\\'y"), "URL 中单引号应被转义");
        assert!(s.contains("tok\\'en"), "token 中单引号应被转义");
    }

    #[test]
    fn topbar_token_is_non_empty_and_unique() {
        let a = new_topbar_token();
        let b = new_topbar_token();
        assert_eq!(a.len(), 32, "token 应为 128 bit 十六进制");
        assert_ne!(a, b, "每窗口 token 必须不同");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// topbar 与 Rust 侧的契约：事件名、通道、拖拽属性必须两边一致。
    ///
    /// 为什么需要这条：topbar.js 是纯 JS，Rust 编译器检查不到它。
    /// 改事件名时若只改了 JS（或只改 Rust 监听器），编译与单测全绿，
    /// 但运行时按钮静默失效——只能靠实机发现。这条断言把"两边名字对不上"
    /// 提前到 `cargo test`。
    ///
    /// 维护方式：改 `topbar.js` 的事件名或 `launch.rs` 的 `window.listen` 时，
    /// 同步改这里的期望值；三者（JS / 监听器 / 本断言）必须同时更新。
    #[test]
    fn topbar_script_emits_events_matching_rust_listeners() {
        let s = build_topbar_script("0.2.0", "0.2.0", "http://127.0.0.1:1/", "deadbeef");

        // 通道：remote 上下文只能走插件命令
        assert!(
            s.contains("plugin:event|emit_to"),
            "topbar 必须用 plugin:event|emit_to（remote 下应用命令被 ACL 拒，且全局 emit 会串窗口）"
        );

        // emit_to 必须带当前窗口 label，否则 window.listen 无法按窗口隔离
        assert!(
            s.contains("metadata.currentWindow") && s.contains("'WebviewWindow'"),
            "topbar 必须用当前窗口 label 定向 emit_to"
        );
        assert!(
            s.contains("token: TOKEN") && s.contains("var TOKEN = 'deadbeef'"),
            "topbar 事件必须携带每窗口 token"
        );

        // 四个事件名：必须与 launch.rs 的窗口级监听器一一对应
        for ev in [
            "topbar-log",
            "topbar-restart",
            "topbar-ctrl",
            "topbar-theme",
        ] {
            assert!(
                s.contains(ev),
                "topbar.js 缺少事件 {}（Rust 侧 window.listen 会收不到）",
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

    /// A1 安全回归：topbar 监听必须按窗口绑定，不能再回到全局 listen_any。
    /// 全局监听 + 信任 payload 的 version 会让任意 dsh 页面关闭/重启别的版本窗口。
    #[test]
    fn topbar_listeners_are_window_scoped() {
        let launch = include_str!("launch.rs");
        assert_eq!(
            launch.matches(".listen(\"topbar-").count(),
            4,
            "四个 topbar 事件都必须用 window.listen 绑定"
        );
        assert!(
            !launch.contains("app.listen_any("),
            "launch.rs 不得出现全局 listen_any"
        );
        assert_eq!(
            launch
                .matches("authenticated_topbar_payload(&event")
                .count(),
            4,
            "四个监听器都必须校验窗口 token"
        );
        let lib = include_str!("../lib.rs");
        assert!(
            !lib.contains("app.listen_any("),
            "lib.rs 不得保留全局 topbar 监听器"
        );
    }

    /// A1 token 校验纯函数：匹配才放行，不匹配 / 缺失都拒绝。
    #[test]
    fn topbar_token_must_match() {
        let payload = r#"{"token":"good","action":"close"}"#;
        assert!(topbar_payload_if_token(payload, "good").is_some());
        assert!(topbar_payload_if_token(payload, "bad").is_none());
        assert!(topbar_payload_if_token(r#"{"action":"close"}"#, "good").is_none());
        assert!(topbar_payload_if_token("not-json", "good").is_none());
    }

    /// A2 权限回归：内嵌 dsh 窗口只保留事件 emit 与拖拽；三键由 Rust 处理，不需要 JS 窗口权限。
    #[test]
    fn capabilities_are_split_with_minimal_embedded_permissions() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        assert!(
            !base.join("default.json").exists(),
            "v0.3.7 起 capability 必须按 main / dsh 拆分，不能保留合并版"
        );

        let dsh: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(base.join("dsh.json")).unwrap()).unwrap();
        let dsh_perms: Vec<&str> = dsh["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            dsh_perms,
            vec![
                "core:event:allow-emit-to",
                "core:window:allow-start-dragging"
            ],
            "内嵌窗口权限必须最小化"
        );
        assert!(
            !dsh_perms.iter().any(|p| p.starts_with("opener:")
                || p.starts_with("dialog:")
                || *p == "core:default"),
            "内嵌窗口不得保留 opener / dialog / core:default"
        );

        let main: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(base.join("main.json")).unwrap())
                .unwrap();
        let main_perms = main["permissions"].as_array().unwrap();
        for required in ["core:default", "opener:default", "dialog:default"] {
            assert!(
                main_perms.iter().any(|v| v.as_str() == Some(required)),
                "主窗口缺少权限 {required}"
            );
        }
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
        let mut cfg = Config::default();
        let base = std::env::temp_dir().join(format!("dsh-rh-iso-{}", crate::logging::now_secs()));
        let _ = std::fs::remove_dir_all(&base);
        let dirs = Dirs::new(base.clone());
        cfg.ensure_default_instance(&crate::domain::VersionId::new("0.1.0").unwrap(), &base)
            .unwrap();
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        // 隔离版本 → <root>/homes/<实例 id>
        assert_eq!(h, dirs.homes.join("i1"));
        assert!(h.exists(), "隔离 home 应被创建");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_home_missing_selected_target_uses_shared() {
        let mut cfg = Config::default();
        cfg.version_instances.insert(
            crate::domain::VersionId::new("0.1.0").unwrap(),
            vec![crate::domain::InstanceId::new("i1").unwrap()],
        );
        let dirs = Dirs::new(std::env::temp_dir().join("dsh-rh-missing-target"));
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        assert_eq!(h, dirs.home, "selected_target 缺失时不得隐式选第一个实例");
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
        let mut cfg = Config {
            use_official_dsh_home: true,
            ..Default::default()
        };
        let base = std::env::temp_dir().join(format!("dsh-rh-prio-{}", crate::logging::now_secs()));
        let _ = std::fs::remove_dir_all(&base);
        let dirs = Dirs::new(base.clone());
        cfg.ensure_default_instance(&crate::domain::VersionId::new("0.1.0").unwrap(), &base)
            .unwrap();
        let h = crate::paths::resolve_home(&cfg, &dirs, "0.1.0");
        assert_eq!(h, dirs.homes.join("i1"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
