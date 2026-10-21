//! dsh 插件管理命令（D6 拆分：原 commands.rs 的插件相关，双向桥 B1）。
#![allow(unused_imports)]
use super::*;

/// 按版本解析 dsh CLI 与 DSH_HOME（隔离 / 官方 / 共享）。
fn resolve_dsh(
    app: &tauri::AppHandle,
    version: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let mdir = manager_dir(app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, version);
    let bin = crate::dsplugins::dsh_bin(&vdir)
        .ok_or_else(|| format!("版本 {} 缺少 dsh 入口", version))?;
    Ok((bin, home))
}

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
///
/// B1-04：改为 async + `spawn_blocking`，带 300s 超时；执行期间按行 emit
/// `ds-plugin-progress` 事件供前端展示进度（B1-08）。
#[tauri::command]
pub(crate) async fn ds_plugin_action(
    app: tauri::AppHandle,
    version: String,
    profile: String,
    action: String,
    package: String,
) -> Result<String, String> {
    // 先校验：包名防选项/特殊字符透传（B1-05）；profile 名防路径穿越。
    crate::dsplugins::validate_package_spec(&package)?;
    crate::dsplugins::validate_profile_name(&profile)?;
    let (bin, home) = resolve_dsh(&app, &version)?;
    let package = package.trim().to_string();
    let args: Vec<String> = match action.as_str() {
        "add" => vec!["add".into(), package],
        "remove" => vec!["remove".into(), package],
        _ => return Err(format!("未知操作：{}", action)),
    };

    let app2 = app.clone();
    let version2 = version.clone();
    let profile2 = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let res = crate::dsplugins::run_dsh_plugin(
            &bin,
            &home,
            &profile2,
            &args_ref,
            crate::dsplugins::PLUGIN_TIMEOUT,
            |line| {
                let _ = app2.emit(
                    "ds-plugin-progress",
                    serde_json::json!({
                        "version": version2,
                        "profile": profile2,
                        "line": line,
                    }),
                );
            },
        );
        if res.timed_out {
            Err(format!(
                "插件操作超时（{} 秒），已中止",
                crate::dsplugins::PLUGIN_TIMEOUT.as_secs()
            ))
        } else if res.ok {
            Ok(res.output)
        } else {
            Err(res.output)
        }
    })
    .await
    .map_err(|e| format!("插件操作执行失败：{e}"))?
}

/// 初始化一个 profile（B1-07）：调 `dsh plugin --profile <p> list`，
/// dsh 会在首次使用时创建 profile 目录并安装模板 bundle。
#[tauri::command]
pub(crate) async fn ds_init_profile(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<String, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (bin, home) = resolve_dsh(&app, &version)?;

    let app2 = app.clone();
    let version2 = version.clone();
    let profile2 = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let res = crate::dsplugins::run_dsh_plugin(
            &bin,
            &home,
            &profile2,
            &["list"],
            crate::dsplugins::PLUGIN_TIMEOUT,
            |line| {
                let _ = app2.emit(
                    "ds-plugin-progress",
                    serde_json::json!({
                        "version": version2,
                        "profile": profile2,
                        "line": line,
                    }),
                );
            },
        );
        if res.timed_out {
            Err(format!(
                "初始化超时（{} 秒），已中止",
                crate::dsplugins::PLUGIN_TIMEOUT.as_secs()
            ))
        } else if res.ok {
            Ok(res.output)
        } else {
            Err(res.output)
        }
    })
    .await
    .map_err(|e| format!("初始化执行失败：{e}"))?
}

/// 打开某版本的 profile 目录（资源管理器）。
#[tauri::command]
pub(crate) fn ds_open_profile_dir(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<(), String> {
    crate::dsplugins::validate_profile_name(&profile)?;
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
