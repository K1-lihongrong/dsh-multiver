mod actions;
mod config;
mod envcheck;
mod versions;

use config::{Config, Dirs};
use serde::Serialize;
use std::path::PathBuf;

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
fn list_remote() -> Result<Vec<String>, String> {
    let (ok, list, err) = actions::list_remote();
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
        versions::install(&dirs.versions, &dirs.store, &version, &on_stage)
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

    // 生成转发脚本（版本号与根目录直接写进脚本，避免解析 JSON）
    let target = path_bin_dir(&mdir);
    match &version {
        Some(v) => {
            let root = cfg.resolve_root(&mdir);
            let script = actions::build_forward_script(v, &root.to_string_lossy());
            actions::write_forward_script(&target, &script).map_err(|e| e.to_string())?;
            Ok(format!("默认版本已设为 {}，dsh 命令已就绪", v))
        }
        None => {
            // 清除默认版本：删除转发脚本
            let script_path = target.join("dsh.cmd");
            let _ = std::fs::remove_file(script_path);
            Ok("已清除默认版本".to_string())
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

#[tauri::command]
fn run_version(app: tauri::AppHandle, version: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(&version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    // 隔离版本使用自己的 DSH_HOME；否则用共享的
    let home = if cfg.isolated_versions.iter().any(|v| v == &version) {
        let isolated_home = vdir.join("home");
        let _ = std::fs::create_dir_all(&isolated_home);
        isolated_home
    } else {
        dirs.home.clone()
    };
    let (ok, msg) = actions::spawn_web(&vdir, &home, &dirs.store);
    if ok { Ok(msg) } else { Err(msg) }
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
    // 根目录变了，如果已设默认版本，重新生成转发脚本
    if let Some(v) = &cfg.default_version {
        let script = actions::build_forward_script(v, &dirs.root.to_string_lossy());
        let target = path_bin_dir(&mdir);
        let _ = actions::write_forward_script(&target, &script);
    }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            list_installed,
            list_remote,
            install_version,
            uninstall_version,
            set_default,
            run_version,
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
