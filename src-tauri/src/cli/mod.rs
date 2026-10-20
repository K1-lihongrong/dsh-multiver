//! 无头 CLI 模式（v0.3.0 拆分为多文件）。
//!
//! 带任一 CLI 子命令时不初始化 GUI，执行完按退出码退出（成功 0 / 失败 1）。
//! 分流点在 `lib.rs` 的 `run()` 最前面——必须早于 Tauri 初始化。
//!
//! 命令（v0.3.0 起为子命令风格）：
//!   list / which / info <版本> / versions / env / doctor
//!   install <版本> [--registry <url>] / uninstall <版本> / set-default <版本>
//!   isolate <版本> [--off] / clean
//!   shortcut <版本> / copy-shared <版本> / clear-isolated <版本>
//!   export [<文件>] / import <文件> / run <版本>
//!   setup-runtime / update / maintenance [--cleanup|--prune]
//!   help / version
//!
//! 修饰符（保持 `--`）：`--json` / `--dry-run` / `--force` / `--registry <url>` / `--off`
//!
//! 设计取舍：
//! - **无交互确认**：破坏性命令靠"精确参数"保护
//! - **进度走 stderr**（单行 `\r` 刷新），非 TTY 时静默 → stdout 保持纯净、可管道
//! - release 版是 `windows_subsystem = "windows"`（无控制台），故先 AttachConsole 再输出

mod commands;
mod console;
mod help;
mod output;
mod parse;

pub use console::setup_console;
pub use parse::{has_dry_run, has_json, parse};

use crate::config::{Config, Dirs};
use std::path::PathBuf;

/// 本程序版本（取自 Cargo.toml）
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 解析出的 CLI 命令。
#[derive(Debug)]
pub enum CliCommand {
    List,
    /// 输出默认版本 dsh 入口的绝对路径（无默认版本 → 失败）
    Which,
    Install {
        version: String,
        registry: Option<String>,
    },
    Uninstall {
        version: String,
    },
    SetDefault {
        version: String,
    },
    /// kind: "cleanup" | "prune" | "all"
    Maintenance {
        kind: String,
    },
    /// 查看单个版本详情（版本号 / 路径 / 默认 / 隔离 / 安装日期 / 占用）
    Info {
        version: String,
    },
    /// 列出远端可用版本（从 npm 查询，最新在前）
    Versions,
    /// 环境检查（Node / pnpm / 根目录可写 / 磁盘 / npm 源连通）
    Env,
    /// 开关某版本的数据隔离（on=true 开启，false 关闭）
    Isolate {
        version: String,
        on: bool,
    },
    /// 清理：孤立 webview 缓存 + 回收站（等价 maintenance --cleanup + trash）
    Clean,
    /// 一键诊断：聚合检查（目录 / broken / 残留登记 / 环境 / 日志）
    Doctor,
    /// 为某版本创建桌面快捷方式
    Shortcut {
        version: String,
    },
    /// 把共享 home 数据复制到某隔离版本的独立 home
    CopyShared {
        version: String,
    },
    /// 清空某隔离版本的独立 home（保留版本本身）
    ClearIsolated {
        version: String,
    },
    /// 导出配置 + 版本清单到指定文件（换机迁移用）
    Export {
        file: Option<String>,
    },
    /// 启动 dsh web（新控制台 + 自动打开系统浏览器）
    Run {
        version: String,
    },
    /// 导入配置（从 export 生成的备份文件）
    Import {
        file: String,
    },
    /// 准备便携运行时：检测缺失的 Node/pnpm 并下载到数据根
    SetupRuntime,
    /// 检查并执行自动更新
    Update,
    Help,
    Version,
}

/// 公共上下文：管理器目录 + 配置 + 各子目录。
pub(crate) struct Ctx {
    pub mdir: PathBuf,
    pub cfg: Config,
    pub dirs: Dirs,
}

pub(crate) fn load_ctx() -> Ctx {
    let mdir = crate::manager_dir_plain();
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    Ctx { mdir, cfg, dirs }
}

/// 执行 CLI 命令，返回进程退出码（成功 0 / 失败 1）。
pub fn run(cmd: CliCommand, dry_run: bool, json: bool) -> i32 {
    commands::dispatch(cmd, dry_run, json)
}
