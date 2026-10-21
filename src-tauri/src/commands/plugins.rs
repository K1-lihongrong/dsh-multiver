//! dsh 插件管理命令（D6 拆分：原 commands.rs 的插件相关，双向桥 B1）。
#![allow(unused_imports)]
use super::*;

/// 按版本解析 dsh CLI 与 DSH_HOME（隔离 / 官方 / 共享）。
fn resolve_dsh(
    app: &tauri::AppHandle,
    version: &str,
) -> Result<(crate::dsplugins::DshCli, std::path::PathBuf), String> {
    let mdir = manager_dir(app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let vdir = dirs.versions.join(version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, version);
    let cli = crate::dsplugins::dsh_cli(&vdir)
        .ok_or_else(|| format!("版本 {} 缺少 dsh 入口", version))?;
    Ok((cli, home))
}

/// dsh 插件管理（双向桥 B1）：列某版本下的所有 profile 与插件。
///
/// DSH_HOME 用 resolve_home 解析（隔离 / 官方 / 共享）。
/// 同时带上 DSH 安装随附的官方组合包名（对齐官方「随 DSH 提供」语义）。
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
    let official = crate::dsplugins::official_bundles(&dirs.versions.join(&version));
    Ok(crate::dsplugins::DsPluginList {
        dsh_home: home.to_string_lossy().to_string(),
        home_kind: home_kind.to_string(),
        profiles: crate::dsplugins::list_profiles(&home, &official),
    })
}

/// profile 级信息查询（B1-11/B1-12）：运行时版本、版本豁免、待决定的构建脚本、日志目录。
#[derive(serde::Serialize)]
pub(crate) struct DsProfileInfo {
    pub runtime_version: Option<String>,
    pub exemptions: serde_json::Value,
    pub pending_builds: Vec<String>,
    pub logs_dir: String,
}

#[tauri::command]
pub(crate) fn ds_profile_info(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<DsProfileInfo, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let pdir = crate::dsplugins::profiles_dir(&home).join(&profile);
    Ok(DsProfileInfo {
        runtime_version: crate::dsplugins::runtime_version(&dirs.versions.join(&version)),
        exemptions: crate::dsplugins::read_exemptions(&home, &profile),
        pending_builds: crate::dsplugins::read_pending_builds(&pdir),
        logs_dir: pdir
            .join(".plugin-manager")
            .join("logs")
            .to_string_lossy()
            .to_string(),
    })
}

/// 版本豁免操作（B1-12）：allow / revoke，走官方 CLI 子命令。
#[tauri::command]
pub(crate) async fn ds_exemption_action(
    app: tauri::AppHandle,
    version: String,
    profile: String,
    action: String,
    package_version: String,
    dsh_version: String,
) -> Result<String, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    crate::dsplugins::validate_package_spec(&package_version)?;
    crate::dsplugins::validate_package_spec(&dsh_version)
        .map_err(|_| "DSH 版本格式不正确".to_string())?;
    let (cli, home) = resolve_dsh(&app, &version)?;
    let args: Vec<String> = match action.as_str() {
        "allow" => vec![
            "allow-version".into(),
            package_version,
            "--dsh-version".into(),
            dsh_version,
            "--accept-risk".into(),
        ],
        "revoke" => vec![
            "revoke-version".into(),
            package_version,
            "--dsh-version".into(),
            dsh_version,
        ],
        _ => return Err(format!("未知操作：{}", action)),
    };
    let profile2 = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let res = crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
            &home,
            &profile2,
            &args_ref,
            crate::dsplugins::PLUGIN_TIMEOUT,
            |_| {},
        );
        if res.timed_out {
            Err(format!(
                "版本豁免操作超时（{} 秒）",
                crate::dsplugins::PLUGIN_TIMEOUT.as_secs()
            ))
        } else if res.ok {
            Ok(res.output)
        } else {
            Err(res.output)
        }
    })
    .await
    .map_err(|e| format!("版本豁免操作失败：{e}"))?
}

/// 允许待决定的构建脚本（B1-12）：写 `pnpm-workspace.yaml` 的 `allowBuilds`，返回剩余待决定项。
#[tauri::command]
pub(crate) fn ds_approve_builds(
    app: tauri::AppHandle,
    version: String,
    profile: String,
    packages: Vec<String>,
) -> Result<Vec<String>, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let pdir = crate::dsplugins::profiles_dir(&home).join(&profile);
    for p in &packages {
        crate::dsplugins::validate_package_spec(p)?;
    }
    crate::dsplugins::approve_builds(&pdir, &packages)?;
    Ok(crate::dsplugins::read_pending_builds(&pdir))
}

/// 打开 profile 的插件诊断日志目录（B1-11）。
#[tauri::command]
pub(crate) fn ds_open_plugin_logs(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<(), String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let dir = crate::dsplugins::profiles_dir(&home)
        .join(&profile)
        .join(".plugin-manager")
        .join("logs");
    let _ = std::fs::create_dir_all(&dir);
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

/// 插件操作结果（安装对话框「完成」页用：name 为新装/变更的依赖名；卸载为 None）。
#[derive(serde::Serialize)]
pub(crate) struct DsActionResult {
    pub output: String,
    pub name: Option<String>,
}

fn read_dep_map(profile_dir: &std::path::Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(profile_dir.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("dependencies").and_then(|d| d.as_object()).cloned())
        .unwrap_or_default()
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
    registry: Option<String>,
) -> Result<DsActionResult, String> {
    // 与官方安装对话框对齐：add 允许「包名 / GitHub 地址 / 本地目录」，remove 只允许包名。
    crate::dsplugins::validate_profile_name(&profile)?;
    match action.as_str() {
        "add" => crate::dsplugins::validate_install_spec(&package)?,
        "remove" => crate::dsplugins::validate_package_spec(&package)?,
        _ => return Err(format!("未知操作：{}", action)),
    }
    if let Some(url) = &registry {
        crate::dsplugins::validate_registry_url(url)?;
    }
    let (cli, home) = resolve_dsh(&app, &version)?;
    let pdir = crate::dsplugins::profiles_dir(&home).join(&profile);
    let before = read_dep_map(&pdir);
    let is_add = action == "add";
    let package = package.trim().to_string();
    let args: Vec<String> = match action.as_str() {
        "add" => {
            let mut a = vec!["add".to_string(), package];
            if let Some(url) = &registry {
                a.push(format!("--registry={}", url.trim()));
            }
            a
        }
        "remove" => vec!["remove".into(), package],
        _ => return Err(format!("未知操作：{}", action)),
    };

    let app2 = app.clone();
    let version2 = version.clone();
    let profile2 = profile.clone();
    let result: Result<String, String> = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let res = crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
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
    .map_err(|e| format!("插件操作执行失败：{e}"))?;
    let output = result?;
    let name = if is_add {
        let after = read_dep_map(&pdir);
        after
            .iter()
            .find(|(k, v)| before.get(*k) != Some(*v))
            .map(|(k, _)| k.clone())
    } else {
        None
    };
    Ok(DsActionResult { output, name })
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
    let (cli, home) = resolve_dsh(&app, &version)?;

    let app2 = app.clone();
    let version2 = version.clone();
    let profile2 = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let res = crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
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

/// 检查可更新插件（B1-13）：`dsh plugin --profile <p> outdated --json`。
#[tauri::command]
pub(crate) async fn ds_check_updates(
    app: tauri::AppHandle,
    version: String,
    profile: String,
) -> Result<Vec<crate::dsplugins::DsUpdate>, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cli, home) = resolve_dsh(&app, &version)?;
    let profile2 = profile.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
            &home,
            &profile2,
            &["outdated", "--json"],
            crate::dsplugins::PLUGIN_TIMEOUT,
            |_| {},
        )
    })
    .await
    .map_err(|e| format!("检查更新失败：{e}"))?;
    if res.timed_out {
        return Err("检查更新超时".into());
    }
    if serde_json::from_str::<serde_json::Value>(res.output.trim()).is_ok() {
        Ok(crate::dsplugins::parse_outdated_json(&res.output))
    } else {
        Err(res.output)
    }
}
