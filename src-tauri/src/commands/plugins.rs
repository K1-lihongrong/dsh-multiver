//! dsh 插件管理命令（D6 拆分：原 commands.rs 的插件相关，双向桥 B1）。
#![allow(unused_imports)]
use super::*;

/// 按版本解析 dsh CLI 与 DSH_HOME（隔离 / 官方 / 共享）。
fn resolve_dsh(
    ctx: &crate::AppCtx,
    version: &str,
) -> Result<(crate::dsplugins::DshCli, std::path::PathBuf), String> {
    let (cfg, dirs) = ctx.config_dirs();
    let vdir = dirs.versions.join(version);
    if !vdir.join("node_modules").exists() {
        return Err(format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&cfg, &dirs, version);
    let cli = crate::dsplugins::dsh_cli(&vdir)
        .ok_or_else(|| format!("版本 {} 缺少 dsh 入口", version))?;
    let adapter = crate::dsh_adapter::adapter_for(version);
    let _ = adapter.generation();
    Ok((cli, home))
}

/// dsh 插件管理（双向桥 B1）：列某版本下的所有 profile 与插件。
///
/// DSH_HOME 用 resolve_home 解析（隔离 / 官方 / 共享）。
/// 同时带上 DSH 安装随附的官方组合包名（对齐官方「随 DSH 提供」语义）。
#[tauri::command]
#[specta::specta]
pub(crate) fn ds_list_plugins(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
) -> Result<crate::dsplugins::DsPluginList, String> {
    let (cfg, dirs) = ctx.config_dirs();
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
#[derive(serde::Serialize, specta::Type)]
pub(crate) struct DsProfileInfo {
    pub runtime_version: Option<String>,
    pub exemptions: serde_json::Value,
    pub pending_builds: Vec<String>,
    pub logs_dir: String,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn ds_profile_info(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
) -> Result<DsProfileInfo, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cfg, dirs) = ctx.config_dirs();
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
///
/// `rename_all = "snake_case"`：Tauri 2 默认把命令参数映射成 camelCase，
/// 而本命令的 `package_version` / `dsh_version` 是 snake_case，不加会报
/// `missing required key packageVersion`。
#[tauri::command(rename_all = "snake_case")]
#[specta::specta(rename_all = "snake_case")]
pub(crate) async fn ds_exemption_action(
    ctx: tauri::State<'_, crate::AppCtx>,
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
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;
    let adapter = crate::dsh_adapter::adapter_for(&version);
    if !adapter.supports_allow_version() {
        return Err("当前 DSH 版本不支持精确版本豁免".into());
    }
    let _ = adapter.profile_dir(&home, &profile);
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
#[specta::specta]
pub(crate) fn ds_approve_builds(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
    packages: Vec<String>,
) -> Result<Vec<String>, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cfg, dirs) = ctx.config_dirs();
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
#[specta::specta]
pub(crate) fn ds_open_plugin_logs(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
) -> Result<(), String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cfg, dirs) = ctx.config_dirs();
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

/// 安装前 inspect（B1-16，对齐官方 `pluginManager.inspect` 的可见行为）：
/// npm 包 → `pnpm view` 读名称/版本/描述/`dsh.bundle`；本地目录 → 读 package.json；
/// git URL → 只确认形式与主机（不抓取）。问题码：not-found / not-a-bundle / network。
#[derive(serde::Serialize, specta::Type)]
pub(crate) struct DsInspect {
    pub kind: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub declares_bundle: bool,
    pub problem: Option<String>,
    pub detail: Option<String>,
}

fn inspect_path_spec(spec: &str, profile_dir: &std::path::Path) -> Option<DsInspect> {
    let p = std::path::Path::new(spec);
    let pkg = if p.join("package.json").exists() {
        p.to_path_buf()
    } else {
        profile_dir.join(spec)
    };
    let text = std::fs::read_to_string(pkg.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(DsInspect {
        kind: "path".into(),
        name: v
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        declares_bundle: v.get("dsh").and_then(|d| d.get("bundle")).is_some(),
        problem: None,
        detail: None,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn ds_inspect_spec(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
    spec: String,
    registry: Option<String>,
) -> Result<DsInspect, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    crate::dsplugins::validate_install_spec(&spec)?;
    if let Some(url) = &registry {
        crate::dsplugins::validate_registry_url(url)?;
    }
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;
    let pdir = crate::dsplugins::profiles_dir(&home).join(&profile);
    let s = spec.trim().to_string();
    let lower = s.to_ascii_lowercase();
    let is_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git+")
        || lower.starts_with("github:")
        || lower.starts_with("file:")
        || lower.starts_with("link:");
    if is_url {
        let host = s
            .split_once("://")
            .map(|(_, r)| r.split('/').next().unwrap_or(r).to_string())
            .unwrap_or_else(|| s.clone());
        return Ok(DsInspect {
            kind: "git".into(),
            name: None,
            version: None,
            description: None,
            declares_bundle: false,
            problem: None,
            detail: Some(host),
        });
    }
    let is_path = lower.starts_with("./")
        || lower.starts_with("../")
        || lower.starts_with(".\\")
        || lower.starts_with("..\\")
        || lower.starts_with('/')
        || (s.len() > 2
            && s.as_bytes()[1] == b':'
            && (s.as_bytes()[2] == b'\\' || s.as_bytes()[2] == b'/'));
    if is_path {
        return match inspect_path_spec(&s, &pdir) {
            Some(mut info) => {
                if !info.declares_bundle {
                    info.problem = Some("not-a-bundle".into());
                }
                Ok(info)
            }
            None => Ok(DsInspect {
                kind: "path".into(),
                name: None,
                version: None,
                description: None,
                declares_bundle: false,
                problem: Some("not-found".into()),
                detail: Some(s),
            }),
        };
    }

    // npm 包：用官方 CLI 的 pnpm view（与安装使用同一注册表/代理/认证配置）
    let mut args: Vec<String> = vec![
        "view".into(),
        s.clone(),
        "name".into(),
        "version".into(),
        "description".into(),
        "dsh".into(),
        "--json".into(),
    ];
    if let Some(url) = &registry {
        args.push(format!("--registry={}", url.trim()));
    }
    let profile2 = profile.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let args_ref: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
            &home,
            &profile2,
            &args_ref,
            crate::dsplugins::PLUGIN_TIMEOUT,
            |_| {},
        )
    })
    .await
    .map_err(|e| format!("inspect 执行失败：{e}"))?;

    let value = serde_json::from_str::<serde_json::Value>(res.output.trim()).ok();
    let Some(v) = value else {
        let problem = if res.output.contains("404") || res.output.contains("not found") {
            "not-found"
        } else {
            "network"
        };
        return Ok(DsInspect {
            kind: "npm".into(),
            name: Some(s.clone()),
            version: None,
            description: None,
            declares_bundle: false,
            problem: Some(problem.into()),
            detail: Some(res.output.chars().take(500).collect()),
        });
    };
    let declares_bundle = v.get("dsh").and_then(|d| d.get("bundle")).is_some();
    Ok(DsInspect {
        kind: "npm".into(),
        name: v
            .get("name")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string())
            .or(Some(s.clone())),
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string()),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string()),
        declares_bundle,
        problem: if declares_bundle {
            None
        } else {
            Some("not-a-bundle".into())
        },
        detail: None,
    })
}

/// 安装源探测（B1-17，对齐官方「首次使用时探测 npm 官方源与镜像，选最快」）。
#[derive(serde::Serialize, specta::Type)]
pub(crate) struct DsRegistryProbe {
    pub kind: Option<String>,
    pub url: Option<String>,
    pub ms: Option<u64>,
}

type ProbeCacheEntry = (String, String, u64, std::time::Instant);

#[tauri::command]
#[specta::specta]
pub(crate) async fn ds_probe_registries() -> Result<DsRegistryProbe, String> {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    static CACHE: OnceLock<Mutex<Option<ProbeCacheEntry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((kind, url, ms, at)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(300) {
                return Ok(DsRegistryProbe {
                    kind: Some(kind.clone()),
                    url: Some(url.clone()),
                    ms: Some(*ms),
                });
            }
        }
    }
    let result = tauri::async_runtime::spawn_blocking(|| {
        let candidates = [
            ("official", "https://registry.npmjs.org/-/ping"),
            ("mirror", "https://registry.npmmirror.com/-/ping"),
        ];
        let mut best: Option<(String, String, u64)> = None;
        for (kind, url) in candidates {
            let started = Instant::now();
            let client = match crate::runtime::blocking_client(Duration::from_millis(1500)) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let ok = client
                .get(url)
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !ok {
                continue;
            }
            let ms = started.elapsed().as_millis() as u64;
            let base = url.trim_end_matches("/-/ping").to_string();
            if best.as_ref().map(|(_, _, b)| ms < *b).unwrap_or(true) {
                best = Some((kind.to_string(), base, ms));
            }
        }
        best
    })
    .await
    .map_err(|e| format!("探测安装源失败：{e}"))?;
    match result {
        Some((kind, url, ms)) => {
            if let Ok(mut guard) = cache.lock() {
                *guard = Some((kind.clone(), url.clone(), ms, Instant::now()));
            }
            Ok(DsRegistryProbe {
                kind: Some(kind),
                url: Some(url),
                ms: Some(ms),
            })
        }
        None => Ok(DsRegistryProbe {
            kind: None,
            url: None,
            ms: None,
        }),
    }
}

/// 插件操作结果（安装对话框「完成」页用：name 为新装/变更的依赖名；卸载为 None）。
#[derive(serde::Serialize, specta::Type)]
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
#[specta::specta]
pub(crate) async fn ds_plugin_action(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, crate::AppCtx>,
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
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;
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

    let job_id = Some(
        ctx.jobs()
            .create("plugin", &format!("{}/{}/{}", version, profile, action)),
    );
    let jobs = Some(ctx.jobs().clone());
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
                if let (Some(jid), Some(reg)) = (job_id.as_deref(), jobs.as_ref()) {
                    reg.progress(jid, "running", 0, None);
                    let _ = app2.emit(
                        "job:progress",
                        serde_json::json!({
                            "id": jid,
                            "kind": "plugin",
                            "target": format!("{}/{}/{}", version2, profile2, action),
                            "stage": "running",
                        }),
                    );
                }
            },
        );
        let outcome = if res.timed_out {
            Err(format!(
                "插件操作超时（{} 秒），已中止",
                crate::dsplugins::PLUGIN_TIMEOUT.as_secs()
            ))
        } else if res.ok {
            Ok(res.output)
        } else {
            Err(res.output)
        };
        if let (Some(jid), Some(reg)) = (job_id.as_deref(), jobs.as_ref()) {
            match &outcome {
                Ok(_) => {
                    reg.finish(jid, "plugin action done");
                    let _ = app2.emit(
                        "job:done",
                        serde_json::json!({
                            "id": jid,
                            "kind": "plugin",
                            "target": format!("{}/{}/{}", version2, profile2, action),
                        }),
                    );
                }
                Err(raw) => {
                    let f = crate::plugin_failure::classify(raw);
                    reg.fail(jid, f.code.as_str(), &f.message);
                    let _ = app2.emit(
                        "job:failed",
                        serde_json::json!({
                            "id": jid,
                            "kind": "plugin",
                            "target": format!("{}/{}/{}", version2, profile2, action),
                            "code": f.code,
                            "message": f.message,
                            "raw": f.raw,
                        }),
                    );
                }
            }
        }
        outcome
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
#[specta::specta]
pub(crate) async fn ds_init_profile(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
) -> Result<String, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;

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
#[specta::specta]
pub(crate) fn ds_open_profile_dir(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
) -> Result<(), String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (cfg, dirs) = ctx.config_dirs();
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

/// 把 pnpm/dsh 原始失败输出分类；`raw` 原样保留。
#[tauri::command]
#[specta::specta]
pub(crate) fn ds_classify_plugin_failure(output: String) -> crate::plugin_failure::PluginFailure {
    crate::plugin_failure::classify(&output)
}

/// 共享 home 只读体检：扫描 profiles/node_modules 的 junction，报告旧版本/悬空/重复来源。
#[tauri::command]
#[specta::specta]
pub(crate) fn ds_shared_home_health(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
) -> Result<crate::shared_home::SharedHomeHealth, String> {
    let (cfg, dirs) = ctx.config_dirs();
    let home = crate::paths::resolve_home(&cfg, &dirs, &version);
    let job = ctx.jobs().create("health", &version);
    ctx.jobs().progress(&job, "scanning", 0, None);
    let report = crate::shared_home::scan(&home, &version);
    ctx.jobs().finish(&job, "shared home health scanned");
    Ok(report)
}

/// 最近 Job 快照，供前端/诊断使用。
#[tauri::command]
#[specta::specta]
pub(crate) fn list_jobs(
    ctx: tauri::State<'_, crate::AppCtx>,
    limit: Option<usize>,
) -> Vec<crate::jobs::JobSnapshot> {
    ctx.jobs().list_recent(limit.unwrap_or(20))
}

/// 检查可更新插件（B1-13）：`dsh plugin --profile <p> outdated --json`。
///
/// `pnpm outdated` 在部分 hoisted / bundle 组合 profile 上会漏报范围内的新版本
///（例如 dsh-ops 0.5.1 到 0.5.2），因此再对非官方插件逐个 `view <pkg> version --json` 兜底。
#[tauri::command]
#[specta::specta]
pub(crate) async fn ds_check_updates(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
) -> Result<Vec<crate::dsplugins::DsUpdate>, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    let (_cfg, dirs) = ctx.config_dirs();
    let version_dir = dirs.versions.join(&version);
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;
    let profile2 = profile.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let res = crate::dsplugins::run_dsh_plugin(
            &cli.bin,
            cli.js.as_deref(),
            &home,
            &profile2,
            &["outdated", "--json"],
            std::time::Duration::from_secs(15),
            |_| {},
        );
        let outdated_timed_out = res.timed_out;
        let outdated_updates = if outdated_timed_out {
            None
        } else {
            crate::dsplugins::parse_outdated_output(&res.output)
        };
        let outdated_json_ok = outdated_updates.is_some();
        let mut updates = outdated_updates.unwrap_or_default();

        let official = crate::dsplugins::official_bundles(&version_dir);
        if let Some(profile_info) = crate::dsplugins::list_profiles(&home, &official)
            .into_iter()
            .find(|p| p.name == profile2)
        {
            let mut known: std::collections::HashSet<String> =
                updates.iter().map(|u| u.name.clone()).collect();
            let fallback_deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
            for plugin in profile_info
                .plugins
                .iter()
                .filter(|p| p.origin != "official")
            {
                if plugin.version.trim().is_empty() || !known.insert(plugin.name.clone()) {
                    continue;
                }
                let left = fallback_deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                let view = crate::dsplugins::run_dsh_plugin(
                    &cli.bin,
                    cli.js.as_deref(),
                    &home,
                    &profile2,
                    &["view", &plugin.name, "version", "--json"],
                    left.min(std::time::Duration::from_secs(10)),
                    |_| {},
                );
                if view.timed_out || !view.ok {
                    continue;
                }
                if let Some(latest) = crate::dsplugins::parse_view_version(&view.output) {
                    if crate::dsplugins::version_is_newer(&plugin.version, &latest) {
                        updates.push(crate::dsplugins::DsUpdate {
                            name: plugin.name.clone(),
                            current: plugin.version.clone(),
                            latest,
                        });
                    }
                }
            }
        }
        updates.sort_by(|a, b| a.name.cmp(&b.name));
        if !outdated_json_ok && updates.is_empty() {
            return Err(if outdated_timed_out {
                "检查更新超时".into()
            } else {
                res.output
            });
        }
        Ok(updates)
    })
    .await
    .map_err(|e| format!("检查更新失败：{e}"))?
}
/// 批量更新插件：逐项事务，add 失败时按原依赖 spec 回滚。
///
/// 只处理当前 version + profile 的可更新项；官方随附 bundle、缺失包、
/// 目标版本与当前版本相同等情况在预检阶段直接返回 Failed，不发生写入。
#[tauri::command]
#[specta::specta]
pub(crate) async fn ds_plugin_update_batch(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    profile: String,
    targets: Vec<crate::dsplugins::PluginUpdateTarget>,
) -> Result<Vec<crate::dsplugins::PluginUpdateResult>, String> {
    crate::dsplugins::validate_profile_name(&profile)?;
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let (_cfg, dirs) = ctx.config_dirs();
    let (cli, home) = resolve_dsh(ctx.inner(), &version)?;
    let version_dir = dirs.versions.join(&version);
    let official = crate::dsplugins::official_bundles(&version_dir);
    let profiles = crate::dsplugins::list_profiles(&home, &official);
    let Some(profile_info) = profiles.iter().find(|p| p.name == profile) else {
        return Err(format!("profile 不存在：{}", profile));
    };
    let pdir = crate::dsplugins::profiles_dir(&home).join(&profile);

    let mut seen = std::collections::HashSet::new();
    let mut plans: Vec<(usize, crate::dsplugins::PluginUpdatePlan)> = Vec::new();
    let mut results: Vec<Option<crate::dsplugins::PluginUpdateResult>> =
        (0..targets.len()).map(|_| None).collect();

    for (idx, target) in targets.iter().enumerate() {
        let fail = |msg: String| crate::dsplugins::PluginUpdateResult {
            name: target.name.clone(),
            current: String::new(),
            latest: target.latest.clone(),
            status: crate::dsplugins::PluginUpdateStatus::Failed,
            message: Some(msg),
            rollback_error: None,
        };
        if !seen.insert(target.name.clone()) {
            results[idx] = Some(fail(format!("批量更新包含重复插件：{}", target.name)));
            continue;
        }
        if target.name.trim().is_empty() || target.latest.trim().is_empty() {
            results[idx] = Some(fail("插件名或目标版本为空".into()));
            continue;
        }
        let Some(plugin) = profile_info.plugins.iter().find(|p| p.name == target.name) else {
            results[idx] = Some(fail(format!("当前 profile 中未找到插件：{}", target.name)));
            continue;
        };
        if plugin.official {
            results[idx] = Some(fail(format!(
                "官方随附 bundle 不参与批量更新：{}",
                target.name
            )));
            continue;
        }
        if plugin.version == target.latest {
            results[idx] = Some(fail(format!("插件已经是最新版本：{}", target.name)));
            continue;
        }
        let Some(original_spec) = crate::dsplugins::rollback_add_spec(
            &plugin.name,
            plugin.spec.as_deref(),
            &plugin.version,
        ) else {
            results[idx] = Some(fail(format!(
                "无法确定原依赖 spec，拒绝更新以避免无法回滚：{}",
                target.name
            )));
            continue;
        };
        if crate::dsplugins::validate_package_spec(&format!("{}@{}", target.name, target.latest))
            .is_err()
        {
            results[idx] = Some(fail(format!("目标版本格式非法：{}", target.latest)));
            continue;
        }
        plans.push((
            idx,
            crate::dsplugins::PluginUpdatePlan {
                name: plugin.name.clone(),
                current: plugin.version.clone(),
                latest: target.latest.clone(),
                original_spec,
            },
        ));
    }

    let total = plans.len();
    if total == 0 {
        return Ok(results.into_iter().map(|r| r.unwrap()).collect());
    }

    let job_id = Some(
        ctx.jobs()
            .create("update", &format!("{}/{}", version, profile)),
    );
    let jobs = Some(ctx.jobs().clone());
    let app2 = app.clone();
    let version2 = version.clone();
    let profile2 = profile.clone();
    let cli2 = cli;
    let home2 = home;
    let pdir2 = pdir;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        if let (Some(jid), Some(reg)) = (job_id.as_deref(), jobs.as_ref()) {
            reg.progress(jid, "batch", 0, Some(total as u64));
            let _ = app2.emit(
                "job:progress",
                serde_json::json!({
                    "id": jid,
                    "kind": "update",
                    "target": format!("{}/{}", version2, profile2),
                    "stage": "batch",
                    "done": 0,
                    "total": total,
                }),
            );
        }
        let mut out = results;
        for (pos, (idx, plan)) in plans.into_iter().enumerate() {
            let index = pos + 1;
            if let (Some(jid), Some(reg)) = (job_id.as_deref(), jobs.as_ref()) {
                reg.progress(jid, "update", pos as u64, Some(total as u64));
                let _ = app2.emit(
                    "job:progress",
                    serde_json::json!({
                        "id": jid,
                        "kind": "update",
                        "target": format!("{}/{}", version2, profile2),
                        "stage": "update",
                        "done": pos,
                        "total": total,
                        "name": plan.name,
                    }),
                );
            }
            let app3 = app2.clone();
            let version3 = version2.clone();
            let profile3 = profile2.clone();
            let name3 = plan.name.clone();
            let latest3 = plan.latest.clone();
            let current3 = plan.current.clone();
            let mut run = |step: crate::dsplugins::PluginUpdateStep, args: &[&str]| {
                let phase = match step {
                    crate::dsplugins::PluginUpdateStep::Remove => "removing",
                    crate::dsplugins::PluginUpdateStep::Install => "installing",
                    crate::dsplugins::PluginUpdateStep::Rollback => "rolling_back",
                };
                let _ = app3.emit(
                    "plugin-update-progress",
                    serde_json::json!({
                        "version": version3,
                        "profile": profile3,
                        "name": name3,
                        "current": current3,
                        "latest": latest3,
                        "index": index,
                        "total": total,
                        "phase": phase,
                    }),
                );
                let res = crate::dsplugins::run_dsh_plugin(
                    &cli2.bin,
                    cli2.js.as_deref(),
                    &home2,
                    &profile3,
                    args,
                    crate::dsplugins::PLUGIN_TIMEOUT,
                    |line| {
                        let _ = app3.emit(
                            "ds-plugin-progress",
                            serde_json::json!({
                                "version": version3,
                                "profile": profile3,
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
            };
            let verify_pdir = pdir2.clone();
            let verify_name = plan.name.clone();
            let verify = move || {
                if read_dep_map(&verify_pdir).contains_key(&verify_name) {
                    Ok(())
                } else {
                    Err(format!("依赖表中缺少 {}", verify_name))
                }
            };
            let result = crate::dsplugins::execute_plugin_update(&plan, &mut run, verify);
            let _ = app2.emit(
                "plugin-update-progress",
                serde_json::json!({
                    "version": version2,
                    "profile": profile2,
                    "name": result.name,
                    "index": index,
                    "total": total,
                    "phase": "done",
                    "status": result.status,
                    "message": result.message,
                }),
            );
            out[idx] = Some(result);
        }
        let failed = out
            .iter()
            .filter(|r| {
                !r.as_ref()
                    .is_some_and(|x| matches!(&x.status, crate::dsplugins::PluginUpdateStatus::Updated))
            })
            .count();
        if let (Some(jid), Some(reg)) = (job_id.as_deref(), jobs.as_ref()) {
            if failed == 0 {
                reg.finish(jid, "plugin update done");
                let _ = app2.emit("job:done", serde_json::json!({ "id": jid, "kind": "update" }));
            } else {
                reg.fail(jid, "partial_failure", "some plugin updates failed");
                let _ = app2.emit(
                    "job:failed",
                    serde_json::json!({ "id": jid, "kind": "update", "code": "partial_failure", "failed": failed }),
                );
            }
        }
        out
    })
    .await
    .map_err(|e| format!("批量更新任务失败：{e}"))?;

    Ok(outcome
        .into_iter()
        .map(|r| {
            r.unwrap_or_else(|| crate::dsplugins::PluginUpdateResult {
                name: String::new(),
                current: String::new(),
                latest: String::new(),
                status: crate::dsplugins::PluginUpdateStatus::Failed,
                message: Some("内部错误：缺少更新结果".into()),
                rollback_error: None,
            })
        })
        .collect())
}

/// 读本地推荐列表（磁盘缓存优先，缺失回退内置副本）。不联网，立即返回。
#[tauri::command]
#[specta::specta]
pub(crate) fn ds_plugin_catalog(
    ctx: tauri::State<'_, crate::AppCtx>,
) -> Result<crate::catalog::Catalog, String> {
    let (_cfg, dirs) = ctx.config_dirs();
    crate::catalog::load_local(&dirs.root)
}

/// 检查远端推荐列表是否有更新。有更新时写入磁盘缓存。
///
/// force=true 跳过 30 分钟节流窗口（用户手动点刷新）。
#[tauri::command]
#[specta::specta]
pub(crate) async fn ds_plugin_catalog_check(
    ctx: tauri::State<'_, crate::AppCtx>,
    force: Option<bool>,
) -> Result<crate::catalog::CheckResult, String> {
    let (_cfg, dirs) = ctx.config_dirs();
    let root = dirs.root;
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || crate::catalog::check_remote(&root, force))
        .await
        .map_err(|e| format!("检查推荐列表更新失败：{e}"))
}
