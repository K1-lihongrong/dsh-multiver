mod actions;
mod config;
mod envcheck;
mod launcher;
mod versions;

use config::{Config, Dirs};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

/// 「窗口 label → (代次, dsh 子进程)」映射。
/// 代次用于解决同名窗口替换时的 Destroyed 回调竞态：回调只在自己那一代仍是当前项时才 kill。
type ProcMap = Arc<Mutex<HashMap<String, (u64, Child)>>>;

/// 全局单调递增的代次计数器（用于区分同名窗口的不同实例）
static NEXT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 管理器自身所在目录（config.json 的存放处）
fn manager_dir(_app: &tauri::AppHandle) -> PathBuf {
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

/// 一次返回给前端的完整状态
#[derive(Serialize)]
struct AppState {
    root_dir: String,
    versions_dir: String,
    home_dir: String,
    store_dir: String,
    cache_dir: String,
    state_dir: String,
    default_version: Option<String>,
    manager_dir: String,
}

#[tauri::command]
fn get_state(app: tauri::AppHandle) -> AppState {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let _ = dirs.ensure();
    AppState {
        root_dir: root.to_string_lossy().to_string(),
        versions_dir: dirs.versions.to_string_lossy().to_string(),
        home_dir: dirs.home.to_string_lossy().to_string(),
        store_dir: dirs.store.to_string_lossy().to_string(),
        cache_dir: dirs.cache.to_string_lossy().to_string(),
        state_dir: dirs.state.to_string_lossy().to_string(),
        default_version: cfg.default_version.clone(),
        manager_dir: mdir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn list_installed(app: tauri::AppHandle) -> Vec<versions::VersionInfo> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    versions::list(&dirs.versions, cfg.default_version.as_deref(), &cfg.isolated_versions)
}

#[tauri::command]
fn list_remote(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    let (ok, list, err) = actions::list_remote(&dirs.store, &dirs.cache, &dirs.state);
    if ok { Ok(list) } else { Err(err) }
}

#[tauri::command]
async fn install_version(app: tauri::AppHandle, version: String) -> Result<String, String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    let app2 = app.clone();
    // 把阻塞的安装逻辑丢到后台线程池，避免占用主线程导致界面卡死
    let result = tauri::async_runtime::spawn_blocking(move || {
        let on_stage = move |stage: &str| {
            let _ = app2.emit("install-progress", stage.to_string());
        };
        versions::install(&dirs.versions, &dirs.store, &dirs.cache, &dirs.state, &version, &on_stage)
    })
    .await
    .map_err(|e| format!("安装任务失败: {}", e))?;

    let (ok, msg) = result;
    if ok { Ok(msg) } else { Err(msg) }
}

#[tauri::command]
async fn uninstall_version(app: tauri::AppHandle, version: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    // 如果卸载的是默认版本，清空默认设置，并移除隔离标记
    let mut cfg = cfg;
    let mut need_save = false;
    if cfg.default_version.as_deref() == Some(version.as_str()) {
        cfg.default_version = None;
        need_save = true;
    }
    if cfg.isolated_versions.iter().any(|v| v == &version) {
        cfg.isolated_versions.retain(|v| v != &version);
        need_save = true;
    }
    if need_save {
        let _ = cfg.save(&mdir);
    }
    // 卸载可能清掉了默认版本 / 隔离标记：重生成转发脚本（无默认版本时会删除 dsh.cmd）
    regenerate_forward_script(&mdir, &cfg);
    // 删除目录可能很慢，放到后台线程
    let (ok, msg) = tauri::async_runtime::spawn_blocking(move || {
        versions::uninstall(&dirs.versions, &version)
    })
    .await
    .map_err(|e| format!("卸载任务失败: {}", e))?;
    if ok { Ok(msg) } else { Err(msg) }
}

#[tauri::command]
fn set_default(app: tauri::AppHandle, version: Option<String>) -> Result<String, String> {
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

/// 根据当前配置，重生成（或删除）终端 dsh 转发脚本 dsh.cmd。
///
/// - 有默认版本：写入 dsh.cmd（含 DSH_HOME，按默认版本是否隔离决定）
/// - 无默认版本：删除 dsh.cmd
/// 供 set_default / set_root / set_isolated / uninstall_version 统一调用。
fn regenerate_forward_script(mdir: &PathBuf, cfg: &Config) {
    let target = path_bin_dir(mdir);
    match &cfg.default_version {
        Some(v) => {
            let isolated = cfg.isolated_versions.iter().any(|x| x == v);
            let root = cfg.resolve_root(mdir);
            let script = actions::build_forward_script(v, &root.to_string_lossy(), isolated);
            let _ = actions::write_forward_script(&target, &script);
        }
        None => {
            let _ = std::fs::remove_file(target.join("dsh.cmd"));
        }
    }
}

/// 决定 dsh.cmd 写到哪个目录：优先 npm 全局 bin，其次管理器目录
fn path_bin_dir(manager: &PathBuf) -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let npm_bin = PathBuf::from(appdata).join("npm");
        if npm_bin.exists() {
            return npm_bin;
        }
    }
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
async fn launch_window(
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

    // 阻塞部分（起进程 + 等端口/URL，最多 30s）丢到后台线程，避免冻结界面
    let vdir2 = vdir.clone();
    let home2 = home.clone();
    let store2 = dirs.store.clone();
    let cache2 = dirs.cache.clone();
    let state2 = dirs.state.clone();
    let spawned = tauri::async_runtime::spawn_blocking(move || {
        launcher::spawn_web_hidden(&vdir2, &home2, &store2, &cache2, &state2)
    })
    .await
    .map_err(|e| format!("启动任务失败: {}", e))?;
    let (child, url) = spawned?;

    // 回到主线程建窗
    let label = window_label(&version);
    // 若同名窗口已存在：先显式结束其旧进程（从 map 移除并 kill），再关闭旧窗口。
    if let Some((_, mut old_child)) = procs.lock().unwrap().remove(&label) {
        // 结束旧进程及其子进程树（kill 不 wait，避免阻塞）
        launcher::kill_tree(&mut old_child);
    }
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.close();
    }

    let generation = NEXT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let title = format!("DSH {}", version);
    let init_script = build_topbar_script(&version, &url);
    let parsed = url.parse().map_err(|e| format!("URL 解析失败: {}", e))?;

    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
        .title(&title)
        .inner_size(1100.0, 760.0)
        .initialization_script(&init_script)
        .build();
    let window = match built {
        Ok(w) => w,
        Err(e) => {
            let mut c = child;
            launcher::kill_tree(&mut c);
            return Err(format!("创建窗口失败: {}", e));
        }
    };

    procs.lock().unwrap().insert(label.clone(), (generation, child));

    let procs2 = procs.clone();
    let label2 = label.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            let mut map = procs2.lock().unwrap();
            let should_kill = matches!(map.get(&label2), Some((g, _)) if *g == generation);
            if should_kill {
                if let Some((_, mut child)) = map.remove(&label2) {
                    // 结束整棵进程树；不 wait，避免阻塞事件线程导致关窗卡顿
                    launcher::kill_tree(&mut child);
                }
            }
        }
    });

    Ok(format!("已在窗口打开 DSH {}", version))
}

#[tauri::command]
async fn run_version(
    app: tauri::AppHandle,
    procs: tauri::State<'_, ProcMap>,
    version: String,
) -> Result<String, String> {
    launch_window(app, procs.inner().clone(), version).await
}

#[tauri::command]
fn create_shortcut(app: tauri::AppHandle, version: String) -> Result<String, String> {
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
async fn open_in_browser(
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
    let label = window_label(&version);
    let has_window = procs.lock().unwrap().contains_key(&label);
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
        Err(msg)
    }
}

/// 重启某版本窗口对应的 dsh web 进程：
/// kill 旧进程 → 用同版本起新进程（端口重分配）→ 窗口导航到新 URL。
#[tauri::command]
async fn restart_version(
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

    let label = window_label(&version);
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("窗口 {} 不存在", label))?;

    // 先结束旧进程及其进程树（不 wait）
    if let Some((_, mut old)) = procs.lock().unwrap().remove(&label) {
        launcher::kill_tree(&mut old);
    }

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
    .map_err(|e| format!("重启任务失败: {}", e))?;
    let (child, url) = spawned?;

    let generation = NEXT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    procs.lock().unwrap().insert(label.clone(), (generation, child));

    window
        .navigate(url.parse().map_err(|e| format!("URL 解析失败: {}", e))?)
        .map_err(|e| format!("窗口导航失败: {}", e))?;

    Ok(())
}


/// 注入到 dsh 页面的顶部可折叠小栏（显示版本号 + 完整 URL + 重启按钮）
///
/// 说明：这里用普通字符串拼接而非 format!，避免 JS 里大量 {} 触发 Rust 格式串转义。
fn build_topbar_script(version: &str, url: &str) -> String {
    let ver = version.replace('\\', "\\\\").replace('\'', "\\'");
    let url_js = url.replace('\\', "\\\\").replace('\'', "\\'");
    let mut s = String::new();
    s.push_str("(function() {\n");
    s.push_str("  if (window.__dshBarInjected) return;\n");
    s.push_str("  window.__dshBarInjected = true;\n");
    s.push_str(&format!("  var VER = '{}';\n", ver));
    s.push_str(&format!("  var URL = '{}';\n", url_js));
    s.push_str("  function inject() {\n");
    s.push_str("    if (!document.body) return setTimeout(inject, 50);\n");
    s.push_str("    if (document.getElementById('__dsh_bar')) return;\n");
    s.push_str("    var bar = document.createElement('div');\n");
    s.push_str("    bar.id = '__dsh_bar';\n");
    s.push_str("    bar.style.cssText = 'position:fixed;top:0;left:0;right:0;height:32px;z-index:2147483647;'\n");
    s.push_str("      + 'background:#1f2328;color:#fff;font:12px/32px \"Segoe UI\",system-ui,sans-serif;'\n");
    s.push_str("      + 'display:flex;align-items:center;gap:12px;padding:0 12px;box-shadow:0 1px 4px rgba(0,0,0,.3);';\n");
    s.push_str("    var label = document.createElement('span');\n");
    s.push_str("    label.textContent = 'DSH ' + VER;\n");
    s.push_str("    label.style.cssText = 'font-weight:600;white-space:nowrap;';\n");
    s.push_str("    var urlBox = document.createElement('input');\n");
    s.push_str("    urlBox.type = 'text';\n");
    s.push_str("    urlBox.value = URL;\n");
    s.push_str("    urlBox.readOnly = true;\n");
    s.push_str("    urlBox.title = '点击可选中，复制到浏览器打开';\n");
    s.push_str("    urlBox.style.cssText = 'flex:1;min-width:120px;background:#2b2f36;color:#cfd8e3;border:1px solid #3a3f47;border-radius:4px;padding:2px 8px;font:12px/1.6 monospace;';\n");
    s.push_str("    urlBox.onclick = function() { urlBox.select(); };\n");
    s.push_str("    var btnRestart = document.createElement('button');\n");
    s.push_str("    btnRestart.textContent = '重启';\n");
    s.push_str("    btnRestart.style.cssText = 'cursor:pointer;background:#4f6ef7;color:#fff;border:0;border-radius:4px;padding:3px 10px;font-size:12px;white-space:nowrap;';\n");
    s.push_str("    btnRestart.onclick = function() {\n");
    s.push_str("      try {\n");
    s.push_str("        var inv = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;\n");
    s.push_str("        if (inv) { inv('restart_version', { version: VER }).catch(function() { location.reload(); }); return; }\n");
    s.push_str("      } catch (e) {}\n");
    s.push_str("      location.reload();\n");
    s.push_str("    };\n");
    s.push_str("    var btnFold = document.createElement('button');\n");
    s.push_str("    btnFold.textContent = '收起';\n");
    s.push_str("    btnFold.style.cssText = 'cursor:pointer;background:transparent;color:#fff;border:1px solid #555;border-radius:4px;padding:3px 10px;font-size:12px;white-space:nowrap;';\n");
    s.push_str("    var folded = false;\n");
    s.push_str("    btnFold.onclick = function() {\n");
    s.push_str("      folded = !folded;\n");
    s.push_str("      bar.style.height = folded ? '18px' : '32px';\n");
    s.push_str("      bar.style.lineHeight = folded ? '18px' : '32px';\n");
    s.push_str("      label.style.display = folded ? 'none' : '';\n");
    s.push_str("      urlBox.style.display = folded ? 'none' : '';\n");
    s.push_str("      btnRestart.style.display = folded ? 'none' : '';\n");
    s.push_str("      btnFold.textContent = folded ? '展开' : '收起';\n");
    s.push_str("      var st = document.getElementById('__dsh_bar_style');\n");
    s.push_str("      if (st) st.textContent = 'body{padding-top:' + (folded ? '18px' : '32px') + ' !important;}';\n");
    s.push_str("    };\n");
    s.push_str("    bar.appendChild(label);\n");
    s.push_str("    bar.appendChild(urlBox);\n");
    s.push_str("    bar.appendChild(btnRestart);\n");
    s.push_str("    bar.appendChild(btnFold);\n");
    s.push_str("    var style = document.createElement('style');\n");
    s.push_str("    style.id = '__dsh_bar_style';\n");
    s.push_str("    style.textContent = 'body{padding-top:32px !important;}';\n");
    s.push_str("    document.head.appendChild(style);\n");
    s.push_str("    document.body.appendChild(bar);\n");
    s.push_str("  }\n");
    s.push_str("  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', inject);\n");
    s.push_str("  else inject();\n");
    s.push_str("})();\n");
    s
}


#[tauri::command]
fn set_isolated(app: tauri::AppHandle, version: String, isolated: bool) -> Result<String, String> {
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

#[tauri::command]
async fn scan_isolated_size(app: tauri::AppHandle, version: String) -> Result<u64, String> {
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

#[tauri::command]
async fn copy_shared_to_isolated(app: tauri::AppHandle, version: String) -> Result<String, String> {
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
async fn clear_isolated_data(app: tauri::AppHandle, version: String) -> Result<String, String> {
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
fn open_isolated_dir(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = versions::isolated_home(&dirs.versions, &version);
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok { Ok(()) } else { Err(msg) }
}

#[tauri::command]
fn set_root(app: tauri::AppHandle, root: Option<String>) -> Result<String, String> {
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

#[tauri::command]
fn open_dir(app: tauri::AppHandle, which: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let target = match which.as_str() {
        "root" => dirs.root.clone(),
        "versions" => dirs.versions.clone(),
        "home" => dirs.home.clone(),
        "store" => dirs.store.clone(),
        "cache" => dirs.cache.clone(),
        "state" => dirs.state.clone(),
        _ => dirs.root.clone(),
    };
    let (ok, msg) = actions::open_folder(&target);
    if ok { Ok(()) } else { Err(msg) }
}

#[tauri::command]
fn get_manager_dir(app: tauri::AppHandle) -> String {
    manager_dir(&app).to_string_lossy().to_string()
}

#[tauri::command]
fn check_env(app: tauri::AppHandle) -> Vec<envcheck::CheckItem> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    envcheck::run_all(&dirs.root)
}

/// 把精简启动模式的错误写入 <根>/logs/launch-error.log（windows_subsystem=windows 下无控制台，便于排查）
fn log_launch_error(root: &std::path::Path, msg: &str) {
    use std::io::Write;
    let dir = root.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("launch-error.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

/// 由版本号生成合法的窗口 label。
/// Tauri 要求 label 只含字母数字和 `-` `/` `:` `_`，而版本号含 `.`，故替换为 `_`。
fn window_label(version: &str) -> String {
    let sanitized: String = version
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') { c } else { '_' })
        .collect();
    format!("dsh-{}", sanitized)
}

/// 从命令行参数解析出 --launch-version <版本>（精简启动模式用）
fn parse_launch_version(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--launch-version" {
            return it.next().cloned();
        }
    }
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let procs: ProcMap = Arc::new(Mutex::new(HashMap::new()));
    let procs_setup = procs.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(procs.clone())
        .invoke_handler(tauri::generate_handler![
            get_state,
            list_installed,
            list_remote,
            install_version,
            uninstall_version,
            set_default,
            run_version,
            open_in_browser,
            create_shortcut,
            restart_version,
            set_root,
            open_dir,
            get_manager_dir,
            check_env,
            set_isolated,
            scan_isolated_size,
            copy_shared_to_isolated,
            clear_isolated_data,
            open_isolated_dir,
        ])
        .setup(move |app| {
            use tauri::Manager;
            // 精简启动模式：命令行带 --launch-version <版本> 时，
            // 关闭默认主窗口，直接打开该版本的内嵌窗口。
            if let Some(version) = parse_launch_version(&std::env::args().collect::<Vec<_>>()) {
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
                    match launch_window(app_handle.clone(), procs_for_launch, version_for_launch.clone()).await {
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
                            log_launch_error(&root, &format!("版本 {} 启动失败: {}", version_for_launch, e));
                            eprintln!("[dsh-multiver] 启动失败: {}", e);
                            // 启动失败时把主窗口显示出来，避免用户看到"什么都没有"
                            if let Some(main) = app_handle.get_webview_window("main") {
                                let _ = main.show();
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
                let mut map = procs.lock().unwrap();
                for (_, (_, child)) in map.iter_mut() {
                    launcher::kill_tree(child);
                }
                map.clear();
            }
        });
}



