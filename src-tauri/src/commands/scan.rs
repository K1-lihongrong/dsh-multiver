//! 维护与占用扫描命令（D6 拆分）：scan_versions / cancel_scan / scan_isolated_size / maintenance。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
#[specta::specta]
pub(crate) async fn scan_versions(
    app: tauri::AppHandle,
    ctx: tauri::State<'_, crate::AppCtx>,
    versions: Vec<String>,
) -> Result<(), String> {
    use tauri::Emitter;
    let dirs = ctx.dirs_snapshot();
    let root = dirs.root.clone();
    let versions_dir = dirs.versions.clone();
    let store = dirs.store.clone();
    let app2 = app.clone();

    // A2-5: one Job per scan, while keeping the legacy scan-size-progress events.
    let job_target = format!("{} versions", versions.len());
    let sink = ctx.job_sink(app.clone());
    let job_id = sink.create("scan", &job_target);
    let token = sink.token(&job_id).unwrap_or_default();

    // Keep the legacy global flag in sync for compatibility; the scan loop
    // itself uses the job's CancellationToken.
    let legacy_cancel = ctx.scan_cancel_handle();
    legacy_cancel.store(false, std::sync::atomic::Ordering::SeqCst);

    let job_for_scan = job_id.clone();
    let sink_for_scan = sink.clone();
    let scan = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        let names: Vec<String> = versions;
        let total = names.len();
        let store_mtime = sizecache::dir_mtime(&store);
        let mut cache = sizecache::load(&root);
        // D13: drop cache entries for versions that no longer exist.
        sizecache::prune(&mut cache, |v| versions_dir.join(v).exists());
        let mut cancelled = false;

        let cancel = {
            let token = token.clone();
            let legacy = legacy_cancel.clone();
            move || token.is_cancelled() || legacy.load(std::sync::atomic::Ordering::SeqCst)
        };

        for (i, n) in names.iter().enumerate() {
            // Cancellation check before each version.
            if cancel() {
                cancelled = true;
                break;
            }
            let idx = i + 1;
            let dir_mtime = sizecache::dir_mtime(&versions_dir.join(n));

            // Cache hit: emit one event per version, preserving the legacy 80ms cadence.
            if sizecache::is_valid(&cache, n, dir_mtime, store_mtime) {
                if let Some(info) = sizecache::get_info(&cache, n) {
                    let _ = app2.emit(
                        "scan-size-progress",
                        serde_json::json!({
                            "version": n,
                            "stage": "detail",
                            "index": idx,
                            "total": total,
                            "done": 0,
                            "fileTotal": 0,
                            "info": info,
                            "cached": true,
                        }),
                    );
                    sink_for_scan.progress(
                        &job_for_scan,
                        "detail",
                        idx as u64,
                        Some(total as u64),
                        Some(serde_json::json!({
                            "version": n,
                            "index": idx,
                            "cached": true,
                        })),
                    );
                    if total > 1 {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                }
                continue;
            }

            // First pass starts: legacy counting event plus the unified job event.
            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "counting",
                    "index": idx,
                    "total": total,
                    "done": 0,
                    "fileTotal": 0,
                    "info": serde_json::Value::Null,
                    "cached": false,
                }),
            );
            sink_for_scan.progress(
                &job_for_scan,
                "counting",
                i as u64,
                Some(total as u64),
                Some(serde_json::json!({
                    "version": n,
                    "index": idx,
                })),
            );

            let app_c = app2.clone();
            let vname_c = n.clone();
            let job_c = job_for_scan.clone();
            let sink_c = sink_for_scan.clone();
            let cb_count = move |done: u64, _tot: u64| {
                let _ = app_c.emit(
                    "scan-size-progress",
                    serde_json::json!({
                        "version": vname_c,
                        "stage": "counting",
                        "index": idx,
                        "total": total,
                        "done": done,
                        "fileTotal": 0,
                        "info": serde_json::Value::Null,
                        "cached": false,
                    }),
                );
                sink_c.progress(
                    &job_c,
                    "counting",
                    i as u64,
                    Some(total as u64),
                    Some(serde_json::json!({
                        "version": vname_c,
                        "index": idx,
                        "files": done,
                    })),
                );
            };
            let Some(fl) =
                versions::collect_files(&versions_dir.join(n), Some(&cb_count), Some(&cancel))
            else {
                cancelled = true;
                break;
            };
            let file_count = fl.files.len() as u64;

            let first = versions::SizeInfo {
                total: fl.total,
                shared_size: 0,
                shared_count: 0,
                exclusive_size: fl.total,
            };
            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "total",
                    "index": idx,
                    "total": total,
                    "done": 0,
                    "fileTotal": file_count,
                    "info": first,
                    "cached": false,
                }),
            );
            sink_for_scan.progress(
                &job_for_scan,
                "counting",
                i as u64,
                Some(total as u64),
                Some(serde_json::json!({
                    "version": n,
                    "index": idx,
                    "fileTotal": file_count,
                })),
            );

            let app3 = app2.clone();
            let vname = n.clone();
            let job_d = job_for_scan.clone();
            let sink_d = sink_for_scan.clone();
            let cb = move |done: u64, ftot: u64| {
                let _ = app3.emit(
                    "scan-size-progress",
                    serde_json::json!({
                        "version": vname,
                        "stage": "detail",
                        "index": idx,
                        "total": total,
                        "done": done,
                        "fileTotal": if ftot > 0 { ftot } else { file_count },
                        "info": serde_json::Value::Null,
                        "cached": false,
                    }),
                );
                sink_d.progress(
                    &job_d,
                    "detail",
                    i as u64,
                    Some(total as u64),
                    Some(serde_json::json!({
                        "version": vname,
                        "index": idx,
                        "files": done,
                        "fileTotal": if ftot > 0 { ftot } else { file_count },
                    })),
                );
            };
            let Some(info) =
                versions::detail_from_files(&fl.files, fl.total, Some(&cb), Some(&cancel))
            else {
                cancelled = true;
                break;
            };

            let _ = app2.emit(
                "scan-size-progress",
                serde_json::json!({
                    "version": n,
                    "stage": "detail",
                    "index": idx,
                    "total": total,
                    "done": file_count,
                    "fileTotal": file_count,
                    "info": info.clone(),
                    "cached": false,
                }),
            );
            sink_for_scan.progress(
                &job_for_scan,
                "detail",
                idx as u64,
                Some(total as u64),
                Some(serde_json::json!({
                    "version": n,
                    "index": idx,
                    "fileTotal": file_count,
                })),
            );
            sizecache::put(&mut cache, n, dir_mtime, store_mtime, info);
        }
        let _ = sizecache::save(&root, &cache);
        cancelled
    })
    .await;

    match scan {
        Ok(true) => {
            sink.cancel(&job_id, "scan cancelled", None);
            Ok(())
        }
        Ok(false) => {
            sink.finish(&job_id, "scan done", None);
            Ok(())
        }
        Err(e) => {
            let msg = format!("扫描失败: {}", e);
            sink.fail(&job_id, "scan_failed", &msg, None);
            Err(msg)
        }
    }
}

/// v0.2.13：请求取消正在进行的扫描。置位全局标志，扫描循环会在当前版本中断后停止。
#[tauri::command]
#[specta::specta]
pub(crate) fn cancel_scan(ctx: tauri::State<'_, crate::AppCtx>) -> Result<(), String> {
    ctx.scan_cancel()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(id) = ctx.jobs().latest_running_id("scan") {
        if let Some(token) = ctx.jobs().token(&id) {
            token.cancel();
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn scan_isolated_size(
    ctx: tauri::State<'_, crate::AppCtx>,
    version: String,
    instance: Option<String>,
) -> Result<u64, String> {
    let (cfg, dirs) = ctx.config_dirs();
    let vid = crate::domain::VersionId::new(&version)?;
    let explicit = match instance {
        Some(raw) => Some(crate::domain::InstanceId::new(raw)?),
        None => None,
    };
    let target = cfg.resolve_target(&vid, explicit.as_ref())?;
    let home = crate::paths::resolve_home_ref(&dirs, &target.home_ref);
    // 目录扫描可能很慢，放到后台线程
    let size = tauri::async_runtime::spawn_blocking(move || versions::dir_size(&home))
        .await
        .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(size)
}

/// 设置自动维护开关。
#[tauri::command]
#[specta::specta]
pub(crate) fn set_auto_maintenance(
    ctx: tauri::State<'_, crate::AppCtx>,
    enabled: bool,
) -> Result<(), String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, _dirs) = ctx.config_dirs();
    cfg.maintenance.auto_enabled = enabled;
    cfg.save(&mdir).map_err(|e| format!("保存配置失败: {}", e))
}

/// 手动执行维护。kind: "cleanup"（孤立 webview）| "prune"（依赖仓库）| "all"。
/// 返回人类可读的结果描述。
#[tauri::command]
#[specta::specta]
pub(crate) async fn run_maintenance(
    ctx: tauri::State<'_, crate::AppCtx>,
    kind: String,
) -> Result<String, String> {
    let mdir = ctx.manager_dir().clone();
    let (mut cfg, dirs) = ctx.config_dirs();
    let root = dirs.root.clone();
    let _ = dirs.ensure();
    let mdir2 = mdir.clone();
    let mounted = cfg.mounted_map();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();

        if kind == "cleanup" || kind == "all" {
            let removed =
                maintenance::cleanup_orphan_webviews(&dirs.versions, &dirs.webview, &mounted);
            cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
            cfg.maintenance.last_cleanup_count = removed.len() as u64;
            logging::log_maintenance(
                &root,
                &format!(
                    "[手动] 清理孤立 webview：{} 项 {:?}",
                    removed.len(),
                    removed
                ),
            );
            // 同时清空「回收站」残留（卸载时 rename 进来的，后台可能未删净）
            let trashed = maintenance::cleanup_trash(&dirs.trash);
            let mut s = format!("已清理孤立缓存 {} 项", removed.len());
            if trashed > 0 {
                s.push_str(&format!("；已清空回收站 {} 项", trashed));
            }
            parts.push(s);
        }

        if kind == "prune" || kind == "all" {
            match maintenance::run_store_prune(&dirs.store, &dirs.cache, &dirs.state) {
                Ok(()) => {
                    cfg.maintenance.last_prune_at = Some(maintenance::now_secs());
                    logging::log_maintenance(&root, "[手动] store prune 完成");
                    parts.push("已回收依赖仓库".to_string());
                }
                Err(e) => {
                    logging::log_maintenance(&root, &format!("[手动] store prune 失败: {}", e));
                    return Err(format!("回收依赖仓库失败：{}", e));
                }
            }
        }

        let _ = cfg.save(&mdir2);
        Ok(parts.join("；"))
    })
    .await
    .map_err(|e| format!("维护任务失败: {}", e))?;

    result
}
