mod actions;
mod backup;
mod cli;
mod commands;
mod config;
mod console_decode;
mod dsplugins;
mod envcheck;
mod jobobj;
mod launcher;
mod logging;
mod maintenance;
mod paths;
#[cfg(unix)]
mod procreg;
mod runtime;
mod sizecache;
mod updater;
mod versions;
mod window;

use config::{Config, Dirs};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

/// 「版本号 → (窗口 label, 代次, dsh 子进程, Job 持有句柄)」映射。
/// - label 唯一化（带代次），避免 "a webview with label ... already exists"；
/// - 代次用于解决窗口替换时的 Destroyed 回调竞态：回调只在自己那一代仍是当前项时才 kill；
/// - Job 句柄用于"管理器退出即杀光子进程"：持有到进程被移除/窗口关闭时。
pub(crate) type ProcEntry = (String, u64, Child, Option<jobobj::ProcessGuard>);
pub(crate) type ProcMapInner = HashMap<String, ProcEntry>;
pub(crate) type ProcMap = Arc<Mutex<ProcMapInner>>;

/// 锁定进程表；若锁已中毒（某线程持锁时 panic），忽略中毒继续使用内部数据。
///
/// 为什么安全：`ProcMap` 的字段之间没有必须维持的跨字段不变量，中毒后继续用不会导致内存不安全，
/// 最坏是拿到一个不一致快照。比起让整个应用 panic，继续运行更符合"管理器不应因单个任务失败而挂掉"。
pub(crate) fn lock_procs(procs: &ProcMap) -> std::sync::MutexGuard<'_, ProcMapInner> {
    procs.lock().unwrap_or_else(|e| e.into_inner())
}

/// 全局单调递增的代次计数器（用于区分同名窗口的不同实例）
pub(crate) static NEXT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 进程启动时刻（run() 第一行）。用于启动耗时打点（v0.2.13）。
pub(crate) static PROC_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 窗口就绪耗时（毫秒）：进程启动 → Tauri setup 进入（含 WebView2 初始化 + 窗口创建）。
pub(crate) static WIN_READY_MS: std::sync::OnceLock<u128> = std::sync::OnceLock::new();

/// 启动模式（v0.2.14）：命令行带 --launch-version 时，记录待启动的版本号。
/// 前端 onMounted 主动查（比事件可靠——setup 里 emit 时前端可能还没监听）。
///
/// M2m（B1）：由 `OnceLock` 改为 `Mutex<Option>`。原因：launch-version 流程启动成功后
/// 会 close 主窗口；用户再双击图标触发 `ensure_main_window` 重建主窗口时，
/// 新窗口前端仍会读到旧值 → 卡在「正在启动」。重建前需要能**清除**它。
pub(crate) static LAUNCH_MODE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 锁定 `LAUNCH_MODE`；中毒时继续用内部数据（同 `lock_procs` 的理由）。
pub(crate) fn lock_launch_mode() -> std::sync::MutexGuard<'static, Option<String>> {
    LAUNCH_MODE.lock().unwrap_or_else(|e| e.into_inner())
}

/// M2m（B5）：判断第二实例与当前实例是否为同一份可执行文件。
///
/// 单实例锁按 app identifier 全局生效，同一用户下两个不同目录的副本也会互斥。
/// 此时**不能**把 `--launch-version` 转发给当前实例——不同副本可能用不同数据根，
/// 转发会操作错数据。应弹对话框说明并聚焦当前实例。
///
/// 比较前先 `canonicalize`（展开 8.3 短路径与符号链接）；Windows 忽略大小写。
pub(crate) fn is_same_exe(second: &str, current: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).ok();
    let a = canon(std::path::Path::new(second));
    let b = canon(current);

    match (a, b) {
        (Some(a), Some(b)) => {
            #[cfg(windows)]
            {
                a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
            }
            #[cfg(not(windows))]
            {
                a == b
            }
        }
        _ => {
            // canonicalize 失败（如路径已被删除）时退化为字符串比较
            #[cfg(windows)]
            {
                second.eq_ignore_ascii_case(&current.to_string_lossy())
            }
            #[cfg(not(windows))]
            {
                second == current.to_string_lossy()
            }
        }
    }
}


/// 管理器自身所在目录（config.json 的存放处）
pub(crate) fn manager_dir(_app: &tauri::AppHandle) -> PathBuf {
    manager_dir_plain()
}

/// 同上，但不依赖 AppHandle——供无头 CLI 模式使用。
pub(crate) fn manager_dir_plain() -> PathBuf {
    // 优先使用 exe 所在目录；开发模式下回退到当前工作目录
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            // 开发模式下 exe 在 target/debug，改用项目目录无意义，
            // 这里统一用 exe 目录，便携版即 exe 同目录
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 进程启动计时起点（v0.2.13 启动耗时打点）
    PROC_START.set(std::time::Instant::now()).ok();

    // 配置损坏检测（D18）：CLI 与 GUI 分流之前统一执行一次。
    // 副作用：config.json 解析失败时写 app.log 警告 + 重命名为 config.json.broken-<ts>。
    // 返回值丢弃——后续所有读取仍走纯读取的 Config::load（此时文件已被重命名，会返回默认值）。
    // 设计依据：docs/交互设计约定.md §2.5。
    {
        let mdir = manager_dir_plain();
        let _ = Config::load_checked(&mdir);
    }

    // ── 无头 CLI 模式 ──
    // 带任一 CLI 参数则不初始化 GUI，执行后按退出码退出。
    // 分流必须早于 tauri::Builder，避免白白起一个 GUI 进程。
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Ok(Some(cmd)) => {
            cli::setup_console();
            // --dry-run：破坏性命令只预览不执行（--list 等只读命令不受影响）
            // --json：结构化输出（只影响格式）
            let dry_run = cli::has_dry_run(&args);
            let json = cli::has_json(&args);
            let code = cli::run(cmd, dry_run, json);
            std::process::exit(code);
        }
        Ok(None) => {} // 无 CLI 参数 → 正常启动 GUI
        Err(msg) => {
            cli::setup_console();
            eprintln!("参数错误：{}", msg);
            eprintln!("用 --help 查看用法");
            std::process::exit(1);
        }
    }

    // v0.2.9 批 2：WebView2 早期检测（必须在 Tauri 初始化前——GUI 本体依赖它）。
    // 缺失时弹 Win32 原生框（不依赖 WebView2）引导安装；用户装完重开即可。
    // 仅在 GUI 路径检测（CLI 不需要 WebView2）。
    if !runtime::webview2_installed() {
        runtime::prompt_webview2_missing();
        std::process::exit(0);
    }

    // v0.2.14：启动模式必须在窗口/前端创建**之前**记录。
    // Tauri 在 setup 前就建了主窗口、前端可能早于 setup 完成 onMounted，
    // 故在此（run 开头）就 set，确保前端 get_launch_mode 一定查得到。
    if let Some(version) = window::parse_launch_version(&std::env::args().collect::<Vec<_>>()) {
        *lock_launch_mode() = Some(version);
    }

    // 进程级 AUMID 必须在任何窗口创建之前设置（Tauri 在 setup 回调前就建了主窗口）。
    // 否则任务栏会退化为"找指向本 exe 的快捷方式取图标"，被桌面 dsh 快捷方式污染。
    window::set_process_aumid();

    let procs: ProcMap = Arc::new(Mutex::new(HashMap::new()));
    let procs_setup = procs.clone();
    tauri::Builder::default()
        // M2m：单实例锁必须是第一个注册的 plugin（Tauri 要求）。
        // 回调在第二实例被检测到时于已运行实例内执行：
        //  - 带 --launch-version V → 转发给 launch_window 打开该版本内嵌窗口（保留版本快捷方式语义）
        //  - 否则 → 确保主窗口存在并聚焦（主窗口可能已被 launch-version 流程 close）
        .plugin(tauri_plugin_single_instance::init({
            let procs_for_si = procs.clone();
            move |app, args, _cwd| {
                // M2m（B3）：release 版 `windows_subsystem = "windows"` 没有控制台，
                // eprintln! 会全部丢失。关键失败路径一律同时写 `logs/app.log`。
                let log_app = |ctx: &str, msg: &str| {
                    let mdir = manager_dir(app);
                    let root = Config::load(&mdir).resolve_root(&mdir);
                    logging::log_app_error(&root, ctx, msg);
                    eprintln!("[dsh-multiver] {}: {}", ctx, msg);
                };

                // M2m（B5）：单实例锁按 app identifier 全局生效，同一用户下两个不同
                // 目录的副本也会互斥。此时不能把 --launch-version 转发给当前实例——
                // 不同副本可能用不同数据根，转发会操作错数据。改为弹对话框说明并聚焦。
                let second_exe = args.first().map(|s| s.as_str()).unwrap_or("");
                log_app(
                    "single-instance",
                    &format!(
                        "回调触发：second_exe={} args={:?}",
                        second_exe, args
                    ),
                );
                let same_copy = match std::env::current_exe() {
                    Ok(cur) => is_same_exe(second_exe, &cur),
                    Err(_) => true, // 拿不到当前 exe 时按同副本处理，保持原有行为
                };

                if !same_copy {
                    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
                    let cur = std::env::current_exe()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "(unknown)".to_string());
                    log_app(
                        "single-instance",
                        &format!(
                            "拦截不同副本启动：second={} current={}",
                            second_exe, cur
                        ),
                    );
                    let _ = window::ensure_main_window(app);
                    // 非阻塞：此回调运行在插件的事件窗口过程（主线程消息泵）里，
                    // blocking_show() 会阻塞消息泵，存在死锁与残留进程风险。
                    app.dialog()
                        .message(format!(
                            "同一时间只允许一个管理器运行。\n\n已运行的副本：\n{}\n\n本次启动的副本：\n{}\n\n请使用已运行的窗口，或先关闭它再启动本副本。",
                            cur, second_exe
                        ))
                        .title("已有管理器在运行")
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCustom("知道了".to_string()))
                        .show(|_| {});
                    return;
                }

                if let Some(version) = window::parse_launch_version(&args) {
                    let app_handle = app.clone();
                    let procs_for_launch = procs_for_si.clone();
                    let version_for_launch = version.clone();
                    tauri::async_runtime::spawn(async move {
                        match commands::launch_window(
                            app_handle.clone(),
                            procs_for_launch,
                            version_for_launch.clone(),
                        )
                        .await
                        {
                            Ok(msg) => {
                                eprintln!(
                                    "[dsh-multiver] 单实例转发 launch-version {}: {}",
                                    version_for_launch, msg
                                );
                            }
                            Err(e) => {
                                // B2：转发失败不能只写日志——屏幕上什么都不发生，用户以为没响应。
                                // 回退到「确保主窗口存在并聚焦」，并把错误落到日志供事后排查。
                                let mdir = manager_dir(&app_handle);
                                let root = Config::load(&mdir).resolve_root(&mdir);
                                logging::log_launch_error(
                                    &root,
                                    &format!(
                                        "单实例转发启动失败（版本 {}）: {}",
                                        version_for_launch, e
                                    ),
                                );
                                logging::log_app_error(
                                    &root,
                                    "single-instance",
                                    &format!(
                                        "转发 launch-version {} 失败，回退主窗口: {}",
                                        version_for_launch, e
                                    ),
                                );
                                eprintln!(
                                    "[dsh-multiver] 单实例转发启动失败（已回退主窗口）: {}",
                                    e
                                );
                                let _ = window::ensure_main_window(&app_handle);
                            }
                        }
                    });
                } else {
                    match window::ensure_main_window(app) {
                        Ok(path) => {
                            eprintln!("[dsh-multiver] 单实例：主窗口 {}", path);
                        }
                        Err(e) => {
                            log_app("single-instance", &format!("主窗口恢复失败: {}", e));
                        }
                    }
                }
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(procs.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::list_installed,
            commands::list_remote,
            commands::install_version,
            commands::clear_broken,
            commands::uninstall_version,
            commands::set_default,
            commands::run_version,
            commands::open_in_browser,
            commands::create_shortcut,
            commands::restart_version,
            commands::set_root,
            commands::open_dir,
            commands::open_url,
            commands::check_env,
            commands::runtime_status,
            commands::setup_runtime,
            commands::dismiss_runtime_prompt,
            commands::set_isolated,
            commands::set_use_official_home,
            commands::get_official_home,
            commands::scan_versions,
            commands::cancel_scan,
            commands::run_maintenance,
            commands::set_auto_maintenance,
            commands::scan_isolated_size,
            commands::copy_shared_to_isolated,
            commands::clear_isolated_data,
            commands::open_isolated_dir,
            commands::log_frontend,
            commands::check_update,
            commands::perform_update,
            commands::exit_app,
            commands::export_backup,
            commands::preview_import,
            commands::apply_import,
            commands::get_launch_mode,
            commands::show_main_window,
            commands::is_version_open,
            commands::list_running_versions,
            commands::ds_list_plugins,
            commands::ds_plugin_action,
            commands::ds_init_profile,
            commands::ds_open_profile_dir,
            commands::ds_profile_info,
            commands::ds_exemption_action,
            commands::ds_approve_builds,
            commands::ds_open_plugin_logs,
            commands::ds_check_updates,
            commands::ds_inspect_spec,
            commands::ds_probe_registries,
        ])
        .setup(move |app| {
            use tauri::Manager;
            // 窗口就绪打点：进程启动 → 此处（含 WebView2 初始化 + 窗口创建）
            if let Some(t) = PROC_START.get() {
                WIN_READY_MS.set(t.elapsed().as_millis()).ok();
            }
            // 注册各 AUMID 的任务栏图标（IconUri）。进程级 AUMID 已在 run() 开头设过。
            window::register_aumid_icons();
            // 主窗口（管理器）显式使用管理器自己的图标（exe 内嵌图标 = bundle.icon 的 icon.ico），
            // 避免被任务栏缓存或其它窗口/快捷方式图标影响而显示错误图标。
            #[cfg(windows)]
            if let Some(main) = app.get_webview_window("main") {
                if let Ok(img) = tauri::image::Image::from_app_icon_resource(256) {
                    let _ = main.set_icon(img);
                }
            }
            // 精简启动模式：命令行带 --launch-version <版本> 时，
            // 关闭默认主窗口，直接打开该版本的内嵌窗口。
            if let Some(version) =
                window::parse_launch_version(&std::env::args().collect::<Vec<_>>())
            {
                // v0.2.14：不隐藏主窗口——窗口始终可见，WebView2 持续渲染（hide→show 会有 ~900ms 恢复开销）。
                // 前端首帧即"启动态"（main.js 挂载前先查 get_launch_mode，作为初始 prop），不闪主界面。
                // 注意：不能 close 主窗口——若此时尚无其他窗口，Tauri 会认为
                // "所有窗口已关闭" 而直接退出事件循环，导致异步创建 dsh 窗口来不及执行。
                // 启动模式已在 run() 开头记录（LAUNCH_MODE），此处只需通知前端
                // 通知前端进入启动态（若前端已就绪，立即响应；未就绪则靠 get_launch_mode 兜底）
                {
                    use tauri::Emitter;
                    let _ = app.emit(
                        "launch-mode-start",
                        serde_json::json!({ "version": version.clone() }),
                    );
                }
                // launch_window 是 async 的；setup 是同步闭包，用 async_runtime 起任务
                let app_handle = app.handle().clone();
                let procs_for_launch = procs_setup.clone();
                let version_for_launch = version.clone();
                tauri::async_runtime::spawn(async move {
                    match commands::launch_window(
                        app_handle.clone(),
                        procs_for_launch,
                        version_for_launch.clone(),
                    )
                    .await
                    {
                        Ok(_) => {
                            // dsh 窗口已建好，此时再关闭主窗口
                            if let Some(main) = app_handle.get_webview_window("main") {
                                let _ = main.close();
                            }
                        }
                        Err(e) => {
                            // 无控制台，错误同时落盘，便于排查
                            let mdir = manager_dir(&app_handle);
                            let root = Config::load(&mdir).resolve_root(&mdir);
                            logging::log_launch_error(
                                &root,
                                &format!("版本 {} 启动失败: {}", version_for_launch, e),
                            );
                            eprintln!("[dsh-multiver] 启动失败: {}", e);
                            // R-13：快捷方式启动失败时通知前端进入错误态，
                            // 否则加载页会停在 preparing/waiting 不动。
                            {
                                use tauri::Emitter;
                                let _ = app_handle.emit(
                                    "launch-progress",
                                    serde_json::json!({
                                        "version": version_for_launch,
                                        "stage": "error",
                                        "error": e.to_string(),
                                    }),
                                );
                            }
                            // 启动失败时把主窗口显示出来，避免用户看到"什么都没有"
                            if let Some(main) = app_handle.get_webview_window("main") {
                                let _ = main.show();
                            }
                        }
                    }
                });
            }

            // Unix：启动时清理上次被强杀/崩溃残留的 dsh 进程组（GAP-010 兜底）。
            // 正常退出/关窗会注销登记，故这里通常无事可做；只有强杀场景才有残留。
            #[cfg(unix)]
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mdir = manager_dir(&app_handle);
                    let root = Config::load(&mdir).resolve_root(&mdir);
                    let n = procreg::cleanup_stale(&root);
                    if n > 0 {
                        logging::write_line(
                            &root.join("logs"),
                            "maintenance.log",
                            &format!("启动清理残留 dsh 进程组：{} 个", n),
                        );
                    }
                });
            }

            // 后台维护（不阻塞界面）：
            //  1) 立即清理孤立 webview 目录（毫秒级）
            //  2) 延迟 + 7 天节流跑 pnpm store prune（重 IO，独立线程 + 低优先级）
            {
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mdir = manager_dir(&app_handle);
                    let mut cfg = Config::load(&mdir);
                    // 用户关闭了自动维护 → 跳过
                    if !cfg.maintenance.auto_enabled {
                        return;
                    }
                    let root = cfg.resolve_root(&mdir);
                    let dirs = Dirs::new(root.clone());
                    let _ = dirs.ensure();

                    // 0) 回收站残留：上次卸载 rename 进来、后台没删净的，这里清掉
                    let trashed = maintenance::cleanup_trash(&dirs.trash);
                    if trashed > 0 {
                        logging::log_maintenance(&root, &format!("清空回收站：{} 项", trashed));
                    }

                    // 1) 孤立 webview：每次启动都能跑（成本极低）
                    let removed =
                        maintenance::cleanup_orphan_webviews(&dirs.versions, &dirs.webview);
                    cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
                    cfg.maintenance.last_cleanup_count = removed.len() as u64;
                    let _ = cfg.save(&mdir);
                    if !removed.is_empty() {
                        logging::log_maintenance(
                            &root,
                            &format!("清理孤立 webview：{:?}", removed),
                        );
                    }

                    // 2) store prune：距上次 >= 7 天才跑，且延迟 30 秒错开启动 IO
                    let due = match cfg.maintenance.last_prune_at {
                        Some(t) => {
                            maintenance::now_secs().saturating_sub(t)
                                >= maintenance::PRUNE_INTERVAL_SECS
                        }
                        None => true,
                    };
                    if due {
                        std::thread::sleep(std::time::Duration::from_secs(30));
                        match maintenance::run_store_prune(&dirs.store, &dirs.cache, &dirs.state) {
                            Ok(()) => {
                                let mut c2 = Config::load(&mdir);
                                c2.maintenance.last_prune_at = Some(maintenance::now_secs());
                                let _ = c2.save(&mdir);
                                logging::log_maintenance(&root, "store prune 完成");
                            }
                            Err(e) => {
                                logging::log_maintenance(
                                    &root,
                                    &format!("store prune 失败（下次重试）: {}", e),
                                );
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // 退出时兜底清理所有 dsh 子进程，避免残留
                let mut map = lock_procs(&procs);
                for (_, _, child, _job) in map.values_mut() {
                    // Unix：正常退出能跑到这里，注销登记（避免下次启动误清）
                    #[cfg(unix)]
                    {
                        let mdir = manager_dir(_app);
                        let root = Config::load(&mdir).resolve_root(&mdir);
                        crate::procreg::unregister(&root, child.id() as i32);
                    }
                    launcher::kill_tree(child);
                }
                map.clear();
            }
        });
}
