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
    Help,
    Version,
}

/// 判断某个参数是否是"命令级"CLI 开关。
///
/// 注意：`--launch-version`（精简启动，内部用）和 `--debug-progress`（开发者用）
/// **不在**此列——它们仍走 GUI 路径。
///
/// `--dry-run` 也**不在**此列：它是**修饰符**（不带命令时不构成 CLI 调用），
/// 单独出现应报错而非启动 GUI（由 `is_dry_run` 单独处理）。
fn is_cli_flag(a: &str) -> bool {
    matches!(
        a,
        "--list"
            | "--which"
            | "--install"
            | "--uninstall"
            | "--set-default"
            | "--maintenance"
            | "--info"
            | "--versions"
            | "--env"
            | "--isolate"
            | "--clean"
            | "--help"
            | "-h"
            | "--version"
            | "-V"
    )
}

/// 是否含 `--dry-run`（只预览不执行）。仅对破坏性命令有意义。
pub(crate) fn has_dry_run(args: &[String]) -> bool {
    args.iter().any(|a| a == "--dry-run")
}

/// 是否含 `--json`（结构化输出）。只影响输出格式，不改行为。
pub(crate) fn has_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

/// 解析命令行参数。
///
/// - `Ok(None)`：无 CLI 参数 → 启动 GUI
/// - `Ok(Some(cmd))`：执行该 CLI 命令
/// - `Err(msg)`：CLI 参数有误（调用方打印 msg + 用法后退出 1）
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    // `--dry-run` / `--json` 是**修饰符**，必须搭配一个命令。单独出现（或无其它 CLI 参数）
    // → 报错，避免误启动 GUI。
    if !args.iter().any(|a| is_cli_flag(a)) {
        if has_dry_run(args) {
            return Err("--dry-run 需要搭配一个命令（如 --install X --dry-run）".to_string());
        }
        if has_json(args) {
            return Err("--json 需要搭配一个命令（如 --list --json）".to_string());
        }
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
            "--which" => return Ok(Some(CliCommand::Which)),
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
            "--info" => {
                let version = next_value(args, i, "--info")?;
                return Ok(Some(CliCommand::Info { version }));
            }
            "--versions" => return Ok(Some(CliCommand::Versions)),
            "--env" => return Ok(Some(CliCommand::Env)),
            "--isolate" => {
                let version = next_value(args, i, "--isolate")?;
                // 默认开启；显式 --off 则关闭
                let on = !args.iter().any(|x| x == "--off");
                return Ok(Some(CliCommand::Isolate { version, on }));
            }
            "--clean" => return Ok(Some(CliCommand::Clean)),
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

#[cfg(unix)]
fn progress_is_tty() -> bool {
    // isatty 判断 stderr 是否连着终端；重定向/管道时返回 false → 静默，保持可管道
    unsafe { libc::isatty(libc::STDERR_FILENO) == 1 }
}

#[cfg(not(any(windows, unix)))]
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
///
/// - `dry_run`：破坏性命令只**打印将要做的事**，不实际执行
/// - `json`：输出 JSON（机读）；只影响格式，不改行为。`--json` 时 stdout 只有 JSON
pub fn run(cmd: CliCommand, dry_run: bool, json: bool) -> i32 {
    match cmd {
        CliCommand::Help => {
            print_help();
            0
        }
        CliCommand::Version => {
            if json {
                println!("{}", json_str(&serde_json::json!({ "version": VERSION })));
            } else {
                println!("dsh-multiver {}", VERSION);
            }
            0
        }
        // --list / --which 是只读命令，dry-run 对它们无意义（照常执行）
        CliCommand::List => cmd_list(json),
        CliCommand::Which => cmd_which(json),
        CliCommand::Install { version, registry } => cmd_install(&version, registry.as_deref(), dry_run, json),
        CliCommand::Uninstall { version } => cmd_uninstall(&version, dry_run, json),
        CliCommand::SetDefault { version } => cmd_set_default(&version, dry_run, json),
        CliCommand::Maintenance { kind } => cmd_maintenance(&kind, dry_run, json),
        CliCommand::Info { version } => cmd_info(&version, json),
        CliCommand::Versions => cmd_versions(json),
        CliCommand::Env => cmd_env(json),
        CliCommand::Isolate { version, on } => cmd_isolate(&version, on, dry_run, json),
        CliCommand::Clean => cmd_clean(dry_run, json),
    }
}

/// 序列化为单行 JSON（失败时退化为空对象，不 panic）。
fn json_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".to_string())
}

/// 统一的"操作结果"输出：JSON 模式输出 {"ok":bool,"message":...}，否则按原样打印。
/// 用于破坏性命令的成功/失败提示。
fn out_result(json: bool, ok: bool, message: &str) {
    if json {
        println!("{}", json_str(&serde_json::json!({ "ok": ok, "message": message })));
    } else if ok {
        println!("{}", message);
    } else {
        eprintln!("{}", message);
    }
}

/// `--list`：默认输出一行一个版本号（默认版本加 `* ` 前缀）；
/// `--json` 输出对象数组（含 is_default / isolated / installed_at 等）。
fn cmd_list(json: bool) -> i32 {
    let ctx = load_ctx();
    let list = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    if json {
        println!("{}", json_str(&list));
    } else {
        for v in &list {
            if v.is_default {
                println!("* {}", v.version);
            } else {
                println!("{}", v.version);
            }
        }
    }
    0
}

/// `--which`：输出默认版本 dsh 入口的绝对路径。
/// 无默认版本 / 入口不存在 → 失败（退出码 1）。
fn cmd_which(json: bool) -> i32 {
    let ctx = load_ctx();
    let Some(def) = ctx.cfg.default_version.as_deref() else {
        return which_fail(json, "未设置默认版本（先用 --set-default <版本>）");
    };
    let bin = ctx
        .dirs
        .versions
        .join(def)
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if !bin.exists() {
        return which_fail(json, &format!("默认版本 {} 的入口不存在：{}", def, bin.to_string_lossy()));
    }
    if json {
        println!("{}", json_str(&serde_json::json!({
            "version": def,
            "path": bin.to_string_lossy(),
        })));
    } else {
        println!("{}", bin.to_string_lossy());
    }
    0
}

fn which_fail(json: bool, msg: &str) -> i32 {
    if json {
        println!("{}", json_str(&serde_json::json!({ "ok": false, "error": msg })));
    } else {
        eprintln!("{}", msg);
    }
    1
}

/// `--install <版本> [--registry <url>]`
fn cmd_install(version: &str, registry: Option<&str>, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true,
                "action": "install",
                "version": version,
                "registry": registry,
            })));
        } else {
            println!("[dry-run] 将安装版本 {} 到 {}", version, ctx.dirs.versions.join(version).to_string_lossy());
            if let Some(r) = registry {
                println!("[dry-run] 使用 npm 源：{}", r);
            }
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        out_result(json, false, &format!("创建数据目录失败：{}", e));
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
        out_result(json, true, &msg);
        0
    } else {
        let kind = versions::classify_error(&msg);
        if kind == "private-package" || kind == "incomplete-version" {
            if !ctx.cfg.broken_versions.iter().any(|v| v == version) {
                ctx.cfg.broken_versions.push(version.to_string());
                let _ = ctx.cfg.save(&ctx.mdir);
            }
        }
        out_result(json, false, &msg);
        1
    }
}

/// `--uninstall <版本>`（与 GUI 的 uninstall_version 行为一致）
fn cmd_uninstall(version: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true, "action": "uninstall", "version": version,
                "installed": versions::exists(&ctx.dirs.versions, version),
            })));
            return 0;
        }
        let installed = versions::exists(&ctx.dirs.versions, version);
        println!("[dry-run] 将卸载版本 {}", version);
        if installed {
            println!("[dry-run] 该版本已安装");
        } else {
            println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
        }
        if ctx.cfg.default_version.as_deref() == Some(version) {
            println!("[dry-run] 注意：这是当前默认版本，卸载后会清除默认设置");
        }
        if ctx.cfg.isolated_versions.iter().any(|v| v == version) {
            println!("[dry-run] 注意：该版本开启了数据隔离，会一并移除隔离标记");
        }
        println!("[dry-run] 未执行任何操作");
        return 0;
    }

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
    crate::commands::regenerate_forward_script(&ctx.mdir, &ctx.cfg);

    // 快速卸载：rename 到回收站（瞬时），后台再删；CLI 为一次性进程，故同步删回收站
    let (ok, msg) = versions::uninstall_fast(&ctx.dirs.versions, &ctx.dirs.trash, version);
    if ok {
        // 顺带把该版本的 WebView2 数据目录也移入回收站
        let wv = ctx.dirs.webview.join(version);
        if wv.exists() {
            let _ = std::fs::create_dir_all(&ctx.dirs.trash);
            let dest = ctx.dirs.trash.join(format!("webview-{}-{}", version, std::process::id()));
            if std::fs::rename(&wv, &dest).is_err() {
                let _ = std::fs::remove_dir_all(&wv);
            }
        }
        // 不在这里同步清空回收站：CLI 也要保持"秒退"。
        // trash 残留由下次启动的维护任务（cleanup_trash）或 --maintenance 清理。
        out_result(json, true, &msg);
        0
    } else {
        out_result(json, false, &msg);
        1
    }
}

/// `--set-default <版本>`
fn cmd_set_default(version: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true, "action": "set-default", "version": version,
                "installed": versions::exists(&ctx.dirs.versions, version),
            })));
            return 0;
        }
        let installed = versions::exists(&ctx.dirs.versions, version);
        println!("[dry-run] 将把默认版本设为 {}", version);
        if installed {
            println!("[dry-run] 该版本已安装");
        } else {
            println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
        }
        println!("[dry-run] 会重生成终端转发脚本 dsh");
        println!("[dry-run] 未执行任何操作");
        // dry-run 下不因未安装而失败（只是预览）
        return 0;
    }
    if !versions::exists(&ctx.dirs.versions, version) {
        out_result(json, false, &format!("版本 {} 未安装", version));
        return 1;
    }
    ctx.cfg.default_version = Some(version.to_string());
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        out_result(json, false, &format!("保存配置失败：{}", e));
        return 1;
    }
    crate::commands::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    out_result(json, true, &format!("默认版本已设为 {}，dsh 命令已就绪", version));
    0
}

/// `--maintenance [--cleanup|--prune]`
fn cmd_maintenance(kind: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true, "action": "maintenance", "kind": kind,
            })));
            return 0;
        }
        if kind == "cleanup" || kind == "all" {
            println!("[dry-run] 将清理孤立 webview 缓存（{}）", ctx.dirs.webview.to_string_lossy());
        }
        if kind == "prune" || kind == "all" {
            println!("[dry-run] 将回收依赖仓库（pnpm store prune）");
        }
        println!("[dry-run] 未执行任何操作");
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        eprintln!("创建数据目录失败：{}", e);
        return 1;
    }

    let mut cleaned: Option<usize> = None;
    let mut pruned = false;
    if kind == "cleanup" || kind == "all" {
        let removed = maintenance::cleanup_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
        ctx.cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
        ctx.cfg.maintenance.last_cleanup_count = removed.len() as u64;
        cleaned = Some(removed.len());
        if !json {
            println!("已清理孤立缓存 {} 项", removed.len());
        }
    }

    if kind == "prune" || kind == "all" {
        match maintenance::run_store_prune(&ctx.dirs.store, &ctx.dirs.cache, &ctx.dirs.state) {
            Ok(()) => {
                ctx.cfg.maintenance.last_prune_at = Some(maintenance::now_secs());
                pruned = true;
                if !json {
                    println!("已回收依赖仓库");
                }
            }
            Err(e) => {
                let _ = ctx.cfg.save(&ctx.mdir);
                if json {
                    println!("{}", json_str(&serde_json::json!({ "ok": false, "error": format!("回收依赖仓库失败：{}", e) })));
                } else {
                    eprintln!("回收依赖仓库失败：{}", e);
                }
                return 1;
            }
        }
    }

    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        if json {
            println!("{}", json_str(&serde_json::json!({ "ok": false, "error": format!("保存配置失败：{}", e) })));
        } else {
            eprintln!("保存配置失败：{}", e);
        }
        return 1;
    }
    if json {
        println!("{}", json_str(&serde_json::json!({
            "ok": true, "kind": kind, "cleaned": cleaned, "pruned": pruned,
        })));
    }
    0
}

/// `--isolate <版本> [--off]`：开关数据隔离（默认开启；--off 关闭）。
/// 与 GUI 的 set_isolated 行为一致：改配置 + 预创建隔离目录 + 重生成转发脚本。
fn cmd_isolate(version: &str, on: bool, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        let installed = versions::exists(&ctx.dirs.versions, version);
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true, "action": "isolate", "version": version,
                "isolated": on, "installed": installed,
            })));
        } else {
            println!(
                "[dry-run] 将{}版本 {} 的数据隔离",
                if on { "开启" } else { "关闭" },
                version
            );
            if !installed {
                println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
            }
            println!("[dry-run] 会重生成终端转发脚本 dsh（若该版本是默认版本）");
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    ctx.cfg.isolated_versions.retain(|v| v != version);
    if on {
        ctx.cfg.isolated_versions.push(version.to_string());
        let _ = std::fs::create_dir_all(ctx.dirs.versions.join(version).join("home"));
    }
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        return info_fail(json, &format!("保存配置失败：{}", e));
    }
    crate::commands::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    let msg = if on {
        format!("{} 已开启数据隔离", version)
    } else {
        format!("{} 已关闭数据隔离", version)
    };
    out_result(json, true, &msg);
    0
}

/// `--clean`：清理孤立 webview 缓存 + 回收站（maintenance --cleanup 的超集）。
fn cmd_clean(dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    if dry_run {
        if json {
            println!("{}", json_str(&serde_json::json!({
                "dry_run": true, "action": "clean",
            })));
        } else {
            println!("[dry-run] 将清理孤立 webview 缓存（{}）", ctx.dirs.webview.to_string_lossy());
            println!("[dry-run] 将清空卸载回收站（{}）", ctx.dirs.trash.to_string_lossy());
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        return info_fail(json, &format!("创建数据目录失败：{}", e));
    }
    let removed = maintenance::cleanup_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
    let trashed = maintenance::cleanup_trash(&ctx.dirs.trash);
    if json {
        println!("{}", json_str(&serde_json::json!({
            "ok": true, "webviews": removed.len(), "trash": trashed,
        })));
    } else {
        println!("已清理孤立缓存 {} 项", removed.len());
        println!("已清空回收站 {} 项", trashed);
    }
    0
}

/// `--info <版本>`：查看单个版本详情（人读多行 / JSON 对象）。
fn cmd_info(version: &str, json: bool) -> i32 {
    let ctx = load_ctx();
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    let list = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    let Some(info) = list.iter().find(|v| v.version == version) else {
        return info_fail(json, &format!("版本 {} 未安装", version));
    };
    let size = versions::version_size_detail(&ctx.dirs.versions, version);
    // 非隔离时 DSH_HOME 可能是共享 home 或官方 ~/.dsh，界面/脚本都需要看到实际值
    let home = crate::commands::resolve_home(&ctx.cfg, &ctx.dirs, version);

    if json {
        println!("{}", json_str(&serde_json::json!({
            "version": info.version,
            "path": info.path,
            "is_default": info.is_default,
            "isolated": info.isolated,
            "shared_home": info.shared_home,
            "installed_at": info.installed_at,
            "home": home.to_string_lossy(),
            "size": size,
        })));
    } else {
        println!("版本:     {}", info.version);
        println!("路径:     {}", info.path);
        println!("默认:     {}", if info.is_default { "是" } else { "否" });
        if info.isolated {
            println!("隔离:     是（独立 home）");
        } else if ctx.cfg.use_official_dsh_home {
            println!("隔离:     否（官方配置目录）");
        } else {
            println!("隔离:     否（共享 home）");
        }
        println!("配置目录: {}", home.to_string_lossy());
        if !info.installed_at.is_empty() {
            println!("安装于:   {}", info.installed_at);
        }
        println!(
            "占用:     {}（复用 {} / 独占 {}）",
            fmt_bytes(size.total),
            fmt_bytes(size.shared_size),
            fmt_bytes(size.exclusive_size)
        );
    }
    0
}

fn info_fail(json: bool, msg: &str) -> i32 {
    if json {
        println!("{}", json_str(&serde_json::json!({ "ok": false, "error": msg })));
    } else {
        eprintln!("{}", msg);
    }
    1
}

/// `--versions`：列出远端可用版本（从 npm 查询，最新在前）。
fn cmd_versions(json: bool) -> i32 {
    let ctx = load_ctx();
    let (ok, list, err) =
        crate::actions::list_remote(&ctx.dirs.store, &ctx.dirs.cache, &ctx.dirs.state);
    if !ok {
        return info_fail(json, &format!("获取可用版本失败：{}", err));
    }
    if json {
        println!("{}", json_str(&list));
    } else {
        for v in &list {
            println!("{}", v);
        }
    }
    0
}

/// `--env`：环境检查。有致命项失败 → 退出码 1（便于脚本 if 判断）。
fn cmd_env(json: bool) -> i32 {
    let ctx = load_ctx();
    let items = crate::envcheck::run_all(&ctx.dirs.root);
    let fatal_fail = items.iter().any(|i| i.critical && !i.ok);
    if json {
        println!("{}", json_str(&items));
    } else {
        for it in &items {
            let mark = if it.ok { "✓" } else { "✗" };
            let crit = if it.critical { "" } else { "（非致命）" };
            println!("[{}] {}{}  {}", mark, it.name, crit, it.detail);
        }
    }
    if fatal_fail { 1 } else { 0 }
}

/// 人类可读的字节数（B / KB / MB / GB，保留两位）。
fn fmt_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.2} MB", f / MB)
    } else if f >= KB {
        format!("{:.2} KB", f / KB)
    } else {
        format!("{} B", n)
    }
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
  dsh-multiver --list [--json]
  dsh-multiver --which [--json]
  dsh-multiver --install <版本> [--registry <url>]
  dsh-multiver --uninstall <版本>
  dsh-multiver --set-default <版本>
  dsh-multiver --info <版本> [--json]
  dsh-multiver --versions [--json]
  dsh-multiver --env [--json]
  dsh-multiver --isolate <版本> [--off]
  dsh-multiver --clean
  dsh-multiver --maintenance [--cleanup | --prune]
  dsh-multiver --help
  dsh-multiver --version [--json]

说明：
  --list         列出已安装版本（一行一个；默认版本以 \"* \" 前缀标记）
  --which        输出默认版本 dsh 入口的绝对路径（无默认版本则失败）
  --info         查看单个版本详情（路径 / 默认 / 隔离 / 配置目录 / 占用）
  --versions     列出远端可用版本（从 npm 查询，最新在前）
  --env          环境检查（有致命项失败时退出码 1）
  --isolate      开启某版本的数据隔离（加 --off 则关闭）
  --clean        清理孤立缓存 + 回收站（maintenance --cleanup 的超集）
  --install      安装指定版本；--registry 可指定 npm 源（默认官方源）
  --uninstall    卸载指定版本
  --set-default  设为默认版本，并重生成终端 dsh 命令
  --maintenance  磁盘维护：--cleanup 清孤立缓存 / --prune 回收依赖仓库 / 不带则两者都做
  --dry-run      只预览不执行（可搭配 --install / --uninstall / --set-default / --maintenance）
  --json         结构化输出（机读）；只影响格式，不改行为

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
    fn which_command() {
        assert!(matches!(parse(&args(&["--which"])), Ok(Some(CliCommand::Which))));
    }

    #[test]
    fn info_command() {
        match parse(&args(&["--info", "0.1.7"])) {
            Ok(Some(CliCommand::Info { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        // 缺参数值 → 报错
        assert!(parse(&args(&["--info"])).is_err());
    }

    #[test]
    fn isolate_and_clean_commands() {
        match parse(&args(&["--isolate", "0.1.7"])) {
            Ok(Some(CliCommand::Isolate { version, on })) => {
                assert_eq!(version, "0.1.7");
                assert!(on, "默认应为开启");
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["--isolate", "0.1.7", "--off"])) {
            Ok(Some(CliCommand::Isolate { version, on })) => {
                assert_eq!(version, "0.1.7");
                assert!(!on, "--off 应为关闭");
            }
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["--isolate"])).is_err());
        assert!(matches!(parse(&args(&["--clean"])), Ok(Some(CliCommand::Clean))));
    }

    #[test]
    fn versions_and_env_commands() {
        assert!(matches!(parse(&args(&["--versions"])), Ok(Some(CliCommand::Versions))));
        assert!(matches!(parse(&args(&["--env"])), Ok(Some(CliCommand::Env))));
        // 修饰符可搭配
        assert!(matches!(parse(&args(&["--versions", "--json"])), Ok(Some(CliCommand::Versions))));
        assert!(matches!(parse(&args(&["--env", "--json"])), Ok(Some(CliCommand::Env))));
    }

    #[test]
    fn json_parses_as_modifier() {
        assert!(has_json(&args(&["--list", "--json"])));
        assert!(!has_json(&args(&["--list"])));
        // 只 --json（无命令）→ 报错（不启动 GUI）
        assert!(parse(&args(&["--json"])).is_err());
        // 命令仍正确解析
        assert!(matches!(parse(&args(&["--list", "--json"])), Ok(Some(CliCommand::List))));
        assert!(matches!(parse(&args(&["--which", "--json"])), Ok(Some(CliCommand::Which))));
    }

    #[test]
    fn dry_run_parses_as_modifier() {
        // --dry-run 是修饰符，不改变命令解析（由 lib.rs 侧读取）
        assert!(has_dry_run(&args(&["--install", "0.1.7", "--dry-run"])));
        assert!(!has_dry_run(&args(&["--install", "0.1.7"])));
        // 命令本身仍正确解析
        match parse(&args(&["--install", "0.1.7", "--dry-run"])) {
            Ok(Some(CliCommand::Install { version, .. })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        // 只 --dry-run（无命令）→ 报错（不启动 GUI）
        assert!(parse(&args(&["--dry-run"])).is_err());
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
