//! Tauri 命令层（从 lib.rs 抽出，GAP-003 选项 B；纯移动，无逻辑改动）。
//!
//! 这里集中所有 #[tauri::command] 及其专属 helper（状态、窗口启动、配置维护、脚本生成）。
//! 共享的进程映射类型与 manager_dir 仍在 crate 根（lib.rs）。

use crate::config::{Config, Dirs};
use crate::logging;
use crate::window;
use crate::{actions, cli, config, envcheck, launcher, maintenance, runtime, sizecache, versions};
use crate::{lock_procs, manager_dir, ProcMap, NEXT_GEN};
use serde::Serialize;
use std::sync::atomic::AtomicBool;

/// v0.2.13：扫描取消标志。scan_versions 开头清零，cancel_scan 置位。
pub(crate) static SCAN_CANCEL: AtomicBool = AtomicBool::new(false);

/// v0.2.14：供前端查询是否处于"启动模式"（命令行带 --launch-version）。
/// 前端 onMounted 主动查，比事件可靠（setup emit 时前端可能还没监听）。
#[tauri::command]
pub(crate) fn get_launch_mode() -> Option<String> {
    crate::LAUNCH_MODE.get().cloned()
}

/// v0.2.14：前端渲染出"启动态"后调用，显示主窗口。
/// （启动模式下 setup 里先 hide 主窗口，避免闪现主界面；前端就绪后由此命令 show。）
#[tauri::command]
pub(crate) fn show_main_window(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
    }
}

/// v0.2.14：查询某版本是否已有内嵌窗口在开。
/// 前端 run() 先查——已开则直接聚焦（不进启动态），避免"杀掉重开"的误导。
#[tauri::command]
pub(crate) fn is_version_open(procs: tauri::State<'_, ProcMap>, version: String) -> bool {
    lock_procs(procs.inner()).contains_key(&version)
}

/// 「路径设置」里额外展示的目录项（默认收起，点"展开"才显示）。
#[derive(Serialize)]
pub(crate) struct ExtraDir {
    /// 目录名（同时是 open_dir 的 which 值）
    name: String,
    /// 中文说明
    desc: String,
    /// 绝对路径
    path: String,
}

/// 一次返回给前端的完整状态
#[derive(Serialize)]
pub(crate) struct AppState {
    /// 管理器自身版本号（来自 Cargo.toml）
    version: String,
    root_dir: String,
    versions_dir: String,
    home_dir: String,
    store_dir: String,
    cache_dir: String,
    state_dir: String,
    /// 额外目录（logs/webview/trash/assets），由前端"展开全部"时展示
    extra_dirs: Vec<ExtraDir>,
    default_version: Option<String>,
    manager_dir: String,
    broken_versions: Vec<String>,
    maintenance: config::MaintenanceConfig,
    /// 是否复用官方 dsh 配置目录（~/.dsh）
    use_official_dsh_home: bool,
    /// 官方 dsh 配置目录路径（用于界面展示）；无法确定主目录时为 None
    official_home_dir: Option<String>,
    /// 用户是否已忽略「缺少运行时」启动提示（v0.2.9）
    runtime_prompt_dismissed: bool,
}

#[tauri::command]
pub(crate) fn get_state(app: tauri::AppHandle) -> AppState {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let _ = dirs.ensure();
    let extra_dirs = vec![
        ExtraDir {
            name: "logs".into(),
            desc: "管理器与 dsh 的运行日志".into(),
            path: dirs.resolve("logs").to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "webview".into(),
            desc: "各版本的 WebView2 数据（删了会丢登录态）".into(),
            path: dirs.webview.to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "trash".into(),
            desc: "卸载回收站（后台自动清空）".into(),
            path: dirs.trash.to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "assets".into(),
            desc: "内部资源（桌面快捷方式图标）".into(),
            path: dirs.assets.to_string_lossy().to_string(),
        },
    ];
    AppState {
        version: cli::VERSION.to_string(),
        root_dir: root.to_string_lossy().to_string(),
        versions_dir: dirs.versions.to_string_lossy().to_string(),
        home_dir: dirs.home.to_string_lossy().to_string(),
        store_dir: dirs.store.to_string_lossy().to_string(),
        cache_dir: dirs.cache.to_string_lossy().to_string(),
        state_dir: dirs.state.to_string_lossy().to_string(),
        extra_dirs,
        default_version: cfg.default_version.clone(),
        manager_dir: mdir.to_string_lossy().to_string(),
        broken_versions: cfg.broken_versions.clone(),
        maintenance: cfg.maintenance.clone(),
        use_official_dsh_home: cfg.use_official_dsh_home,
        official_home_dir: crate::paths::official_dsh_home()
            .map(|p| p.to_string_lossy().to_string()),
        runtime_prompt_dismissed: cfg.runtime_prompt_dismissed,
    }
}

#[tauri::command]
pub(crate) fn list_installed(app: tauri::AppHandle) -> Vec<versions::VersionInfo> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    versions::list(
        &dirs.versions,
        cfg.default_version.as_deref(),
        &cfg.isolated_versions,
    )
}

#[tauri::command]
pub(crate) fn list_remote(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    let (ok, list, err) = actions::list_remote(&dirs.store, &dirs.cache, &dirs.state);
    if ok {
        Ok(list)
    } else {
        Err(err)
    }
}

#[tauri::command]
pub(crate) async fn install_version(
    app: tauri::AppHandle,
    version: String,
    registry: Option<String>,
) -> Result<String, String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    // 启动参数 --debug-progress 时，把进度事件落盘到 <exe目录>/progress-debug.log
    // 供排查进度问题（默认关闭，不产生任何文件）。
    let debug_progress = std::env::args().any(|a| a == "--debug-progress");

    let app2 = app.clone();
    let version_for_record = version.clone();
    // 把阻塞的安装逻辑丢到后台线程池，避免占用主线程导致界面卡死
    let result = tauri::async_runtime::spawn_blocking(move || {
        let on_progress = move |ev: &versions::ProgressEvent| {
            if debug_progress {
                if let Ok(mdir) = std::env::current_exe() {
                    if let Some(dir) = mdir.parent() {
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(dir.join("progress-debug.log"))
                        {
                            let _ = writeln!(
                                f,
                                "step={} total={} stage={:?} detail={:?} frac={:.2}",
                                ev.step, ev.total, ev.stage, ev.detail, ev.fraction
                            );
                        }
                    }
                }
            }
            let _ = app2.emit("install-progress", ev.clone());
        };
        versions::install(
            &dirs.versions,
            &dirs.store,
            &dirs.cache,
            &dirs.state,
            &version,
            &on_progress,
            registry.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("安装任务失败: {}", e))?;

    let (ok, msg) = result;
    // 维护 broken_versions：失败且分类为「私有包/不完整」时记入；成功时移除。
    let mut c = cfg.clone();
    let kind = if ok {
        ""
    } else {
        versions::classify_error(&msg)
    };
    if c.apply_install_result(&version_for_record, ok, kind) {
        let _ = c.save(&mdir);
    }
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

/// 清除「已知安装失败」标记（用户手动重试前可调用）。
#[tauri::command]
pub(crate) fn clear_broken(app: tauri::AppHandle, version: Option<String>) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    match version {
        Some(v) => cfg.broken_versions.retain(|x| x != &v),
        None => cfg.broken_versions.clear(),
    }
    cfg.save(&mdir).map_err(|e| format!("保存配置失败: {}", e))
}

#[tauri::command]
pub(crate) async fn uninstall_version(
    app: tauri::AppHandle,
    version: String,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    // 如果卸载的是默认版本，清空默认设置，并移除隔离标记
    let mut cfg = cfg;
    if cfg.remove_version_refs(&version) {
        let _ = cfg.save(&mdir);
    }
    // 卸载可能清掉了默认版本 / 隔离标记：重生成转发脚本（无默认版本时会删除 dsh.cmd）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    // 快速卸载：把版本目录 rename 到回收站（瞬时），实际删除交给后台线程。
    // WebView2 数据目录（<根>/webview/<版本>）同样 rename 到回收站（也瞬时）。
    // 这样用户点「卸载」后版本立即消失，不再盯着"卸载中..."等几十秒。
    let webview_dir = dirs.webview.join(&version);
    let trash = dirs.trash.clone();
    let versions_dir = dirs.versions.clone();
    let (ok, msg) = tauri::async_runtime::spawn_blocking(move || {
        let result = versions::uninstall_fast(&versions_dir, &trash, &version);
        // 版本目录已 rename 走后，webview 目录也 rename 进回收站（失败不影响卸载结果）
        if result.0 && webview_dir.exists() {
            let _ = std::fs::create_dir_all(&trash);
            let dest = trash.join(format!("webview-{}-{}", version, std::process::id()));
            if std::fs::rename(&webview_dir, &dest).is_ok() {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_dir_all(&dest);
                });
            } else {
                let _ = std::fs::remove_dir_all(&webview_dir);
            }
        }
        result
    })
    .await
    .map_err(|e| format!("卸载任务失败: {}", e))?;
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

/// set_default 检测到"非管理器生成的同名 dsh 命令"时，返回此前缀的错误，
/// 前端据此弹出确认框，用户确认后再以 force=true 重调。
pub(crate) const CONFLICT_PREFIX: &str = "DSH_CONFLICT:";

#[tauri::command]
pub(crate) fn set_default(
    app: tauri::AppHandle,
    version: Option<String>,
    force: Option<bool>,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if let Some(v) = &version {
        if !versions::exists(&dirs.versions, v) {
            return Err(format!("版本 {} 未安装", v));
        }
    }

    // GAP-014：设为默认（会写转发脚本）前，检测目标位置是否已有"非本管理器"的 dsh 命令。
    // 有且用户未确认（force != Some(true)）→ 返回带前缀的提示，让前端确认。
    if version.is_some() && force != Some(true) {
        let target = crate::paths::forward_script_dir(&mdir);
        if let Some(p) = actions::existing_foreign_dsh(&target) {
            return Err(format!("{}{}", CONFLICT_PREFIX, p.to_string_lossy()));
        }
    }

    cfg.default_version = version.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;

    // 生成/删除转发脚本（版本号、根目录、DSH_HOME 直接写进脚本）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    match &version {
        Some(v) => Ok(format!("默认版本已设为 {}，dsh 命令已就绪", v)),
        None => Ok("已清除默认版本".to_string()),
    }
}

/// dsh 插件管理（双向桥 B1）：列某版本下的所有 profile 与插件。
///
/// DSH_HOME 用 resolve_home 解析（隔离 / 官方 / 共享）。
#[tauri::command]
pub(crate) fn ds_list_plugins(
    app: tauri::AppHandle,
    version: String,
) -> Result<crate::dsplugins::DsPluginList, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let home_kind = if cfg.isolated_versions.iter().any(|v| v == &version) {
        "isolated"
    } else if cfg.use_official_dsh_home && crate::paths::official_dsh_home().is_some() {
        "official"
    } else {
        "shared"
    };
    Ok(crate::dsplugins::DsPluginList {
        dsh_home: home.to_string_lossy().to_string(),
        home_kind: home_kind.to_string(),
        profiles: crate::dsplugins::list_profiles(&home),
    })
}

/// dsh 插件安装/卸载（双向桥 B1）。
///
/// action: "add" | "remove"；package: npm 包名。
/// 通过 dsh CLI（`dsh plugin --profile <p> add/remove <包>`）执行。
#[tauri::command]
pub(crate) fn ds_plugin_action(
    app: tauri::AppHandle,
    version: String,
    profile: String,
    action: String,
    package: String,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(&version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let bin = crate::dsplugins::dsh_bin(&vdir)
        .ok_or_else(|| format!("版本 {} 缺少 dsh 入口", version))?;
    let args: Vec<&str> = match action.as_str() {
        "add" => vec!["add", &package],
        "remove" => vec!["remove", &package],
        _ => return Err(format!("未知操作：{}", action)),
    };
    let (ok, out) = crate::dsplugins::run_dsh_plugin(&bin, &home, &profile, &args);
    if ok {
        Ok(out)
    } else {
        Err(out)
    }
}

/// 打开某版本的 profile 目录（资源管理器）。
#[tauri::command]
pub(crate) fn ds_open_profile_dir(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let dir = crate::dsplugins::profiles_dir(&home).join(&profile);
    if !dir.exists() {
        return Err(format!("目录不存在：{}", dir.to_string_lossy()));
    }
    let (ok, msg) = crate::actions::open_folder(&dir);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

/// 启动 dsh web（阻塞部分放到 spawn_blocking）并创建内嵌窗口。
///
/// 注意：WebviewWindowBuilder::build() 必须在主线程执行，
/// 因此这里先 await 一个 spawn_blocking 完成"起进程 + 等 URL"，再在主线程建窗。
pub(crate) async fn launch_window(
    app: tauri::AppHandle,
    procs: ProcMap,
    version: String,
) -> Result<String, String> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(&version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);

    // v0.2.14：该版本窗口已开 → 直接聚焦，不重启（幂等，避免误点导致"杀掉重开"）。
    // 在起 dsh 进程之前判断，避免白起一个进程。
    {
        let existing = lock_procs(&procs)
            .get(&version)
            .map(|(label, _, _, _)| label.clone());
        if let Some(lbl) = existing {
            if let Some(win) = app.get_webview_window(&lbl) {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
                return Ok(format!("已聚焦 DSH {} 的窗口", version));
            }
        }
    }

    // v0.2.14：启动进度事件（前端"启动态"据此更新文案）。
    // dsh 启动期间静默、不打印输出，无法细分；两个粗阶段：
    //   preparing —— 即将起进程（本函数开始的短暂窗口）
    //   waiting   —— 进程已起、正在等 dsh 打印 URL（最慢，最坏 90s）
    // 这里直接发 waiting：spawn_blocking 内部即"起进程 + 等 URL"，属"等待"。
    // （preparing 由前端进入启动态时自行设定，无需后端再发。）
    {
        use tauri::Emitter;
        let _ = app.emit(
            "launch-progress",
            serde_json::json!({ "version": version, "stage": "waiting" }),
        );
    }

    // 阻塞部分（起进程 + 等端口/URL，最多 90s，见 launcher 的超时常量）丢到后台线程，避免冻结界面
    let vdir2 = vdir.clone();
    let home2 = home.clone();
    let store2 = dirs.store.clone();
    let cache2 = dirs.cache.clone();
    let state2 = dirs.state.clone();
    let spawned = tauri::async_runtime::spawn_blocking(move || {
        launcher::spawn_web_hidden(&vdir2, &home2, &store2, &cache2, &state2)
    })
    .await
    .map_err(|e| {
        let m = format!("启动任务失败: {}", e);
        logging::log_app_error(&dirs.root, "launch_window", &format!("{} ({})", version, m));
        m
    })?;
    let (child, url, job) = spawned.map_err(|e| {
        logging::log_app_error(&dirs.root, "launch_window", &format!("{} ({})", version, e));
        e
    })?;

    // Unix：登记该 dsh 的进程组，供"管理器被强杀后下次启动清理"兜底（GAP-010）。
    // Windows 无此需要（Job Object 是 OS 级保障）。
    #[cfg(unix)]
    crate::procreg::register(&dirs.root, child.id() as i32, &version);

    // 回到主线程建窗。
    // 先结束该版本的旧窗口/旧进程（若有）。
    let old = lock_procs(&procs).remove(&version);
    if let Some((old_label, _old_gen, mut old_child, _old_job)) = old {
        // 结束旧进程及其子进程树（kill 不 wait，避免阻塞）
        #[cfg(unix)]
        crate::procreg::unregister(&dirs.root, old_child.id() as i32);
        launcher::kill_tree(&mut old_child);
        // 关闭旧窗口（destroy 异步投递；因下面用新唯一 label，不受其影响）
        if let Some(existing) = app.get_webview_window(&old_label) {
            let _ = existing.destroy();
        }
    }

    // 生成唯一 label（带代次），从根本上避免 label 复用冲突
    let generation = NEXT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let label = window::window_label_gen(&version, generation);

    let title = format!("DSH {}", version);
    let init_script = build_topbar_script(&version, &url);
    let parsed = url.parse().map_err(|e| format!("URL 解析失败: {}", e))?;

    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
        .title(&title)
        .inner_size(1100.0, 760.0)
        // 用常见 Edge UA，避免 dsh 把内嵌 WebView2 当"非浏览器"而对 bundle 返回 404
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0")
        // 每个版本用独立的 WebView2 数据目录，避免所有窗口共享同一目录
        // 长期累积 cookie/缓存 → 请求头膨胀 → dsh 返回 431 → "Failed to load plugins"。
        // 放在数据根目录内（<根>/webview/<版本>），符合自包含原则。
        .data_directory(dirs.webview.join(&version))
        .initialization_script(&init_script)
        // 先隐藏：任务栏项在窗口**显示时**按 AUMID 建图标，
        // 必须等设好窗口级 AUMID / 图标（下面）后再 show()。
        .visible(false)
        .build();
    let window = match built {
        Ok(w) => w,
        Err(e) => {
            let mut c = child;
            launcher::kill_tree(&mut c);
            logging::log_app_error(
                &dirs.root,
                "launch_window",
                &format!("{} 创建窗口失败: {}", version, e),
            );
            return Err(format!("创建窗口失败: {}", e));
        }
    };

    // 给 dsh 窗口设 dsh 图标（作用于标题栏 / Alt+Tab；任务栏图标由 AUMID 的 IconUri 决定）
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(ico) = launcher::materialize_dsh_icon(&exe) {
            if let Ok(img) = tauri::image::Image::from_path(&ico) {
                let _ = window.set_icon(img);
            }
        }
    }

    // 给 dsh 窗口单独设 AUMID（任务栏显示 dsh 图标、与管理器窗口分开成组）。
    // 必须在窗口**显示之前**设置（上面已用 .visible(false) 延迟显示）。
    // 失败不阻塞（退回进程级 AUMID，即管理器图标）。
    #[cfg(windows)]
    {
        if let Err(e) = window::set_window_app_user_model_id(&window, window::DSH_WINDOW_AUMID) {
            logging::log_app_error(
                &dirs.root,
                "launch_window",
                &format!("{} 设窗口 AUMID 失败: {}", version, e),
            );
        }
    }

    // 设好 AUMID / 图标后再显示
    if let Err(e) = window.show() {
        logging::log_app_error(
            &dirs.root,
            "launch_window",
            &format!("{} 显示窗口失败: {}", version, e),
        );
    }

    lock_procs(&procs).insert(version.clone(), (label.clone(), generation, child, job));

    // 关窗回调里要注销进程登记，故提前把 root 算好（闭包不能捕获 app —— 它随后还要用）
    #[cfg(unix)]
    let root_for_unregister = dirs.root.clone();

    let procs2 = procs.clone();
    let version2 = version.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let mut map = lock_procs(&procs2);
            // 仅当该版本仍是"本代次"时才 kill，避免旧窗口滞后回调误杀新进程
            let should_kill = matches!(map.get(&version2), Some((_, g, _, _)) if *g == generation);
            if should_kill {
                if let Some((_, _, mut child, _job)) = map.remove(&version2) {
                    // Unix：正常关窗能跑到这里，注销登记（避免下次启动误清）
                    #[cfg(unix)]
                    crate::procreg::unregister(&root_for_unregister, child.id() as i32);
                    // 结束整棵进程树；不 wait，避免阻塞事件线程导致关窗卡顿
                    launcher::kill_tree(&mut child);
                }
            }
        }
    });

    // ── 建窗后延迟 3 秒的「双条件检查」──
    // 目的：捕捉"dsh 输出了 URL、窗口也建了，但进程随即崩溃"的情况（例：老版本 dsh 的
    // profile / HMR 错误）。此时 launch_window 已返回成功，用户会看到 WebView 的"拒绝连接"。
    //
    // 只有**同时满足**「窗口仍开着」+「该版本进程已退出（且仍是本代次）」才提示——
    // 用户关窗会销毁窗口、重启会换成新代次，都不会命中，因此误报极低。
    {
        let app2 = app.clone();
        let procs3 = procs.clone();
        let version3 = version.clone();
        let label3 = label.clone();
        let gen3 = generation;
        let logs_dir = dirs.root.join("logs");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            use tauri::Manager;
            // ① 窗口还开着吗？
            if app2.get_webview_window(&label3).is_none() {
                return; // 用户已关窗，正常
            }
            // ② 该版本进程还活着吗？（且仍是本代次）
            let dead = {
                let mut map = lock_procs(&procs3);
                match map.get_mut(&version3) {
                    Some((_, g, child, _)) if *g == gen3 => {
                        // try_wait 返回 Ok(Some(_)) 表示已退出
                        matches!(child.try_wait(), Ok(Some(_)))
                    }
                    // 条目已不在（重启/关窗已移除）或代次不符 → 不提示
                    _ => false,
                }
            };
            if !dead {
                return;
            }
            let detail = launcher::latest_session_errors(&logs_dir, &version3, 6);
            let msg = if detail.is_empty() {
                format!(
                    "版本 {} 的 dsh 启动后立即退出。请查看日志目录：\n{}",
                    version3,
                    logs_dir.to_string_lossy()
                )
            } else {
                format!(
                    "版本 {} 的 dsh 启动后立即退出。dsh 输出：\n{}",
                    version3, detail
                )
            };
            logging::log_app_error(
                &logs_dir,
                "launch_window",
                &format!("{} 启动后立即退出", version3),
            );
            use tauri::Emitter;
            let _ = app2.emit(
                "launch-crashed",
                serde_json::json!({
                    "version": version3,
                    "detail": detail,
                    "message": msg,
                }),
            );
        });
    }

    Ok(format!("已在窗口打开 DSH {}", version))
}

#[tauri::command]
pub(crate) async fn run_version(
    app: tauri::AppHandle,
    procs: tauri::State<'_, ProcMap>,
    version: String,
) -> Result<String, String> {
    launch_window(app, procs.inner().clone(), version).await
}

#[tauri::command]
pub(crate) fn create_shortcut(app: tauri::AppHandle, version: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if !versions::exists(&dirs.versions, &version) {
        return Err(format!("版本 {} 未安装", version));
    }
    let exe = std::env::current_exe().map_err(|e| format!("无法定位程序路径: {}", e))?;
    let lnk = launcher::create_desktop_shortcut(&exe, &version)?;
    Ok(format!("已创建桌面快捷方式：{}", lnk))
}

/// 用新控制台窗口启动某版本的 dsh web，并让 dsh 自动打开系统默认浏览器。
/// home 与「运行」一致：非隔离用共享 home，隔离用独立 home。
///
/// 交互：
/// - 若该版本已有内嵌窗口（ProcMap 中存在 dsh-<版本>），先弹原生对话框确认「再开一个？」
/// - 端口固定 3080；若被占用，弹原生对话框让用户选择「换随机端口」或「取消」
#[tauri::command]
pub(crate) async fn open_in_browser(
    app: tauri::AppHandle,
    procs: tauri::State<'_, ProcMap>,
    version: String,
) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(&version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);

    // 1) 互斥提示：该版本已有内嵌窗口在开
    let has_window = lock_procs(procs.inner()).contains_key(&version);
    if has_window {
        let ok = app
            .dialog()
            .message(format!(
                "版本 {} 已在窗口打开，确定还要在浏览器再开一个吗？",
                version
            ))
            .title("已在窗口打开")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "再开一个".to_string(),
                "取消".to_string(),
            ))
            .blocking_show();
        if !ok {
            return Ok("已取消".to_string());
        }
    }

    // 2) 端口：固定 3080；占用则让用户选择
    let default_port: u16 = 3080;
    let mut port = default_port;
    if launcher::port_in_use(default_port) {
        let use_random = app
            .dialog()
            .message(format!(
                "端口 {} 已被占用。\n\n点「用随机端口」将换一个空闲端口打开；点「取消」放弃本次操作。",
                default_port
            ))
            .title("端口被占用")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "用随机端口".to_string(),
                "取消".to_string(),
            ))
            .blocking_show();
        if !use_random {
            return Ok("已取消".to_string());
        }
        port = 0;
    }

    // 3) 启动（新控制台，不传 --no-open，让 dsh 打开系统浏览器）
    let (ok, msg) =
        launcher::spawn_web_console(&vdir, &home, &dirs.store, &dirs.cache, &dirs.state, port);
    if ok {
        // 提示：端口 + 数据目录
        Ok(format!("{}；数据目录：{}", msg, home.to_string_lossy()))
    } else {
        logging::log_app_error(
            &dirs.root,
            "open_in_browser",
            &format!("{} ({})", version, msg),
        );
        Err(msg)
    }
}

/// 重启某版本窗口对应的 dsh web 进程：
/// kill 旧进程 → 用同版本起新进程（端口重分配）→ 窗口导航到新 URL。
#[tauri::command]
pub(crate) async fn restart_version(
    app: tauri::AppHandle,
    procs: tauri::State<'_, ProcMap>,
    version: String,
) -> Result<(), String> {
    use tauri::Manager;

    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(&version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);

    // 找到该版本当前的窗口（label 带代次，从 map 里取）
    let (label, _old_gen, mut old_child, _old_job) = lock_procs(procs.inner())
        .remove(&version)
        .ok_or_else(|| format!("版本 {} 的窗口不存在", version))?;
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("窗口 {} 不存在", label))?;

    // 先结束旧进程及其进程树（不 wait）
    launcher::kill_tree(&mut old_child);

    // 起新进程（阻塞部分丢后台）
    let vdir2 = vdir.clone();
    let home2 = home.clone();
    let store2 = dirs.store.clone();
    let cache2 = dirs.cache.clone();
    let state2 = dirs.state.clone();
    let spawned = tauri::async_runtime::spawn_blocking(move || {
        launcher::spawn_web_hidden(&vdir2, &home2, &store2, &cache2, &state2)
    })
    .await
    .map_err(|e| {
        let m = format!("重启任务失败: {}", e);
        logging::log_app_error(
            &dirs.root,
            "restart_version",
            &format!("{} ({})", version, m),
        );
        m
    })?;
    let (child, url, job) = spawned.map_err(|e| {
        logging::log_app_error(
            &dirs.root,
            "restart_version",
            &format!("{} ({})", version, e),
        );
        e
    })?;

    // 记录新进程（同一 label，代次更新）
    let generation = NEXT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    lock_procs(procs.inner()).insert(version.clone(), (label.clone(), generation, child, job));

    window
        .navigate(url.parse().map_err(|e| format!("URL 解析失败: {}", e))?)
        .map_err(|e| format!("窗口导航失败: {}", e))?;

    Ok(())
}

/// 注入到 dsh 页面的顶部可折叠小栏（显示版本号 + 完整 URL + 重启按钮）。
///
/// 脚本正文外置在 `src/topbar.js`（便于编辑/高亮/审查），编译期用 include_str! 内联；
/// 运行时把占位符 `__VER__` / `__URL__` 替换为实际值。
fn build_topbar_script(version: &str, url: &str) -> String {
    const TEMPLATE: &str = include_str!("topbar.js");
    // 防注入：转义反斜杠与单引号（模板中以单引号包裹字符串字面量）
    let ver = version.replace('\\', "\\\\").replace('\'', "\\'");
    let url_js = url.replace('\\', "\\\\").replace('\'', "\\'");
    TEMPLATE
        .replace("__VER__", &ver)
        .replace("__URL__", &url_js)
}

#[tauri::command]
pub(crate) fn set_isolated(
    app: tauri::AppHandle,
    version: String,
    isolated: bool,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if !versions::exists(&dirs.versions, &version) {
        return Err(format!("版本 {} 未安装", version));
    }
    cfg.isolated_versions.retain(|v| v != &version);
    if isolated {
        cfg.isolated_versions.push(version.clone());
        // 预创建隔离目录
        let _ = std::fs::create_dir_all(dirs.versions.join(&version).join("home"));
    }
    cfg.save(&mdir).map_err(|e| e.to_string())?;
    // 隔离状态变了：若改的是当前默认版本，重生成转发脚本（DSH_HOME 会变）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    if isolated {
        Ok(format!("{} 已开启数据隔离", version))
    } else {
        Ok(format!("{} 已关闭数据隔离", version))
    }
}

/// 导入预览：读备份文件，返回将改动的字段（不应用）。
#[tauri::command]
pub(crate) fn preview_import(
    app: tauri::AppHandle,
    file: String,
) -> Result<crate::backup::ImportPreview, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    crate::backup::preview_import(std::path::Path::new(&file), &cfg, &dirs.versions)
}

/// 应用导入：把备份文件的配置字段写入当前配置。
#[tauri::command]
pub(crate) fn apply_import(app: tauri::AppHandle, file: String) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let applied = crate::backup::apply_import(std::path::Path::new(&file), &mut cfg)?;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败：{}", e))?;
    // 配置变了（默认版本/根目录/隔离可能变）→ 重生成转发脚本
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    Ok(applied)
}

/// 导出配置 + 版本清单到指定文件。返回写入路径。
#[tauri::command]
pub(crate) fn export_backup(app: tauri::AppHandle, file: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let out = std::path::PathBuf::from(file);
    crate::backup::export_to_file(&cfg, &dirs.versions, &out)
}

/// 检查是否有新版本（更新提醒，C1）。网络请求放后台线程，绝不阻塞 UI。
#[tauri::command]
pub(crate) async fn check_update() -> crate::updater::UpdateInfo {
    tauri::async_runtime::spawn_blocking(crate::updater::check)
        .await
        .unwrap_or_else(|_| crate::updater::UpdateInfo {
            has_update: false,
            current: cli::VERSION.to_string(),
            latest: None,
            release_url: None,
            download_url: None,
            sha256: None,
        })
}

/// 请求管理器退出（供一键更新用：替换批处理需主进程退出后才能覆盖 exe）。
#[tauri::command]
pub(crate) fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 执行一键更新（C2，v0.2.10）：下载新 exe → 校验 → 生成替换批处理 → 返回；
/// 调用方收到成功后应**退出管理器**，由批处理完成替换并重启。
///
/// 返回成功消息（含新版本号）。失败时返回 Err（不触碰原 exe）。
#[tauri::command]
pub(crate) async fn perform_update(
    app: tauri::AppHandle,
    url: String,
    sha256: Option<String>,
) -> Result<String, String> {
    let target = std::env::current_exe().map_err(|e| format!("无法定位当前程序：{}", e))?;
    // 下载 + 校验（放后台线程，避免阻塞）
    let new_exe = tauri::async_runtime::spawn_blocking(move || {
        crate::updater::download_and_verify(&url, sha256.as_deref())
    })
    .await
    .map_err(|e| format!("下载任务失败：{}", e))??;

    // 生成替换批处理并启动
    crate::updater::spawn_replacer(&new_exe, &target)?;
    // 记录到维护日志
    let mdir = manager_dir(&app);
    let root = Config::load(&mdir).resolve_root(&mdir);
    logging::write_line(
        &root.join("logs"),
        "maintenance.log",
        "自动更新：已下载并启动替换程序",
    );
    Ok("更新已就绪，管理器将退出并自动重启为新版本".to_string())
}

/// 设置「复用官方 dsh 配置目录（~/.dsh）」开关。
///
/// 开启后，非隔离版本的 DSH_HOME 指向官方默认目录，用户已有的会话/凭据/插件立即可用；
/// 隔离版本不受影响。切换后重生成转发脚本，保证 GUI 与终端行为一致。
#[tauri::command]
pub(crate) fn set_use_official_home(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.use_official_dsh_home = enabled;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败: {}", e))?;
    // 非隔离版本的 home 来源变了：重生成转发脚本
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    if enabled {
        let path = crate::paths::official_dsh_home()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/.dsh".to_string());
        Ok(format!("已复用官方配置目录：{}", path))
    } else {
        Ok("已恢复为管理器共享配置目录".to_string())
    }
}

/// 查询官方 dsh 配置目录路径（供前端展示/判断是否存在）。
#[tauri::command]
pub(crate) fn get_official_home() -> Option<String> {
    crate::paths::official_dsh_home().map(|p| p.to_string_lossy().to_string())
}

/// v0.2.13：统一的占用扫描命令。
/// versions 传 1 个 = 单版本；传 N 个 = 全部。前端只有这一条路径。
///
/// 行为：
///   - 串行遍历 versions；每个版本先查版本级缓存（dir_mtime + store_mtime），命中直接 emit 完成。
///   - 未命中：两段式 —— 先 emit 总量（stage="total"，带 fileTotal），再逐文件补精确（stage="detail"），
///     扫完 emit 完整 info（stage="detail"，info 非空）。
///   - 支持取消：cancel_scan 置位全局标志 → 当前版本中断、丢弃、停止循环。
///   - 只扫完的版本写缓存；被取消的版本不写。
#[tauri::command]
pub(crate) async fn scan_versions(
    app: tauri::AppHandle,
    versions: Vec<String>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let versions_dir = dirs.versions.clone();
    let store = dirs.store.clone();
    let app2 = app.clone();

    // 清除上次的取消标志
    SCAN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let names: Vec<String> = versions;
        let total = names.len();
        let store_mtime = sizecache::dir_mtime(&store);
        let mut cache = sizecache::load(&root);

        for (i, n) in names.iter().enumerate() {
            // 取消检查（每个版本开始前）
            if SCAN_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let idx = i + 1;
            let dir_mtime = sizecache::dir_mtime(&versions_dir.join(n));

            // 缓存命中：逐个 emit（间隔 80ms），让"复用"有节奏感
            if sizecache::is_valid(&cache, n, dir_mtime, store_mtime) {
                if let Some(info) = sizecache::get_info(&cache, n) {
                    let _ = app2.emit(
                        "scan-size-progress",
                        serde_json::json!({
                            "version": n,
                            "stage": "detail",
                            "index": idx,
                            "total": total,
                            "done": 0,
                            "fileTotal": 0,
                            "info": info,
                            "cached": true,
                        }),
                    );
                    if total > 1 {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                }
                continue;
            }

            // 第一段开始：发 counting 事件（前端显示不确定进度条）
            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "counting",
                    "index": idx,
                    "total": total,
                    "done": 0,
                    "fileTotal": 0,
                    "info": serde_json::Value::Null,
                    "cached": false,
                }),
            );

            // 取消检查闭包
            let cancel = || SCAN_CANCEL.load(std::sync::atomic::Ordering::SeqCst);

            // 第一段（合并遍历）：收集文件列表 + 总量。带进度与取消。
            let app_c = app2.clone();
            let vname_c = n.clone();
            let cb_count = move |done: u64, _tot: u64| {
                let _ = app_c.emit(
                    "scan-size-progress",
                    serde_json::json!({
                        "version": vname_c,
                        "stage": "counting",
                        "index": idx,
                        "total": total,
                        "done": done,
                        "fileTotal": 0,
                        "info": serde_json::Value::Null,
                        "cached": false,
                    }),
                );
            };
            let Some(fl) =
                versions::collect_files(&versions_dir.join(n), Some(&cb_count), Some(&cancel))
            else {
                break; // 被取消
            };
            let file_count = fl.files.len() as u64;

            // 第一段完成：发总量（stage="total"）
            let first = versions::SizeInfo {
                total: fl.total,
                shared_size: 0,
                shared_count: 0,
                exclusive_size: fl.total,
            };
            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "total",
                    "index": idx,
                    "total": total,
                    "done": 0,
                    "fileTotal": file_count,
                    "info": first,
                    "cached": false,
                }),
            );

            // 第二段：用文件列表读 nlink（带进度与取消）
            let app3 = app2.clone();
            let vname = n.clone();
            let cb = move |done: u64, ftot: u64| {
                let _ = app3.emit(
                    "scan-size-progress",
                    serde_json::json!({
                        "version": vname,
                        "stage": "detail",
                        "index": idx,
                        "total": total,
                        "done": done,
                        "fileTotal": if ftot > 0 { ftot } else { file_count },
                        "info": serde_json::Value::Null,
                        "cached": false,
                    }),
                );
            };
            let Some(info) =
                versions::detail_from_files(&fl.files, fl.total, Some(&cb), Some(&cancel))
            else {
                break; // 被取消
            };

            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "detail",
                    "index": idx,
                    "total": total,
                    "done": file_count,
                    "fileTotal": file_count,
                    "info": info.clone(),
                    "cached": false,
                }),
            );
            sizecache::put(&mut cache, n, dir_mtime, store_mtime, info);
        }
        let _ = sizecache::save(&root, &cache);
    })
    .await
    .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(())
}

/// v0.2.13：请求取消正在进行的扫描。置位全局标志，扫描循环会在当前版本中断后停止。
#[tauri::command]
pub(crate) fn cancel_scan() -> Result<(), String> {
    SCAN_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub(crate) async fn scan_isolated_size(
    app: tauri::AppHandle,
    version: String,
) -> Result<u64, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = versions::isolated_home(&dirs.versions, &version);
    // 目录扫描可能很慢，放到后台线程
    let size = tauri::async_runtime::spawn_blocking(move || versions::dir_size(&home))
        .await
        .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(size)
}

/// 设置自动维护开关。
#[tauri::command]
pub(crate) fn set_auto_maintenance(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.maintenance.auto_enabled = enabled;
    cfg.save(&mdir).map_err(|e| format!("保存配置失败: {}", e))
}

/// 手动执行维护。kind: "cleanup"（孤立 webview）| "prune"（依赖仓库）| "all"。
/// 返回人类可读的结果描述。
#[tauri::command]
pub(crate) async fn run_maintenance(app: tauri::AppHandle, kind: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let _ = dirs.ensure();
    let mdir2 = mdir.clone();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();

        if kind == "cleanup" || kind == "all" {
            let removed = maintenance::cleanup_orphan_webviews(&dirs.versions, &dirs.webview);
            cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
            cfg.maintenance.last_cleanup_count = removed.len() as u64;
            logging::log_maintenance(
                &root,
                &format!(
                    "[手动] 清理孤立 webview：{} 项 {:?}",
                    removed.len(),
                    removed
                ),
            );
            // 同时清空「回收站」残留（卸载时 rename 进来的，后台可能未删净）
            let trashed = maintenance::cleanup_trash(&dirs.trash);
            let mut s = format!("已清理孤立缓存 {} 项", removed.len());
            if trashed > 0 {
                s.push_str(&format!("；已清空回收站 {} 项", trashed));
            }
            parts.push(s);
        }

        if kind == "prune" || kind == "all" {
            match maintenance::run_store_prune(&dirs.store, &dirs.cache, &dirs.state) {
                Ok(()) => {
                    cfg.maintenance.last_prune_at = Some(maintenance::now_secs());
                    logging::log_maintenance(&root, "[手动] store prune 完成");
                    parts.push("已回收依赖仓库".to_string());
                }
                Err(e) => {
                    logging::log_maintenance(&root, &format!("[手动] store prune 失败: {}", e));
                    return Err(format!("回收依赖仓库失败：{}", e));
                }
            }
        }

        let _ = cfg.save(&mdir2);
        Ok(parts.join("；"))
    })
    .await
    .map_err(|e| format!("维护任务失败: {}", e))?;

    result
}

#[tauri::command]
pub(crate) async fn copy_shared_to_isolated(
    app: tauri::AppHandle,
    version: String,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if !cfg.isolated_versions.iter().any(|v| v == &version) {
        return Err(format!("{} 未开启隔离", version));
    }
    let shared = dirs.home.clone();
    let isolated = versions::isolated_home(&dirs.versions, &version);
    let (ok, msg, _) = tauri::async_runtime::spawn_blocking(move || {
        versions::copy_shared_to_isolated(&shared, &isolated)
    })
    .await
    .map_err(|e| format!("复制任务失败: {}", e))?;
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) async fn clear_isolated_data(
    app: tauri::AppHandle,
    version: String,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let (ok, msg) = tauri::async_runtime::spawn_blocking(move || {
        versions::clear_isolated(&dirs.versions, &version)
    })
    .await
    .map_err(|e| format!("清理任务失败: {}", e))?;
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) fn open_isolated_dir(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = versions::isolated_home(&dirs.versions, &version);
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) fn set_root(app: tauri::AppHandle, root: Option<String>) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);

    // 切换前：对**旧**数据根做一次残留进程清理（GAP-011）。
    // 否则旧 root 下遗留的登记不会被新 root 的启动清理触及，其中的 dsh 可能永久残留。
    #[cfg(unix)]
    {
        let old_root = cfg.resolve_root(&mdir);
        if old_root != mdir {
            // 仅在旧 root 与管理器目录不同、且确实存在时才尝试（避免误清管理器自身目录）
            let n = crate::procreg::cleanup_stale(&old_root);
            if n > 0 {
                logging::write_line(
                    &old_root.join("logs"),
                    "maintenance.log",
                    &format!("切换数据根前清理旧 root 残留 dsh：{} 个", n),
                );
            }
        }
    }

    cfg.root_dir = root.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    dirs.ensure().map_err(|e| e.to_string())?;
    // 根目录变了，重生成转发脚本（含新的 DSH_HOME）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    Ok(format!("根目录已设为 {}", dirs.root.to_string_lossy()))
}

/// 用系统默认浏览器打开一个 URL（环境配置引导用）。
#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只允许打开 http/https 链接".to_string());
    }
    let (ok, msg) = actions::open_url(&url);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) fn open_dir(app: tauri::AppHandle, which: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let target = dirs.resolve(&which);
    let (ok, msg) = actions::open_folder(&target);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) fn check_env(app: tauri::AppHandle, full: Option<bool>) -> Vec<envcheck::CheckItem> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    // v0.2.13：full=false（启动首屏）跳过 npm 源连通（网络）检查，避免拖慢启动；
    // 用户点「重新检查」/安装前传 full=true 做完整检查。
    envcheck::run_all(&dirs.root, full.unwrap_or(true))
}

/// 运行时状态：系统/便携的 Node、pnpm 是否就绪（供前端判断是否显示"自动准备"按钮）。
#[tauri::command]
pub(crate) fn runtime_status(app: tauri::AppHandle) -> runtime::RuntimeStatus {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    runtime::RuntimeStatus {
        system_node_ok: runtime::system_node_ok(),
        bundled_node_ok: runtime::bundled_node_ok(&root),
        system_pnpm_ok: runtime::system_pnpm_ok(),
        bundled_pnpm_ok: runtime::bundled_pnpm_ok(&root),
    }
}

/// 准备便携运行时（下载缺失的 Node/pnpm）。
///
/// 阻塞式下载放在后台线程池，进度经 "runtime-progress" 事件推给前端（GUI 进度条）。
/// 返回结果消息（成功/已就绪）。
#[tauri::command]
pub(crate) async fn setup_runtime(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let app2 = app.clone();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>, String> {
        let mut msgs: Vec<String> = Vec::new();
        let on_prog = move |ev: runtime::RuntimeProgress| {
            let _ = app2.emit("runtime-progress", ev);
        };
        if !runtime::system_node_ok() && !runtime::bundled_node_ok(&root) {
            runtime::install_bundled_node(&root, &on_prog)
                .map_err(|e| format!("下载便携 Node 失败：{}", e))?;
            msgs.push("已安装便携 Node".to_string());
        }
        if !runtime::system_pnpm_ok() && !runtime::bundled_pnpm_ok(&root) {
            runtime::install_bundled_pnpm(&root, &on_prog)
                .map_err(|e| format!("下载便携 pnpm 失败：{}", e))?;
            msgs.push("已安装便携 pnpm".to_string());
        }
        Ok(msgs)
    })
    .await
    .map_err(|e| format!("运行时准备任务失败：{}", e))??;

    if result.is_empty() {
        Ok("运行时已就绪，无需下载".to_string())
    } else {
        Ok(result.join("；"))
    }
}

/// 用户忽略「缺少运行时」的启动提示（v0.2.9）：记住选择，不再自动弹横幅。
/// 用户仍可从环境检查面板手动准备运行时。
#[tauri::command]
pub(crate) fn dismiss_runtime_prompt(app: tauri::AppHandle) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.runtime_prompt_dismissed = true;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败: {}", e))?;
    Ok("已忽略运行时提示".to_string())
}

/// 前端（WebView）JS 错误上报：写入 <根>/logs/frontend.log。
/// 历史踩坑：App.vue 缺 import 导致白屏、window.confirm 在 WebView2 静默失效——
/// 这些前端异常过去无任何留痕。此命令为其提供落盘通道。
#[tauri::command]
pub(crate) fn log_frontend(app: tauri::AppHandle, msg: String) {
    let mdir = manager_dir(&app);
    let root = Config::load(&mdir).resolve_root(&mdir);
    // v0.2.13 启动打点：若前端首屏打点（[boot]），附加"进程启动→窗口就绪"耗时
    let msg = if msg.starts_with("[boot]") {
        if let Some(ms) = crate::WIN_READY_MS.get() {
            format!("{}(窗口就绪={}ms) ", msg, ms)
        } else {
            msg
        }
    } else {
        msg
    };
    logging::write_line(&root.join("logs"), "frontend.log", &msg);
}

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
