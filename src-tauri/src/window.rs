//! 窗口 label 与平台（Windows 任务栏）辅助。
//!
//! 从 lib.rs 抽出（GAP-003，纯移动，无逻辑改动）。

/// 由版本号生成合法的窗口 label 前缀（不含代次）。
/// Tauri 要求 label 只含字母数字和 `-` `/` `:` `_`，而版本号含 `.`，故替换为 `_`。
pub(crate) fn window_label_prefix(version: &str) -> String {
    let sanitized: String = version
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') { c } else { '_' })
        .collect();
    format!("dsh-{}", sanitized)
}

/// 带代次的**唯一**窗口 label。
/// 用唯一 label 从根本上避免 "a webview with label ... already exists"：
/// close()/destroy() 都是异步投递，不能保证旧窗口立即消失，故不再复用 label。
pub(crate) fn window_label_gen(version: &str, gen: u64) -> String {
    format!("{}-{}", window_label_prefix(version), gen)
}

/// 从命令行参数解析出 --launch-version <版本>（精简启动模式用）
pub(crate) fn parse_launch_version(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--launch-version" {
            return it.next().cloned();
        }
    }
    None
}

/// 显式设置进程 AppUserModelID，稳定 Windows 任务栏图标。
///
/// 不设置时，Windows 会尝试从"与 exe 关联的快捷方式"推断分组：
/// 一旦为某版本创建桌面快捷方式，任务栏重新分组就会取不到图标（图标丢失），
/// 重启程序才暂时恢复。显式固定 AUMID（用 app identifier）后分组稳定。
///
/// 注意：设了 AUMID 后，任务栏会改为从注册表
/// HKCU\Software\Classes\AppUserModelId\<AUMID> 的 IconUri 读取图标，
/// 因此这里一并把 IconUri 注册为当前 exe，否则任务栏会显示占位图。
/// 必须在任何窗口显示之前调用。
#[cfg(windows)]
pub(crate) fn set_windows_app_user_model_id(app: &tauri::AppHandle) {
    let app_id = app.config().identifier.clone();
    let wide: Vec<u16> = app_id.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }

    // 为 AUMID 注册任务栏图标：IconUri 指向当前 exe。
    if let Ok(exe) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let key = format!(
            "HKCU\\Software\\Classes\\AppUserModelId\\{}",
            app_id
        );
        let _ = std::process::Command::new("reg")
            .args([
                "add",
                &key,
                "/v",
                "IconUri",
                "/t",
                "REG_SZ",
                "/d",
                &exe.to_string_lossy(),
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

#[cfg(not(windows))]
pub(crate) fn set_windows_app_user_model_id(_app: &tauri::AppHandle) {}
