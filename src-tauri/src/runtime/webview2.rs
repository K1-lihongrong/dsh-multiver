//! WebView2 运行时检测与安装引导（D12 拆分：原 runtime.rs 的 WebView2 部分）。
// download_to 只被 Windows 专属的 install_webview2_bootstrapper 使用，
// 非 Windows 平台上必须门控，否则 Linux 编译报 unused import。
#[cfg(windows)]
use super::download_to;
/// 检测系统是否安装了 WebView2 Runtime（Windows 专用）。
///
/// 读注册表 `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-...}`
/// 的 `pv` 值（微软官方推荐方式）。非 Windows 平台恒返回 true。
///
/// 必须在 Tauri 初始化**之前**调用（GUI 依赖 WebView2）。
#[cfg(windows)]
pub fn webview2_installed() -> bool {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };
    // WebView2 Runtime 的固定 GUID（微软文档）
    const GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    // 依次尝试两个位置：WOW6432Node（64 位系统上的 32 位视图）与常规路径
    let subkeys = [
        format!(
            "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{}",
            GUID
        ),
        format!("SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{}", GUID),
    ];
    for sub in &subkeys {
        let wide: Vec<u16> = sub.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey: HKEY = std::ptr::null_mut();
        let rc =
            unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide.as_ptr(), 0, KEY_READ, &mut hkey) };
        if rc != 0 {
            continue;
        }
        // 查询 pv（版本号）；存在且非空即视为已安装
        let val_name: Vec<u16> = "pv".encode_utf16().chain(std::iter::once(0)).collect();
        let mut buf = [0u8; 512];
        let mut len: u32 = buf.len() as u32;
        let mut ty: u32 = 0;
        let qrc = unsafe {
            RegQueryValueExW(
                hkey,
                val_name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                buf.as_mut_ptr(),
                &mut len,
            )
        };
        unsafe { RegCloseKey(hkey) };
        if qrc == 0 && len > 2 {
            // len 含结尾的 NUL（UTF-16，按字节计）
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
pub fn webview2_installed() -> bool {
    true
}

/// WebView2 缺失时的处理：弹 Win32 原生框（不依赖 WebView2）。
///
/// 返回 true = 继续启动 GUI（用户选择安装或忽略）；false = 退出。
/// 选项：[自动下载并安装] → 下 Bootstrapper 并拉起；[打开下载页] → 打开微软页；关窗 → 退出。
#[cfg(windows)]
pub fn prompt_webview2_missing() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDNO, IDYES, MB_ICONWARNING, MB_YESNO,
    };
    let text = "本工具需要 Microsoft Edge WebView2 运行时才能显示界面，但系统未安装。\n\n                选择「是」自动下载并安装（约 2MB，需联网）；\n                选择「否」打开微软官方下载页手动安装。\n\n                安装完成后请重新打开本工具。";
    let caption = "DSH 版本管理器 - 缺少 WebView2 运行时";
    let wtext: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let wcap: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
    let choice = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wtext.as_ptr(),
            wcap.as_ptr(),
            MB_YESNO | MB_ICONWARNING,
        )
    };
    if choice == IDYES {
        // 自动下载并安装（Bootstrapper）
        if let Err(e) = install_webview2_bootstrapper() {
            // 安装失败 → 退而打开下载页
            let msg = format!("自动安装失败：{}\n\n将打开微软官方下载页。", e);
            let wm: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    wm.as_ptr(),
                    wcap.as_ptr(),
                    MB_ICONWARNING,
                );
            }
            open_webview2_download_page();
        }
    } else if choice == IDNO {
        open_webview2_download_page();
    }
    // 无论哪种选择，都不继续（WebView2 缺失时 GUI 起不来，需用户装完重开）
    false
}

#[cfg(not(windows))]
pub fn prompt_webview2_missing() -> bool {
    true
}

/// 下载并拉起 WebView2 Evergreen Bootstrapper（Windows）。
#[cfg(windows)]
fn install_webview2_bootstrapper() -> Result<(), String> {
    const URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    let tmp = std::env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    download_to(URL, &tmp, "正在下载 WebView2 安装程序", &|_| {})?;
    // 拉起安装程序（等待用户完成）
    let status = std::process::Command::new(&tmp)
        .status()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    if status.success() {
        Ok(())
    } else {
        Err(format!("安装程序退出码 {:?}", status.code()))
    }
}

/// 打开 WebView2 官方下载页。
#[cfg(windows)]
fn open_webview2_download_page() {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let url = "https://developer.microsoft.com/microsoft-edge/webview2/";
    let op: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let u: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            u.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}
