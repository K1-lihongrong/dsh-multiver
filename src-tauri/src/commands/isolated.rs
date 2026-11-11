//! 数据隔离命令（D6 拆分）：复制共享 home / 清理隔离数据 / 打开隔离目录。
#![allow(unused_imports)]
use super::*;
use crate::domain::InstanceId;

/// 解析版本默认目标或显式实例的 home。返回 (home, 实例 id)。
pub(super) fn target_home(
    ctx: &crate::AppCtx,
    version: &str,
    instance: Option<&str>,
) -> Result<(std::path::PathBuf, Option<InstanceId>), String> {
    let (cfg, dirs) = ctx.config_dirs();
    let vid = crate::domain::VersionId::new(version)?;
    let explicit = match instance {
        Some(raw) => Some(InstanceId::new(raw)?),
        None => None,
    };
    let target = cfg.resolve_target(&vid, explicit.as_ref())?;
    Ok((
        crate::paths::resolve_home_ref(&dirs, &target.home_ref),
        target.instance,
    ))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn copy_shared_to_isolated(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    instance: Option<String>,
) -> Result<String, String> {
    let (isolated, instance_id) = target_home(ctx.inner(), &version, instance.as_deref())?;
    if instance_id.is_none() {
        return Err("该版本未开启隔离，请指定实例".into());
    }
    let shared = ctx.config_dirs().1.home.clone();
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
    instance: Option<String>,
) -> Result<String, String> {
    let (home, instance_id) = target_home(ctx.inner(), &version, instance.as_deref())?;
    if instance_id.is_none() {
        return Err("该版本未开启隔离，请指定实例".into());
    }
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
    instance: Option<String>,
) -> Result<(), String> {
    let (home, _) = target_home(ctx.inner(), &version, instance.as_deref())?;
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}
