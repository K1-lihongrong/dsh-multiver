//! 维护与占用扫描命令（D6 拆分）：scan_versions / cancel_scan / scan_isolated_size / maintenance。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) async fn scan_versions(
    app: tauri::AppHandle,
    versions: Vec<String>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let versions_dir = dirs.versions.clone();
    let store = dirs.store.clone();
    let app2 = app.clone();

    // 清除上次的取消标志
    SCAN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let names: Vec<String> = versions;
        let total = names.len();
        let store_mtime = sizecache::dir_mtime(&store);
        let mut cache = sizecache::load(&root);
        // D13：清掉已卸载版本留下的死条目，保证写回时只含当前实际安装的版本
        sizecache::prune(&mut cache, |v| versions_dir.join(v).exists());

        for (i, n) in names.iter().enumerate() {
            // 取消检查（每个版本开始前）
            if SCAN_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let idx = i + 1;
            let dir_mtime = sizecache::dir_mtime(&versions_dir.join(n));

            // 缓存命中：逐个 emit（间隔 80ms），让"复用"有节奏感
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
                    if total > 1 {
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                }
                continue;
            }

            // 第一段开始：发 counting 事件（前端显示不确定进度条）
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

            // 取消检查闭包
            let cancel = || SCAN_CANCEL.load(std::sync::atomic::Ordering::SeqCst);

            // 第一段（合并遍历）：收集文件列表 + 总量。带进度与取消。
            let app_c = app2.clone();
            let vname_c = n.clone();
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
            };
            let Some(fl) =
                versions::collect_files(&versions_dir.join(n), Some(&cb_count), Some(&cancel))
            else {
                break; // 被取消
            };
            let file_count = fl.files.len() as u64;

            // 第一段完成：发总量（stage="total"）
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

            // 第二段：用文件列表读 nlink（带进度与取消）
            let app3 = app2.clone();
            let vname = n.clone();
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
            };
            let Some(info) =
                versions::detail_from_files(&fl.files, fl.total, Some(&cb), Some(&cancel))
            else {
                break; // 被取消
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
            sizecache::put(&mut cache, n, dir_mtime, store_mtime, info);
        }
        let _ = sizecache::save(&root, &cache);
    })
    .await
    .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(())
}

/// v0.2.13：请求取消正在进行的扫描。置位全局标志，扫描循环会在当前版本中断后停止。
#[tauri::command]
pub(crate) fn cancel_scan() -> Result<(), String> {
    SCAN_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub(crate) async fn scan_isolated_size(
    app: tauri::AppHandle,
    version: String,
) -> Result<u64, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let home = versions::isolated_home(&dirs.versions, &version);
    // 目录扫描可能很慢，放到后台线程
    let size = tauri::async_runtime::spawn_blocking(move || versions::dir_size(&home))
        .await
        .map_err(|e| format!("扫描失败: {}", e))?;
    Ok(size)
}

/// 设置自动维护开关。
#[tauri::command]
pub(crate) fn set_auto_maintenance(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.maintenance.auto_enabled = enabled;
    cfg.save(&mdir).map_err(|e| format!("保存配置失败: {}", e))
}

/// 手动执行维护。kind: "cleanup"（孤立 webview）| "prune"（依赖仓库）| "all"。
/// 返回人类可读的结果描述。
#[tauri::command]
pub(crate) async fn run_maintenance(app: tauri::AppHandle, kind: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let dirs = Dirs::new(root.clone());
    let _ = dirs.ensure();
    let mdir2 = mdir.clone();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();

        if kind == "cleanup" || kind == "all" {
            let removed = maintenance::cleanup_orphan_webviews(&dirs.versions, &dirs.webview);
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
