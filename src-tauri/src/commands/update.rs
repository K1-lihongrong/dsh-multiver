//! 自动更新命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) async fn check_update() -> crate::updater::UpdateInfo {
    tauri::async_runtime::spawn_blocking(crate::updater::check)
        .await
        .unwrap_or_else(|_| crate::updater::UpdateInfo {
            has_update: false,
            current: cli::VERSION.to_string(),
            latest: None,
            release_url: None,
            download_url: None,
            sha256: None,
        })
}

/// 请求管理器退出（供一键更新用：替换批处理需主进程退出后才能覆盖 exe）。
#[tauri::command]
pub(crate) fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 执行一键更新（C2，v0.2.10）：下载新 exe → 校验 → 生成替换批处理 → 返回；
/// 调用方收到成功后应**退出管理器**，由批处理完成替换并重启。
///
/// 返回成功消息（含新版本号）。失败时返回 Err（不触碰原 exe）。
#[tauri::command]
pub(crate) async fn perform_update(
    app: tauri::AppHandle,
    url: String,
    sha256: Option<String>,
) -> Result<String, String> {
    let target = std::env::current_exe().map_err(|e| format!("无法定位当前程序：{}", e))?;
    // 下载 + 校验（放后台线程，避免阻塞）
    let new_exe = tauri::async_runtime::spawn_blocking(move || {
        crate::updater::download_and_verify(&url, sha256.as_deref())
    })
    .await
    .map_err(|e| format!("下载任务失败：{}", e))??;

    // 生成替换批处理并启动
    crate::updater::spawn_replacer(&new_exe, &target)?;
    // 记录到维护日志
    let mdir = manager_dir(&app);
    let root = Config::load(&mdir).resolve_root(&mdir);
    logging::write_line(
        &root.join("logs"),
        "maintenance.log",
        "自动更新：已下载并启动替换程序",
    );
    Ok("更新已就绪，管理器将退出并自动重启为新版本".to_string())
}
