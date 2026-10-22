//! 启动与窗口命令（D6 拆分）：launch_window / run_version / create_shortcut / open_in_browser / restart_version / build_topbar_script。
#![allow(unused_imports)]
use super::*;

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
#[derive(serde::Serialize)]
pub(crate) struct OpenInBrowserResult {
    pub url: Option<String>,
    pub opened: bool,
    pub message: String,
}

/// - 若该版本已有内嵌窗口（ProcMap 中存在 dsh-<版本>），先弹原生对话框确认「再开一个？」
/// - 端口固定 3080；若被占用，弹原生对话框让用户选择「换随机端口」或「取消」
#[tauri::command]
pub(crate) async fn open_in_browser(
    app: tauri::AppHandle,
    procs: tauri::State<'_, ProcMap>,
    version: String,
) -> Result<OpenInBrowserResult, String> {
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
            return Ok(OpenInBrowserResult {
                url: None,
                opened: false,
                message: "已取消".to_string(),
            });
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
            return Ok(OpenInBrowserResult {
                url: None,
                opened: false,
                message: "已取消".to_string(),
            });
        }
        port = 0;
    }

    // 3) 启动（新控制台窗口，不传 --no-open，由 dsh 自己打开系统浏览器）
    // 设计意图：用户拿到一个显式终端窗口，行为与「自己开终端敲 dsh web」一致。
    let (ok, msg) =
        launcher::spawn_web_console(&vdir, &home, &dirs.store, &dirs.cache, &dirs.state, port);
    if !ok {
        logging::log_app_error(
            &dirs.root,
            "open_in_browser",
            &format!("{} ({})", version, msg),
        );
        return Err(msg);
    }
    // 控制台窗口模式：URL/token 由 dsh 自己打印在终端里，管理器不再捕获。
    Ok(OpenInBrowserResult {
        url: None,
        opened: true,
        message: format!("{}；数据目录：{}", msg, home.to_string_lossy()),
    })
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
pub(crate) fn build_topbar_script(version: &str, url: &str) -> String {
    const TEMPLATE: &str = include_str!("../topbar.js");
    // 防注入：转义反斜杠与单引号（模板中以单引号包裹字符串字面量）
    let ver = version.replace('\\', "\\\\").replace('\'', "\\'");
    let url_js = url.replace('\\', "\\\\").replace('\'', "\\'");
    TEMPLATE
        .replace("__VER__", &ver)
        .replace("__URL__", &url_js)
}

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
