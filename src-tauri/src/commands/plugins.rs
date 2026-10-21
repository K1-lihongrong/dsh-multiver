//! dsh 插件管理命令（D6 拆分：原 commands.rs 的插件相关，双向桥 B1）。
#![allow(unused_imports)]
use super::*;

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
