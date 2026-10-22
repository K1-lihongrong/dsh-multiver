//! 应用状态查询与隔离开关命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

/// 「路径设置」里额外展示的目录项（默认收起，点"展开"才显示）。
#[derive(Serialize)]
pub(crate) struct ExtraDir {
    /// 目录名（同时是 open_dir 的 which 值）
    name: String,
    /// 中文说明
    desc: String,
    /// 绝对路径
    path: String,
}

/// 一次返回给前端的完整状态
#[derive(Serialize)]
pub(crate) struct AppState {
    /// 管理器自身版本号（来自 Cargo.toml）
    version: String,
    root_dir: String,
    versions_dir: String,
    home_dir: String,
    store_dir: String,
    cache_dir: String,
    state_dir: String,
    /// 额外目录（logs/webview/trash/assets），由前端"展开全部"时展示
    extra_dirs: Vec<ExtraDir>,
    default_version: Option<String>,
    manager_dir: String,
    broken_versions: Vec<String>,
    maintenance: config::MaintenanceConfig,
    /// 是否复用官方 dsh 配置目录（~/.dsh）
    use_official_dsh_home: bool,
    /// 官方 dsh 配置目录路径（用于界面展示）；无法确定主目录时为 None
    official_home_dir: Option<String>,
    /// 用户是否已忽略「缺少运行时」启动提示（v0.2.9）
    runtime_prompt_dismissed: bool,
    /// 数据根是否可写（D19）：`Dirs::ensure()` 是否成功。false 时前端显示不可关闭横幅
    root_writable: bool,
    /// 不可写时的具体错误（供横幅展示）；可写时为 None
    root_writable_error: Option<String>,
}

#[tauri::command]
pub(crate) fn get_state(app: tauri::AppHandle) -> AppState {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    // D19：记录 ensure 结果供前端横幅使用（不再静默忽略）
    let ensure_err = dirs.ensure().err().map(|e| e.to_string());
    let extra_dirs = vec![
        ExtraDir {
            name: "logs".into(),
            desc: "管理器与 dsh 的运行日志".into(),
            path: dirs.resolve("logs").to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "webview".into(),
            desc: "各版本的 WebView2 数据（删了会丢登录态）".into(),
            path: dirs.webview.to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "trash".into(),
            desc: "卸载回收站（后台自动清空）".into(),
            path: dirs.trash.to_string_lossy().to_string(),
        },
        ExtraDir {
            name: "assets".into(),
            desc: "内部资源（桌面快捷方式图标）".into(),
            path: dirs.assets.to_string_lossy().to_string(),
        },
    ];
    AppState {
        version: cli::VERSION.to_string(),
        root_dir: root.to_string_lossy().to_string(),
        versions_dir: dirs.versions.to_string_lossy().to_string(),
        home_dir: dirs.home.to_string_lossy().to_string(),
        store_dir: dirs.store.to_string_lossy().to_string(),
        cache_dir: dirs.cache.to_string_lossy().to_string(),
        state_dir: dirs.state.to_string_lossy().to_string(),
        extra_dirs,
        default_version: cfg.default_version.clone(),
        manager_dir: mdir.to_string_lossy().to_string(),
        broken_versions: cfg.broken_versions.clone(),
        maintenance: cfg.maintenance.clone(),
        use_official_dsh_home: cfg.use_official_dsh_home,
        official_home_dir: crate::paths::official_dsh_home()
            .map(|p| p.to_string_lossy().to_string()),
        runtime_prompt_dismissed: cfg.runtime_prompt_dismissed,
        root_writable: ensure_err.is_none(),
        root_writable_error: ensure_err,
    }
}

#[tauri::command]
pub(crate) fn set_isolated(
    app: tauri::AppHandle,
    version: String,
    isolated: bool,
) -> Result<String, String> {
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
    // 隔离状态变了：若改的是当前默认版本，重生成转发脚本（DSH_HOME 会变）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    if isolated {
        Ok(format!("{} 已开启数据隔离", version))
    } else {
        Ok(format!("{} 已关闭数据隔离", version))
    }
}
