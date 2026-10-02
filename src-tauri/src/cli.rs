//! 无头 CLI 模式（v0.1.8）。
//!
//! 带任一 CLI 参数时**不初始化 GUI**，执行完按退出码退出（成功 0 / 失败 1）。
//! 分流点在 `lib.rs` 的 `run()` 最前面——必须早于 Tauri 初始化。
//!
//! 命令：
//!   --list                               列出已安装版本（默认版本以 `* ` 前缀标记）
//!   --install <版本> [--registry <url>]  无头安装
//!   --uninstall <版本>                   无头卸载
//!   --set-default <版本>                 设默认版本（会重生成 dsh.cmd）
//!   --maintenance [--cleanup|--prune]    无头维护（默认 all）
//!   --help / -h                          帮助
//!   --version / -V                       版本号
//!
//! 设计取舍：
//! - **无交互确认**：破坏性命令靠"精确参数"保护（与整合包分支的 `--import --yes` 一致）
//! - **进度走 stderr**（单行 `\r` 刷新），非 TTY 时静默 → stdout 保持纯净、可管道
//! - release 版是 `windows_subsystem = "windows"`（无控制台），故先 AttachConsole 再输出

use crate::config::{Config, Dirs};
use crate::{maintenance, versions};
use std::path::PathBuf;

/// 本程序版本（取自 Cargo.toml）
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 解析出的 CLI 命令。
#[derive(Debug)]
pub enum CliCommand {
    List,
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
    Help,
    Version,
}

/// 判断某个参数是否是"命令级"CLI 开关。
///
/// 注意：`--launch-version`（精简启动，内部用）和 `--debug-progress`（开发者用）
/// **不在**此列——它们仍走 GUI 路径。
fn is_cli_flag(a: &str) -> bool {
    matches!(
        a,
        "--list"
            | "--install"
            | "--uninstall"
            | "--set-default"
            | "--maintenance"
            | "--help"
            | "-h"
            | "--version"
            | "-V"
    )
}

/// 解析命令行参数。
///
/// - `Ok(None)`：无 CLI 参数 → 启动 GUI
/// - `Ok(Some(cmd))`：执行该 CLI 命令
/// - `Err(msg)`：CLI 参数有误（调用方打印 msg + 用法后退出 1）
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    if !args.iter().any(|a| is_cli_flag(a)) {
        // 没有 CLI 参数：GUI 只认 --launch-version（精简启动）和 --debug-progress（开发者用），
        // 其余 -- 开头的参数大概率是打错字 —— 直接报错，别默默弹个 GUI 窗口。
        const GUI_FLAGS: [&str; 2] = ["--launch-version", "--debug-progress"];
        for a in args {
            if a.starts_with("--") && !GUI_FLAGS.contains(&a.as_str()) {
                return Err(format!("未知参数：{}", a));
            }
        }
        return Ok(None);
    }

    for (i, a) in args.iter().enumerate() {
        match a.as_str() {
            "--help" | "-h" => return Ok(Some(CliCommand::Help)),
            "--version" | "-V" => return Ok(Some(CliCommand::Version)),
            "--list" => return Ok(Some(CliCommand::List)),
            "--install" => {
                let version = next_value(args, i, "--install")?;
                let registry = find_opt(args, "--registry");
                return Ok(Some(CliCommand::Install { version, registry }));
            }
            "--uninstall" => {
                let version = next_value(args, i, "--uninstall")?;
                return Ok(Some(CliCommand::Uninstall { version }));
            }
            "--set-default" => {
                let version = next_value(args, i, "--set-default")?;
                return Ok(Some(CliCommand::SetDefault { version }));
            }
            "--maintenance" => {
                let kind = if args.iter().any(|x| x == "--cleanup") {
                    "cleanup"
                } else if args.iter().any(|x| x == "--prune") {
                    "prune"
                } else {
                    "all"
                };
                return Ok(Some(CliCommand::Maintenance {
                    kind: kind.to_string(),
                }));
            }
            _ => {}
        }
    }
    Err("无法识别的命令行参数组合".to_string())
}

/// 取 `args[i]` 后面那个值（不能是另一个 `--flag`）
fn next_value(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    match args.get(i + 1) {
        Some(v) if !v.starts_with("--") => Ok(v.clone()),
        _ => Err(format!("{} 缺少参数值", flag)),
    }
}

/// 在整个参数表里找 `key <value>`，返回 value（与 key 的先后位置无关）
fn find_opt(args: &[String], key: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == key {
            return it.next().cloned();
        }
    }
    None
}

// ─────────────────────────── 控制台接入 ───────────────────────────

/// 把进程接到父控制台，使 release 版（`windows_subsystem = "windows"`）也能在
/// cmd / PowerShell 里直接看到 CLI 输出。
///
/// 规则：
/// - stdout 已重定向到管道 / 文件 → 保持原样（别抢走用户的 `> out.txt`）
/// - 否则尝试 `AttachConsole(父进程)`，成功则把 stdout/stderr 指到 `CONOUT$`
/// - 顺带把控制台输出代码页设为 UTF-8（65001），避免中文乱码
#[cfg(windows)]
pub fn setup_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileType, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_TYPE_DISK, FILE_TYPE_PIPE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetConsoleOutputCP, SetStdHandle, ATTACH_PARENT_PROCESS,
        STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    unsafe {
        // stdout 已被重定向到管道 / 文件 → 保持原样
        let existing = GetStdHandle(STD_OUTPUT_HANDLE);
        if !existing.is_null() && existing != INVALID_HANDLE_VALUE {
            let t = GetFileType(existing);
            if t == FILE_TYPE_PIPE || t == FILE_TYPE_DISK {
                return;
            }
        }
        // 附加到父进程控制台（在 cmd 里直接运行时生效）
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        // 打开 CONOUT$，把它设为进程的 stdout / stderr
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let h = CreateFileW(
            name.as_ptr(),
            FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, h);
            SetStdHandle(STD_ERROR_HANDLE, h);
            // 控制台默认代码页可能不是 UTF-8，中文会乱码
            SetConsoleOutputCP(65001);
        }
    }
}

#[cfg(not(windows))]
pub fn setup_console() {}

/// stderr 是否是真控制台（决定要不要画 `\r` 进度行）。
/// 重定向到文件/管道时返回 false → 静默，保持输出可管道。
#[cfg(windows)]
fn progress_is_tty() -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE};
    unsafe {
        let h = GetStdHandle(STD_ERROR_HANDLE);
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: u32 = 0;
        GetConsoleMode(h, &mut mode) != 0
    }
}

#[cfg(not(windows))]
fn progress_is_tty() -> bool {
    false
}

// ─────────────────────────── 执行 ───────────────────────────

/// 公共上下文：管理器目录 + 配置 + 各子目录。
struct Ctx {
    mdir: PathBuf,
    cfg: Config,
    dirs: Dirs,
}

fn load_ctx() -> Ctx {
    let mdir = crate::manager_dir_plain();
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    Ctx { mdir, cfg, dirs }
}

/// 执行 CLI 命令，返回进程退出码（成功 0 / 失败 1）。
pub fn run(cmd: CliCommand) -> i32 {
    match cmd {
        CliCommand::Help => {
            print_help();
            0
        }
        CliCommand::Version => {
            println!("dsh-multiver {}", VERSION);
            0
        }
        CliCommand::List => cmd_list(),
        CliCommand::Install { version, registry } => cmd_install(&version, registry.as_deref()),
        CliCommand::Uninstall { version } => cmd_uninstall(&version),
        CliCommand::SetDefault { version } => cmd_set_default(&version),
        CliCommand::Maintenance { kind } => cmd_maintenance(&kind),
    }
}

/// `--list`：一行一个版本号；默认版本加 `* ` 前缀。
fn cmd_list() -> i32 {
    let ctx = load_ctx();
    let list = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    for v in &list {
        if v.is_default {
            println!("* {}", v.version);
        } else {
            println!("{}", v.version);
        }
    }
    0
}

/// `--install <版本> [--registry <url>]`
fn cmd_install(version: &str, registry: Option<&str>) -> i32 {
    let mut ctx = load_ctx();
    if let Err(e) = ctx.dirs.ensure() {
        eprintln!("创建数据目录失败：{}", e);
        return 1;
    }

    let tty = progress_is_tty();
    let last = std::cell::Cell::new(u32::MAX);
    let on_progress = |ev: &versions::ProgressEvent| {
        if !tty {
            return;
        }
        let pct = pct_of(ev);
        if pct != last.get() {
            last.set(pct);
            eprint!("\r正在安装 {}  {:>3}%  {}", version, pct, ev.stage);
            let _ = std::io::Write::flush(&mut std::io::stderr());
        }
    };

    let (ok, msg) = versions::install(
        &ctx.dirs.versions,
        &ctx.dirs.store,
        &ctx.dirs.cache,
        &ctx.dirs.state,
        version,
        &on_progress,
        registry,
    );
    if tty {
        eprintln!(); // 收尾进度行
    }

    // 维护 broken_versions（与 GUI 的 install_version 保持一致）
    if ok {
        if ctx.cfg.broken_versions.iter().any(|v| v == version) {
            ctx.cfg.broken_versions.retain(|v| v != version);
            let _ = ctx.cfg.save(&ctx.mdir);
        }
        println!("{}", msg);
        0
    } else {
        let kind = versions::classify_error(&msg);
        if kind == "private-package" || kind == "incomplete-version" {
            if !ctx.cfg.broken_versions.iter().any(|v| v == version) {
                ctx.cfg.broken_versions.push(version.to_string());
                let _ = ctx.cfg.save(&ctx.mdir);
            }
        }
        eprintln!("{}", msg);
        1
    }
}

/// `--uninstall <版本>`（与 GUI 的 uninstall_version 行为一致）
fn cmd_uninstall(version: &str) -> i32 {
    let mut ctx = load_ctx();

    // 卸载的是默认版本 / 隔离版本 → 同步清配置，并重生成转发脚本
    let mut need_save = false;
    if ctx.cfg.default_version.as_deref() == Some(version) {
        ctx.cfg.default_version = None;
        need_save = true;
    }
    if ctx.cfg.isolated_versions.iter().any(|v| v == version) {
        ctx.cfg.isolated_versions.retain(|v| v != version);
        need_save = true;
    }
    if need_save {
        let _ = ctx.cfg.save(&ctx.mdir);
    }
    crate::regenerate_forward_script(&ctx.mdir, &ctx.cfg);

    let (ok, msg) = versions::uninstall(&ctx.dirs.versions, version);
    if ok {
        // 顺带清理该版本的 WebView2 数据目录（失败不影响卸载结果）
        let _ = std::fs::remove_dir_all(ctx.dirs.webview.join(version));
        println!("{}", msg);
        0
    } else {
        eprintln!("{}", msg);
        1
    }
}

/// `--set-default <版本>`
fn cmd_set_default(version: &str) -> i32 {
    let mut ctx = load_ctx();
    if !versions::exists(&ctx.dirs.versions, version) {
        eprintln!("版本 {} 未安装", version);
        return 1;
    }
    ctx.cfg.default_version = Some(version.to_string());
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        eprintln!("保存配置失败：{}", e);
        return 1;
    }
    crate::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    println!("默认版本已设为 {}，dsh 命令已就绪", version);
    0
}

/// `--maintenance [--cleanup|--prune]`
fn cmd_maintenance(kind: &str) -> i32 {
    let mut ctx = load_ctx();
    if let Err(e) = ctx.dirs.ensure() {
        eprintln!("创建数据目录失败：{}", e);
        return 1;
    }

    if kind == "cleanup" || kind == "all" {
        let removed = maintenance::cleanup_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
        ctx.cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
        ctx.cfg.maintenance.last_cleanup_count = removed.len() as u64;
        println!("已清理孤立缓存 {} 项", removed.len());
    }

    if kind == "prune" || kind == "all" {
        match maintenance::run_store_prune(&ctx.dirs.store, &ctx.dirs.cache, &ctx.dirs.state) {
            Ok(()) => {
                ctx.cfg.maintenance.last_prune_at = Some(maintenance::now_secs());
                println!("已回收依赖仓库");
            }
            Err(e) => {
                let _ = ctx.cfg.save(&ctx.mdir);
                eprintln!("回收依赖仓库失败：{}", e);
                return 1;
            }
        }
    }

    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        eprintln!("保存配置失败：{}", e);
        return 1;
    }
    0
}

/// 由 ProgressEvent 估算总进度百分比（0-100）。
/// 简化版加权：每个阶段等权，阶段内按 fraction 推进。
fn pct_of(ev: &versions::ProgressEvent) -> u32 {
    let done = (ev.step.saturating_sub(1)) as f32 + ev.fraction;
    let total = ev.total.max(1) as f32;
    ((done / total) * 100.0).round().clamp(0.0, 100.0) as u32
}

fn print_help() {
    println!(
        "dsh-multiver {} —— DSH 多版本管理器（无头 CLI）

用法：
  dsh-multiver --list
  dsh-multiver --install <版本> [--registry <url>]
  dsh-multiver --uninstall <版本>
  dsh-multiver --set-default <版本>
  dsh-multiver --maintenance [--cleanup | --prune]
  dsh-multiver --help
  dsh-multiver --version

说明：
  --list         列出已安装版本（一行一个；默认版本以 \"* \" 前缀标记）
  --install      安装指定版本；--registry 可指定 npm 源（默认官方源）
  --uninstall    卸载指定版本
  --set-default  设为默认版本，并重生成终端 dsh 命令
  --maintenance  磁盘维护：--cleanup 清孤立缓存 / --prune 回收依赖仓库 / 不带则两者都做

  不带上述任一参数时，启动图形界面。
  退出码：成功 0 / 失败 1。",
        VERSION
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_returns_none() {
        assert!(matches!(parse(&args(&[])), Ok(None)));
    }

    #[test]
    fn gui_flags_pass_through() {
        // --launch-version / --debug-progress 属 GUI 路径，返回 None
        assert!(matches!(parse(&args(&["--launch-version", "0.1.7"])), Ok(None)));
        assert!(matches!(parse(&args(&["--debug-progress"])), Ok(None)));
    }

    #[test]
    fn unknown_double_dash_is_error() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["--launch-version", "0.1.7", "--typo"])).is_err());
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Some(CliCommand::Help))));
        assert!(matches!(parse(&args(&["-h"])), Ok(Some(CliCommand::Help))));
        assert!(matches!(parse(&args(&["--version"])), Ok(Some(CliCommand::Version))));
        assert!(matches!(parse(&args(&["-V"])), Ok(Some(CliCommand::Version))));
    }

    #[test]
    fn list_command() {
        assert!(matches!(parse(&args(&["--list"])), Ok(Some(CliCommand::List))));
    }

    #[test]
    fn install_with_and_without_registry() {
        match parse(&args(&["--install", "0.1.7"])) {
            Ok(Some(CliCommand::Install { version, registry })) => {
                assert_eq!(version, "0.1.7");
                assert!(registry.is_none());
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["--install", "0.1.7", "--registry", "https://r.example"])) {
            Ok(Some(CliCommand::Install { version, registry })) => {
                assert_eq!(version, "0.1.7");
                assert_eq!(registry.as_deref(), Some("https://r.example"));
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn install_missing_value_is_error() {
        assert!(parse(&args(&["--install"])).is_err());
        assert!(parse(&args(&["--install", "--registry", "x"])).is_err());
    }

    #[test]
    fn uninstall_and_set_default() {
        match parse(&args(&["--uninstall", "0.1.6"])) {
            Ok(Some(CliCommand::Uninstall { version })) => assert_eq!(version, "0.1.6"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["--set-default", "0.1.6"])) {
            Ok(Some(CliCommand::SetDefault { version })) => assert_eq!(version, "0.1.6"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn maintenance_kinds() {
        match parse(&args(&["--maintenance"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "all"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["--maintenance", "--cleanup"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "cleanup"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["--maintenance", "--prune"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "prune"),
            other => panic!("unexpected: {:?}", other),
        }
    }
}
