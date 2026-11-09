//! 数据隔离命令（D6 拆分）：复制共享 home / 清理隔离数据 / 打开隔离目录。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
#[specta::specta]
pub(crate) async fn copy_shared_to_isolated(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
) -> Result<String, String> {
    let (cfg, dirs) = ctx.config_dirs();
    if !cfg.isolated_versions.iter().any(|v| v == &version) {
        return Err(format!("{} 未开启隔离", version));
    }
    let shared = dirs.home.clone();
    let isolated = crate::paths::resolve_home(&cfg, &dirs, &version);
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
#[specta::specta]
pub(crate) async fn clear_isolated_data(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
) -> Result<String, String> {
    let (cfg, dirs) = ctx.config_dirs();
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let (ok, msg) =
        tauri::async_runtime::spawn_blocking(move || versions::clear_home_dir(&home, &version))
            .await
            .map_err(|e| format!("清理任务失败: {}", e))?;
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn open_isolated_dir(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
) -> Result<(), String> {
    let (cfg, dirs) = ctx.config_dirs();
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}
