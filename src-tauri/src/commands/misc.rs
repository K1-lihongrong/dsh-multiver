//! 杂项命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) fn log_frontend(app: tauri::AppHandle, msg: String) {
    let mdir = manager_dir(&app);
    let root = Config::load(&mdir).resolve_root(&mdir);
    // v0.2.13 启动打点：若前端首屏打点（[boot]），附加"进程启动→窗口就绪"耗时
    let msg = if msg.starts_with("[boot]") {
        if let Some(ms) = crate::WIN_READY_MS.get() {
            format!("{}(窗口就绪={}ms) ", msg, ms)
        } else {
            msg
        }
    } else {
        msg
    };
    logging::write_line(&root.join("logs"), "frontend.log", &msg);
}

#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只允许打开 http/https 链接".to_string());
    }
    let (ok, msg) = actions::open_url(&url);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}

#[tauri::command]
pub(crate) fn open_dir(app: tauri::AppHandle, which: String) -> Result<(), String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let target = dirs.resolve(&which);
    let (ok, msg) = actions::open_folder(&target);
    if ok {
        Ok(())
    } else {
        Err(msg)
    }
}
