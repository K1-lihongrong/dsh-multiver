//! Tauri 命令层（从 lib.rs 抽出，GAP-003 选项 B；纯移动，无逻辑改动）。
//!
//! 这里集中所有 #[tauri::command] 及其专属 helper（状态、窗口启动、配置维护、脚本生成）。
//! 共享的进程映射类型与 manager_dir 仍在 crate 根（lib.rs）。

use crate::config::{Config, Dirs};
use crate::logging;
use crate::window;
use crate::{actions, cli, config, envcheck, launcher, maintenance, versions};
use crate::{lock_procs, manager_dir, ProcMap, NEXT_GEN};
use serde::Serialize;
use std::path::PathBuf;

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
    }
}

#[tauri::command]
pub(crate) fn list_installed(app: tauri::AppHandle) -> Vec<versions::VersionInfo> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    versions::list(&dirs.versions, cfg.default_version.as_deref(), &cfg.isolated_versions)
}

#[tauri::command]
pub(crate) fn list_remote(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    let (ok, list, err) = actions::list_remote(&dirs.store, &dirs.cache, &dirs.state);
    if ok { Ok(list) } else { Err(err) }
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
                            .create(true).append(true)
                            .open(dir.join("progress-debug.log"))
                        {
                            let _ = writeln!(f, "step={} total={} stage={:?} detail={:?} frac={:.2}",
                                ev.step, ev.total, ev.stage, ev.detail, ev.fraction);
                        }
                    }
                }
            }
            let _ = app2.emit("install-progress", ev.clone());
        };
        versions::install(&dirs.versions, &dirs.store, &dirs.cache, &dirs.state, &version, &on_progress, registry.as_deref())
    })
    .await
    .map_err(|e| format!("安装任务失败: {}", e))?;

    let (ok, msg) = result;
    // 维护 broken_versions：失败且分类为「私有包/不完整」时记入；成功时移除。
    let mut c = cfg.clone();
    let kind = if ok { "" } else { versions::classify_error(&msg) };
    if c.apply_install_result(&version_for_record, ok, kind) {
        let _ = c.save(&mdir);
    }
    if ok { Ok(msg) } else { Err(msg) }
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
pub(crate) async fn uninstall_version(app: tauri::AppHandle, version: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    // 如果卸载的是默认版本，清空默认设置，并移除隔离标记
    let mut cfg = cfg;
    if cfg.remove_version_refs(&version) {
        let _ = cfg.save(&mdir);
    }
    // 卸载可能清掉了默认版本 / 隔离标记：重生成转发脚本（无默认版本时会删除 dsh.cmd）
    regenerate_forward_script(&mdir, &cfg);
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
    if ok { Ok(msg) } else { Err(msg) }
}

#[tauri::command]
pub(crate) fn set_default(app: tauri::AppHandle, version: Option<String>) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if let Some(v) = &version {
        if !versions::exists(&dirs.versions, v) {
            return Err(format!("版本 {} 未安装", v));
        }
    }
    cfg.default_version = version.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;

    // 生成/删除转发脚本（版本号、根目录、DSH_HOME 直接写进脚本）
    regenerate_forward_script(&mdir, &cfg);
    match &version {
        Some(v) => Ok(format!("默认版本已设为 {}，dsh 命令已就绪", v)),
        None => Ok("已清除默认版本".to_string()),
    }
}

/// 根据当前配置，重生成（或删除）终端 dsh 转发脚本。
///
/// - 有默认版本：写入转发脚本（Windows 为 dsh.cmd，Unix 为 dsh；含 DSH_HOME）
/// - 无默认版本：删除转发脚本
/// 供 set_default / set_root / set_isolated / uninstall_version 统一调用。
pub(crate) fn regenerate_forward_script(mdir: &PathBuf, cfg: &Config) {
    let target = path_bin_dir(mdir);
    match &cfg.default_version {
        Some(v) => {
            let isolated = cfg.isolated_versions.iter().any(|x| x == v);
            let root = cfg.resolve_root(mdir);
            let script = actions::build_forward_script(v, &root.to_string_lossy(), isolated);
            let _ = actions::write_forward_script(&target, &script);
        }
        None => {
            #[cfg(windows)]
            let _ = std::fs::remove_file(target.join("dsh.cmd"));
            #[cfg(unix)]
            let _ = std::fs::remove_file(target.join("dsh"));
        }
    }
}

/// 决定转发脚本写到哪个目录。
///
/// - Windows：优先 `%APPDATA%\npm`（npm 全局 bin），否则管理器目录
/// - Unix：优先 `~/.local/bin`（XDG，多数发行版已在 PATH），其次 `~/.local/share/pnpm`、
///   `~/.npm-global/bin`；都不存在则创建 `~/.local/bin`。最后回退管理器目录。
#[cfg(windows)]
fn path_bin_dir(manager: &PathBuf) -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let npm_bin = PathBuf::from(appdata).join("npm");
        if npm_bin.exists() {
            return npm_bin;
        }
    }
    manager.clone()
}

#[cfg(unix)]
fn path_bin_dir(manager: &PathBuf) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let candidates = [
            format!("{}/.local/bin", home),
            format!("{}/.local/share/pnpm", home),
            format!("{}/.npm-global/bin", home),
        ];
        for c in &candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return p;
            }
        }
        // 都不存在：创建 XDG 标准的 ~/.local/bin
        let fallback = PathBuf::from(&candidates[0]);
        if std::fs::create_dir_all(&fallback).is_ok() {
            return fallback;
        }
    }
    manager.clone()
}

#[cfg(not(any(windows, unix)))]
fn path_bin_dir(manager: &PathBuf) -> PathBuf {
    manager.clone()
}

/// 计算某版本实际使用的 DSH_HOME（隔离版本用独立目录，否则用共享 home）
fn resolve_home(cfg: &Config, dirs: &Dirs, version: &str) -> PathBuf {
    let vdir = dirs.versions.join(version);
    if cfg.isolated_versions.iter().any(|v| v == version) {
        let isolated_home = vdir.join("home");
        let _ = std::fs::create_dir_all(&isolated_home);
        isolated_home
    } else {
        dirs.home.clone()
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
    let home = resolve_home(&cfg, &dirs, &version);

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

    // 回到主线程建窗。
    // 先结束该版本的旧窗口/旧进程（若有）。
    let old = lock_procs(&procs).remove(&version);
    if let Some((old_label, _old_gen, mut old_child, _old_job)) = old {
        // 结束旧进程及其子进程树（kill 不 wait，避免阻塞）
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
            logging::log_app_error(&dirs.root, "launch_window", &format!("{} 创建窗口失败: {}", version, e));
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
            logging::log_app_error(&dirs.root, "launch_window", &format!("{} 设窗口 AUMID 失败: {}", version, e));
        }
    }

    // 设好 AUMID / 图标后再显示
    if let Err(e) = window.show() {
        logging::log_app_error(&dirs.root, "launch_window", &format!("{} 显示窗口失败: {}", version, e));
    }

    lock_procs(&procs).insert(version.clone(), (label.clone(), generation, child, job));

    let procs2 = procs.clone();
    let version2 = version.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let mut map = lock_procs(&procs2);
            // 仅当该版本仍是"本代次"时才 kill，避免旧窗口滞后回调误杀新进程
            let should_kill = matches!(map.get(&version2), Some((_, g, _, _)) if *g == generation);
            if should_kill {
                if let Some((_, _, mut child, _job)) = map.remove(&version2) {
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
                format!("版本 {} 的 dsh 启动后立即退出。请查看日志目录：\n{}", version3, logs_dir.to_string_lossy())
            } else {
                format!("版本 {} 的 dsh 启动后立即退出。dsh 输出：\n{}", version3, detail)
            };
            logging::log_app_error(&logs_dir, "launch_window", &format!("{} 启动后立即退出", version3));
            use tauri::Emitter;
            let _ = app2.emit("launch-crashed", serde_json::json!({
                "version": version3,
                "detail": detail,
                "message": msg,
            }));
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
    let home = resolve_home(&cfg, &dirs, &version);

    // 1) 互斥提示：该版本已有内嵌窗口在开
    let has_window = lock_procs(procs.inner()).contains_key(&version);
    if has_window {
        let ok = app
            .dialog()
            .message(format!("版本 {} 已在窗口打开，确定还要在浏览器再开一个吗？", version))
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
    let (ok, msg) = launcher::spawn_web_console(
        &vdir, &home, &dirs.store, &dirs.cache, &dirs.state, port,
    );
    if ok {
        // 提示：端口 + 数据目录
        Ok(format!("{}；数据目录：{}", msg, home.to_string_lossy()))
    } else {
        logging::log_app_error(&dirs.root, "open_in_browser", &format!("{} ({})", version, msg));
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
    let home = resolve_home(&cfg, &dirs, &version);

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
        logging::log_app_error(&dirs.root, "restart_version", &format!("{} ({})", version, m));
        m
    })?;
    let (child, url, job) = spawned.map_err(|e| {
        logging::log_app_error(&dirs.root, "restart_version", &format!("{} ({})", version, e));
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
    TEMPLATE.replace("__VER__", &ver).replace("__URL__", &url_js)
}

#[tauri::command]
pub(crate) fn set_isolated(app: tauri::AppHandle, version: String, isolated: bool) -> Result<String, String> {
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
    regenerate_forward_script(&mdir, &cfg);
    if isolated {
        Ok(format!("{} 已开启数据隔离", version))
    } else {
        Ok(format!("{} 已关闭数据隔离", version))
    }
}

/// 扫描整个版本目录的占用（含依赖 + 隔离 home），返回总量 + 共享/独占详情。
#[tauri::command]
pub(crate) async fn scan_version_size(app: tauri::AppHandle, version: String) -> Result<versions::SizeInfo, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let versions_dir = dirs.versions.clone();
    let v = version.clone();
    let info = tauri::async_runtime::spawn_blocking(move || {
        versions::version_size_detail(&versions_dir, &v)
    })
    .await
    .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(info)
}

#[tauri::command]
pub(crate) async fn scan_isolated_size(app: tauri::AppHandle, version: String) -> Result<u64, String> {
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
            logging::log_maintenance(&root, &format!("[手动] 清理孤立 webview：{} 项 {:?}", removed.len(), removed));
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
pub(crate) async fn copy_shared_to_isolated(app: tauri::AppHandle, version: String) -> Result<String, String> {
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
    if ok { Ok(msg) } else { Err(msg) }
}

#[tauri::command]
pub(crate) async fn clear_isolated_data(app: tauri::AppHandle, version: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let (ok, msg) = tauri::async_runtime::spawn_blocking(move || {
        versions::clear_isolated(&dirs.versions, &version)
    })
    .await
    .map_err(|e| format!("清理任务失败: {}", e))?;
    if ok { Ok(msg) } else { Err(msg) }
}

#[tauri::command]
pub(crate) fn open_isolated_dir(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = versions::isolated_home(&dirs.versions, &version);
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok { Ok(()) } else { Err(msg) }
}

#[tauri::command]
pub(crate) fn set_root(app: tauri::AppHandle, root: Option<String>) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.root_dir = root.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    dirs.ensure().map_err(|e| e.to_string())?;
    // 根目录变了，重生成转发脚本（含新的 DSH_HOME）
    regenerate_forward_script(&mdir, &cfg);
    Ok(format!("根目录已设为 {}", dirs.root.to_string_lossy()))
}

/// 用系统默认浏览器打开一个 URL（环境配置引导用）。
#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只允许打开 http/https 链接".to_string());
    }
    let (ok, msg) = actions::open_url(&url);
    if ok { Ok(()) } else { Err(msg) }
}

#[tauri::command]
pub(crate) fn open_dir(app: tauri::AppHandle, which: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let target = dirs.resolve(&which);
    let (ok, msg) = actions::open_folder(&target);
    if ok { Ok(()) } else { Err(msg) }
}

#[tauri::command]
pub(crate) fn check_env(app: tauri::AppHandle) -> Vec<envcheck::CheckItem> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    envcheck::run_all(&dirs.root)
}

/// 前端（WebView）JS 错误上报：写入 <根>/logs/frontend.log。
/// 历史踩坑：App.vue 缺 import 导致白屏、window.confirm 在 WebView2 静默失效——
/// 这些前端异常过去无任何留痕。此命令为其提供落盘通道。
#[tauri::command]
pub(crate) fn log_frontend(app: tauri::AppHandle, msg: String) {
    let mdir = manager_dir(&app);
    let root = Config::load(&mdir).resolve_root(&mdir);
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
        let h = resolve_home(&cfg, &dirs, "0.1.0");
        assert_eq!(h, dirs.home);
    }

    #[test]
    fn resolve_home_isolated_uses_version_subdir() {
        let mut cfg = Config::default();
        cfg.isolated_versions = vec!["0.1.0".to_string()];
        let base = std::env::temp_dir().join(format!(
            "dsh-rh-iso-{}",
            crate::logging::now_secs()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let dirs = Dirs::new(base.clone());
        let h = resolve_home(&cfg, &dirs, "0.1.0");
        // 隔离版本 → <root>/versions/<ver>/home
        assert_eq!(h, dirs.versions.join("0.1.0").join("home"));
        // 该目录应被创建
        assert!(h.exists(), "隔离 home 应被创建");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_home_isolated_only_for_listed_version() {
        let mut cfg = Config::default();
        cfg.isolated_versions = vec!["0.1.0".to_string()];
        let dirs = Dirs::new(std::env::temp_dir().join("dsh-rh-mixed"));
        // 未列入隔离的版本 → 共享 home
        let h = resolve_home(&cfg, &dirs, "0.2.0");
        assert_eq!(h, dirs.home);
    }
}
