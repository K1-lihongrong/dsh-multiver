//! 自动更新命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
#[specta::specta]
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
            error: Some("更新检查任务异常结束".to_string()),
        })
}

/// 请求管理器退出（供一键更新用：新 exe 已替换，退出后由延迟脚本拉起）。
#[tauri::command]
#[specta::specta]
pub(crate) fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 执行一键更新（C2，v0.2.10）：下载新 exe → 校验 → 进程内替换 → 安排重启。
/// 调用方收到成功后应退出管理器，由延迟脚本拉起新 exe。
///
/// 返回成功消息（含新版本号）。失败时返回 Err（不触碰原 exe）。
#[tauri::command]
#[specta::specta]
pub(crate) async fn perform_update(
    ctx: tauri::State<'_, crate::AppCtx>,
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

    // 先完成替换：失败会回滚，UI 能收到真实错误，不会出现退出后仍是旧版。
    crate::updater::install_replace(&new_exe, &target)?;
    // 再安排延迟重启：等当前进程退出、单实例锁释放后拉起新 exe。
    crate::updater::spawn_relaunch_after_exit(&target)?;
    // 记录到维护日志
    let (_cfg, dirs) = ctx.config_dirs();
    let root = dirs.root;
    logging::write_line(
        &root.join("logs"),
        "maintenance.log",
        "自动更新：新版本已替换，已安排重启",
    );
    Ok("更新已安装，管理器将退出并自动重启为新版本".to_string())
}
