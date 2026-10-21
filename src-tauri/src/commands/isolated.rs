//! 数据隔离命令（D6 拆分）：复制共享 home / 清理隔离数据 / 打开隔离目录。
#![allow(unused_imports)]
use super::*;

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
