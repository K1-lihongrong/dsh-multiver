mod actions;
mod backup;
mod cli;
mod commands;
mod config;
mod envcheck;
mod jobobj;
mod launcher;
mod logging;
mod maintenance;
#[cfg(unix)]
mod procreg;
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

    // 进程级 AUMID 必须在任何窗口创建之前设置（Tauri 在 setup 回调前就建了主窗口）。
    // 否则任务栏会退化为"找指向本 exe 的快捷方式取图标"，被桌面 dsh 快捷方式污染。
    window::set_process_aumid();

    let procs: ProcMap = Arc::new(Mutex::new(HashMap::new()));
    let procs_setup = procs.clone();
    tauri::Builder::default()
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
            commands::set_isolated,
            commands::set_use_official_home,
            commands::get_official_home,
            commands::scan_version_size,
            commands::run_maintenance,
            commands::set_auto_maintenance,
            commands::scan_isolated_size,
            commands::copy_shared_to_isolated,
            commands::clear_isolated_data,
            commands::open_isolated_dir,
            commands::log_frontend,
            commands::check_update,
            commands::export_backup,
            commands::preview_import,
            commands::apply_import,
        ])
        .setup(move |app| {
            use tauri::Manager;
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
            if let Some(version) = window::parse_launch_version(&std::env::args().collect::<Vec<_>>()) {
                // 注意：不能立刻 close 主窗口——若此时尚无其他窗口，Tauri 会认为
                // "所有窗口已关闭" 而直接退出事件循环，导致异步创建 dsh 窗口来不及执行。
                // 因此先**隐藏**主窗口（保持进程存活），待 dsh 窗口建好后再关闭它。
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.hide();
                }
                // launch_window 是 async 的；setup 是同步闭包，用 async_runtime 起任务
                let app_handle = app.handle().clone();
                let procs_for_launch = procs_setup.clone();
                let version_for_launch = version.clone();
                tauri::async_runtime::spawn(async move {
                    match commands::launch_window(app_handle.clone(), procs_for_launch, version_for_launch.clone()).await {
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
                            logging::log_launch_error(&root, &format!("版本 {} 启动失败: {}", version_for_launch, e));
                            eprintln!("[dsh-multiver] 启动失败: {}", e);
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
                    let removed = maintenance::cleanup_orphan_webviews(&dirs.versions, &dirs.webview);
                    cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
                    cfg.maintenance.last_cleanup_count = removed.len() as u64;
                    let _ = cfg.save(&mdir);
                    if !removed.is_empty() {
                        logging::log_maintenance(&root, &format!("清理孤立 webview：{:?}", removed));
                    }

                    // 2) store prune：距上次 >= 7 天才跑，且延迟 30 秒错开启动 IO
                    let due = match cfg.maintenance.last_prune_at {
                        Some(t) => maintenance::now_secs().saturating_sub(t) >= maintenance::PRUNE_INTERVAL_SECS,
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
                                logging::log_maintenance(&root, &format!("store prune 失败（下次重试）: {}", e));
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
                for (_, (_, _, child, _job)) in map.iter_mut() {
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

