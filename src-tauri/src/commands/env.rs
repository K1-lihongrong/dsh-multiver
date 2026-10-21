//! 环境检查与运行时命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) fn check_env(app: tauri::AppHandle, full: Option<bool>) -> Vec<envcheck::CheckItem> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let _ = dirs.ensure();
    // v0.2.13：full=false（启动首屏）跳过 npm 源连通（网络）检查，避免拖慢启动；
    // 用户点「重新检查」/安装前传 full=true 做完整检查。
    envcheck::run_all(&dirs.root, full.unwrap_or(true))
}

/// 运行时状态：系统/便携的 Node、pnpm 是否就绪（供前端判断是否显示"自动准备"按钮）。
#[tauri::command]
pub(crate) fn runtime_status(app: tauri::AppHandle) -> runtime::RuntimeStatus {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    runtime::RuntimeStatus {
        system_node_ok: runtime::system_node_ok(),
        bundled_node_ok: runtime::bundled_node_ok(&root),
        system_pnpm_ok: runtime::system_pnpm_ok(),
        bundled_pnpm_ok: runtime::bundled_pnpm_ok(&root),
    }
}

/// 准备便携运行时（下载缺失的 Node/pnpm）。
///
/// 阻塞式下载放在后台线程池，进度经 "runtime-progress" 事件推给前端（GUI 进度条）。
/// 返回结果消息（成功/已就绪）。
#[tauri::command]
pub(crate) async fn setup_runtime(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let root = cfg.resolve_root(&mdir);
    let app2 = app.clone();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>, String> {
        let mut msgs: Vec<String> = Vec::new();
        let on_prog = move |ev: runtime::RuntimeProgress| {
            let _ = app2.emit("runtime-progress", ev);
        };
        if !runtime::system_node_ok() && !runtime::bundled_node_ok(&root) {
            runtime::install_bundled_node(&root, &on_prog)
                .map_err(|e| format!("下载便携 Node 失败：{}", e))?;
            msgs.push("已安装便携 Node".to_string());
        }
        if !runtime::system_pnpm_ok() && !runtime::bundled_pnpm_ok(&root) {
            runtime::install_bundled_pnpm(&root, &on_prog)
                .map_err(|e| format!("下载便携 pnpm 失败：{}", e))?;
            msgs.push("已安装便携 pnpm".to_string());
        }
        Ok(msgs)
    })
    .await
    .map_err(|e| format!("运行时准备任务失败：{}", e))??;

    if result.is_empty() {
        Ok("运行时已就绪，无需下载".to_string())
    } else {
        Ok(result.join("；"))
    }
}

/// 用户忽略「缺少运行时」的启动提示（v0.2.9）：记住选择，不再自动弹横幅。
/// 用户仍可从环境检查面板手动准备运行时。
#[tauri::command]
pub(crate) fn dismiss_runtime_prompt(app: tauri::AppHandle) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.runtime_prompt_dismissed = true;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败: {}", e))?;
    Ok("已忽略运行时提示".to_string())
}
