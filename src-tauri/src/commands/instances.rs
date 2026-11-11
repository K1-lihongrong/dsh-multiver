//! 实例管理命令（M3）：读模型、CRUD、挂载与主按钮目标。

use super::*;
use crate::domain::{InstanceId, ProcKey, VersionId};
use crate::instances;

fn running_map(ctx: &crate::AppCtx) -> std::collections::BTreeMap<InstanceId, VersionId> {
    lock_procs(&ctx.procs)
        .iter()
        .filter_map(|(key, entry)| match key {
            ProcKey::Instance(id) => Some((id.clone(), entry.version.clone())),
            ProcKey::Shared(_) => None,
        })
        .collect()
}

#[tauri::command]
#[specta::specta]
pub(crate) fn list_instances(ctx: tauri::State<'_, crate::AppCtx>) -> instances::InstancesView {
    let (cfg, dirs) = ctx.config_dirs();
    instances::views(&cfg, &dirs, &running_map(&ctx))
}

#[tauri::command]
#[specta::specta]
pub(crate) fn instance_create(
    ctx: tauri::State<'_, crate::AppCtx>,
    name: String,
    attach: Vec<String>,
) -> Result<String, String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, dirs) = ctx.config_dirs();
    let id = instances::create(&mut cfg, &mdir, &dirs.versions, &name, &attach)?;
    Ok(id.to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn instance_rename(
    ctx: tauri::State<'_, crate::AppCtx>,
    id: String,
    name: String,
) -> Result<(), String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, _dirs) = ctx.config_dirs();
    let id = InstanceId::new(id)?;
    instances::rename(&mut cfg, &mdir, &id, &name)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn instance_remove(
    ctx: tauri::State<'_, crate::AppCtx>,
    id: String,
) -> Result<(), String> {
    let id = InstanceId::new(id)?;
    if lock_procs(&ctx.procs).contains_key(&ProcKey::Instance(id.clone())) {
        return Err(format!("实例 {id} 正在运行，请先关闭其窗口"));
    }
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, dirs) = ctx.config_dirs();
    instances::remove(&mut cfg, &mdir, &dirs.root, &id)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn attach_instance(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    id: String,
) -> Result<(), String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, dirs) = ctx.config_dirs();
    let version = VersionId::new(version)?;
    let id = InstanceId::new(id)?;
    instances::attach(&mut cfg, &mdir, &dirs.versions, &version, &id)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn detach_instance(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    id: String,
) -> Result<(), String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, _dirs) = ctx.config_dirs();
    let version = VersionId::new(version)?;
    let id = InstanceId::new(id)?;
    instances::detach(&mut cfg, &mdir, &version, &id)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn set_selected_target(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    instance: Option<String>,
) -> Result<(), String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, _dirs) = ctx.config_dirs();
    let version = VersionId::new(version)?;
    let target = match instance {
        Some(raw) => Some(InstanceId::new(raw)?),
        None => None,
    };
    instances::select_target(&mut cfg, &mdir, &version, target.as_ref())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn open_instance_dir(
    ctx: tauri::State<'_, crate::AppCtx>,
    id: String,
) -> Result<(), String> {
    let id = InstanceId::new(id)?;
    let (_cfg, dirs) = ctx.config_dirs();
    let home = dirs.homes.join(id.as_str());
    let _ = std::fs::create_dir_all(&home);
    let (ok, msg) = actions::open_folder(&home);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn scan_instance_size(
    ctx: tauri::State<'_, crate::AppCtx>,
    id: String,
) -> Result<u64, String> {
    let id = InstanceId::new(id)?;
    let (_cfg, dirs) = ctx.config_dirs();
    let home = dirs.homes.join(id.as_str());
    tauri::async_runtime::spawn_blocking(move || crate::versions::dir_size(&home))
        .await
        .map_err(|e| format!("扫描实例占用失败：{e}"))
}
