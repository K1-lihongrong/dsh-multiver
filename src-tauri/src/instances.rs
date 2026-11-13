//! 实例读模型与共享操作（M2）。
//!
//! 这里不依赖 Tauri：CLI 与命令层共用同一套校验、创建、挂载、删除语义。

use crate::config::{trash_instance_home, Config, Dirs};
use crate::domain::{InstanceId, VersionId};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 单个实例的展示数据。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub(crate) struct InstanceView {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub state: String,
    pub home: String,
    pub mounted_versions: Vec<String>,
    pub orphan: bool,
    pub running_on: Option<String>,
    pub size: Option<u64>,
}

/// GUI 一次读取所需的全部实例数据。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub(crate) struct InstancesView {
    pub instances: Vec<InstanceView>,
    /// 版本 -> 主按钮目标；缺失表示共享 home。
    pub version_targets: BTreeMap<String, Option<String>>,
}

fn state_label(state: &crate::config::InstanceState) -> String {
    match state {
        crate::config::InstanceState::Ready => "ready".into(),
        crate::config::InstanceState::Deleting => "deleting".into(),
    }
}

/// 构造实例读模型。running 只由命令层（ProcMap）填充；CLI 传空表。
pub(crate) fn views(
    cfg: &Config,
    dirs: &Dirs,
    running: &BTreeMap<InstanceId, VersionId>,
) -> InstancesView {
    let mut instances: Vec<InstanceView> = cfg
        .instances
        .iter()
        .map(|inst| {
            let mounted = cfg.mounted_versions(&inst.id);
            InstanceView {
                id: inst.id.to_string(),
                name: inst.name.clone(),
                created_at: inst.created_at,
                state: state_label(&inst.state),
                home: dirs
                    .homes
                    .join(inst.id.as_str())
                    .to_string_lossy()
                    .to_string(),
                mounted_versions: mounted.iter().map(|v| v.to_string()).collect(),
                orphan: mounted.is_empty(),
                running_on: running.get(&inst.id).map(|v| v.to_string()),
                size: None,
            }
        })
        .collect();
    instances.sort_by(|a, b| a.id.cmp(&b.id));

    let version_targets = cfg
        .version_instances
        .keys()
        .map(|version| {
            (
                version.to_string(),
                cfg.selected_target
                    .get(version)
                    .and_then(|t| t.as_ref())
                    .map(|id| id.to_string()),
            )
        })
        .collect();

    InstancesView {
        instances,
        version_targets,
    }
}

/// 校验并创建实例，按顺序挂载。任一 attach 版本未安装则整体失败，不落盘。
pub(crate) fn create(
    cfg: &mut Config,
    manager_dir: &Path,
    versions_dir: &Path,
    name: &str,
    attaches: &[String],
) -> Result<InstanceId, String> {
    let mut versions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in attaches {
        let version = VersionId::new(raw)?;
        if !seen.insert(version.clone()) {
            return Err(format!("重复的 --attach：{version}"));
        }
        if !crate::versions::exists(versions_dir, version.as_str()) {
            return Err(format!("版本 {version} 未安装，不能挂载实例"));
        }
        versions.push(version);
    }

    let id = cfg.create_instance(name, crate::logging::now_secs())?;
    for version in versions {
        cfg.attach_instance(&version, &id)?;
    }
    if let Err(e) = cfg.save(manager_dir) {
        return Err(format!("保存配置失败：{e}"));
    }
    crate::paths::regenerate_forward_script(manager_dir, cfg);
    Ok(id)
}

pub(crate) fn rename(
    cfg: &mut Config,
    manager_dir: &Path,
    id: &InstanceId,
    name: &str,
) -> Result<(), String> {
    cfg.rename_instance(id, name)?;
    cfg.save(manager_dir)
        .map_err(|e| format!("保存配置失败：{e}"))
}

pub(crate) fn attach(
    cfg: &mut Config,
    manager_dir: &Path,
    versions_dir: &Path,
    version: &VersionId,
    id: &InstanceId,
) -> Result<bool, String> {
    if !crate::versions::exists(versions_dir, version.as_str()) {
        return Err(format!("版本 {version} 未安装"));
    }
    let changed = cfg.attach_instance(version, id)?;
    cfg.save(manager_dir)
        .map_err(|e| format!("保存配置失败：{e}"))?;
    crate::paths::regenerate_forward_script(manager_dir, cfg);
    Ok(changed)
}

pub(crate) fn detach(
    cfg: &mut Config,
    manager_dir: &Path,
    version: &VersionId,
    id: &InstanceId,
) -> Result<(), String> {
    cfg.detach_instance(version, id)?;
    cfg.save(manager_dir)
        .map_err(|e| format!("保存配置失败：{e}"))?;
    crate::paths::regenerate_forward_script(manager_dir, cfg);
    Ok(())
}

#[allow(dead_code)] // M3 GUI/API 接线后移除
pub(crate) fn select_target(
    cfg: &mut Config,
    manager_dir: &Path,
    version: &VersionId,
    target: Option<&InstanceId>,
) -> Result<(), String> {
    cfg.set_selected_target(version, target)?;
    cfg.save(manager_dir)
        .map_err(|e| format!("保存配置失败：{e}"))?;
    crate::paths::regenerate_forward_script(manager_dir, cfg);
    Ok(())
}

/// 删除实例：先落 state=deleting，再 rename 到 trash，最后清配置。
/// 返回 trash 路径（None 表示目录不存在或已退化为直接删）。
pub(crate) fn remove(
    cfg: &mut Config,
    manager_dir: &Path,
    root: &Path,
    id: &InstanceId,
) -> Result<Option<PathBuf>, String> {
    if !cfg.instance_exists(id) {
        return Err(format!("实例 {id} 不存在"));
    }
    if cfg
        .instances
        .iter()
        .any(|i| &i.id == id && i.state == crate::config::InstanceState::Deleting)
    {
        return Err(format!("实例 {id} 正在删除，请重启管理器续完"));
    }
    cfg.set_instance_state(id, crate::config::InstanceState::Deleting)?;
    cfg.save(manager_dir)
        .map_err(|e| format!("保存删除状态失败：{e}"))?;

    let trashed = match trash_instance_home(root, id) {
        Ok(path) => path,
        Err(e) => {
            let _ = cfg.set_instance_state(id, crate::config::InstanceState::Ready);
            let _ = cfg.save(manager_dir);
            return Err(e);
        }
    };

    cfg.remove_instance_record(id);
    if let Err(e) = cfg.save(manager_dir) {
        return Err(format!(
            "实例目录已移入回收站，但配置保存失败（重启后自动续完）：{e}"
        ));
    }
    crate::paths::regenerate_forward_script(manager_dir, cfg);
    Ok(trashed)
}
