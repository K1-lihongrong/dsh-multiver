//! 版本管理命令（D6 拆分）：列表 / 远端 / 安装 / 卸载 / 默认版本。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) fn list_installed(app: tauri::AppHandle) -> Vec<versions::VersionInfo> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    versions::list(
        &dirs.versions,
        cfg.default_version.as_deref(),
        &cfg.isolated_versions,
    )
}

#[tauri::command]
pub(crate) fn list_remote(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    let (ok, list, err) = actions::list_remote(&dirs.store, &dirs.cache, &dirs.state);
    if ok {
        Ok(list)
    } else {
        Err(err)
    }
}

#[tauri::command]
pub(crate) async fn install_version(
    app: tauri::AppHandle,
    version: String,
    registry: Option<String>,
) -> Result<String, String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    // 启动参数 --debug-progress 时，把进度事件落盘到 <exe目录>/progress-debug.log
    // 供排查进度问题（默认关闭，不产生任何文件）。
    let debug_progress = std::env::args().any(|a| a == "--debug-progress");

    let app2 = app.clone();
    let version_for_record = version.clone();
    // 把阻塞的安装逻辑丢到后台线程池，避免占用主线程导致界面卡死
    let result = tauri::async_runtime::spawn_blocking(move || {
        let on_progress = move |ev: &versions::ProgressEvent| {
            if debug_progress {
                if let Ok(mdir) = std::env::current_exe() {
                    if let Some(dir) = mdir.parent() {
                        use std::io::Write;
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(dir.join("progress-debug.log"))
                        {
                            let _ = writeln!(
                                f,
                                "step={} total={} stage={:?} detail={:?} frac={:.2}",
                                ev.step, ev.total, ev.stage, ev.detail, ev.fraction
                            );
                        }
                    }
                }
            }
            let _ = app2.emit("install-progress", ev.clone());
        };
        versions::install(
            &dirs.versions,
            &dirs.store,
            &dirs.cache,
            &dirs.state,
            &version,
            &on_progress,
            registry.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("安装任务失败: {}", e))?;

    let (ok, msg) = result;
    // 维护 broken_versions：失败且分类为「私有包/不完整」时记入；成功时移除。
    let mut c = cfg.clone();
    let kind = if ok {
        ""
    } else {
        versions::classify_error(&msg)
    };
    if c.apply_install_result(&version_for_record, ok, kind) {
        let _ = c.save(&mdir);
    }
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

/// 清除「已知安装失败」标记（用户手动重试前可调用）。
#[tauri::command]
pub(crate) fn clear_broken(app: tauri::AppHandle, version: Option<String>) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    match version {
        Some(v) => cfg.broken_versions.retain(|x| x != &v),
        None => cfg.broken_versions.clear(),
    }
    cfg.save(&mdir).map_err(|e| format!("保存配置失败: {}", e))
}

#[tauri::command]
pub(crate) async fn uninstall_version(
    app: tauri::AppHandle,
    version: String,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    // 如果卸载的是默认版本，清空默认设置，并移除隔离标记
    let mut cfg = cfg;
    if cfg.remove_version_refs(&version) {
        let _ = cfg.save(&mdir);
    }
    // 卸载可能清掉了默认版本 / 隔离标记：重生成转发脚本（无默认版本时会删除 dsh.cmd）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    // 快速卸载：把版本目录 rename 到回收站（瞬时），实际删除交给后台线程。
    // WebView2 数据目录（<根>/webview/<版本>）同样 rename 到回收站（也瞬时）。
    // 这样用户点「卸载」后版本立即消失，不再盯着"卸载中..."等几十秒。
    let webview_dir = dirs.webview.join(&version);
    let trash = dirs.trash.clone();
    let versions_dir = dirs.versions.clone();
    let (ok, msg) = tauri::async_runtime::spawn_blocking(move || {
        let result = versions::uninstall_fast(&versions_dir, &trash, &version);
        // 版本目录已 rename 走后，webview 目录也 rename 进回收站（失败不影响卸载结果）
        if result.0 && webview_dir.exists() {
            let _ = std::fs::create_dir_all(&trash);
            let dest = trash.join(format!("webview-{}-{}", version, std::process::id()));
            if std::fs::rename(&webview_dir, &dest).is_ok() {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_dir_all(&dest);
                });
            } else {
                let _ = std::fs::remove_dir_all(&webview_dir);
            }
        }
        result
    })
    .await
    .map_err(|e| format!("卸载任务失败: {}", e))?;
    if ok {
        Ok(msg)
    } else {
        Err(msg)
    }
}

/// set_default 检测到"非管理器生成的同名 dsh 命令"时，返回此前缀的错误，
/// 前端据此弹出确认框，用户确认后再以 force=true 重调。
pub(crate) const CONFLICT_PREFIX: &str = "DSH_CONFLICT:";

#[tauri::command]
pub(crate) fn set_default(
    app: tauri::AppHandle,
    version: Option<String>,
    force: Option<bool>,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    if let Some(v) = &version {
        if !versions::exists(&dirs.versions, v) {
            return Err(format!("版本 {} 未安装", v));
        }
    }

    // GAP-014：设为默认（会写转发脚本）前，检测目标位置是否已有"非本管理器"的 dsh 命令。
    // 有且用户未确认（force != Some(true)）→ 返回带前缀的提示，让前端确认。
    if version.is_some() && force != Some(true) {
        let target = crate::paths::forward_script_dir(&mdir);
        if let Some(p) = actions::existing_foreign_dsh(&target) {
            return Err(format!("{}{}", CONFLICT_PREFIX, p.to_string_lossy()));
        }
    }

    cfg.default_version = version.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;

    // 生成/删除转发脚本（版本号、根目录、DSH_HOME 直接写进脚本）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    match &version {
        Some(v) => Ok(format!("默认版本已设为 {}，dsh 命令已就绪", v)),
        None => Ok("已清除默认版本".to_string()),
    }
}
