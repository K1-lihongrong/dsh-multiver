//! 运行时适配（v0.2.9）：让环境不齐的用户也能用起来。
//!
//! 目标：不内置、不偷偷下、不污染系统——按需把「便携运行时」下载到数据根内，
//! 并在启动 dsh 时把其目录注入子进程 PATH。
//!
//! 已验证（2026-10-13）：dsh 的 `dsh.cmd` 采用「就近 node.exe → PATH 兜底」的解析逻辑，
//! 故只要把便携 Node 目录注入 dsh 子进程 PATH，即可在无系统 Node 的环境下运行 dsh。
//!
//! 本模块只负责「检测 / 下载 / 解压 / 路径解析 / PATH 计算」；
//! 交互（提示、进度、确认）由命令层 + 前端负责。

use std::path::{Path, PathBuf};

/// 便携运行时依赖的 Node 版本（与 envcheck 的下限一致）。
pub const NODE_VERSION: &str = "22.19.0";

/// 运行时准备进度事件（v0.2.9 进度条）。
///
/// stage 是面向用户的阶段文案（如"正在下载 Node"）；done/total 是该阶段的
/// 已下载字节数与总字节数（total 为 None 表示服务器未给 Content-Length）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeProgress {
    pub stage: String,
    pub done: u64,
    pub total: Option<u64>,
}

/// 便携 Node 目录：`<root>/tools/node`
pub fn node_dir(root: &Path) -> PathBuf {
    root.join("tools").join("node")
}

/// 便携 pnpm 目录：`<root>/tools/pnpm`
pub fn pnpm_dir(root: &Path) -> PathBuf {
    root.join("tools").join("pnpm")
}

/// Windows 下便携 Node 的可执行文件名；其余平台为 `node`。
#[cfg(windows)]
pub fn node_exe_name() -> &'static str {
    "node.exe"
}
#[cfg(not(windows))]
pub fn node_exe_name() -> &'static str {
    "node"
}

/// 便携 Node 是否已就绪（存在 node 可执行文件）。
pub fn bundled_node_ok(root: &Path) -> bool {
    node_dir(root).join(node_exe_name()).is_file()
}

/// 便携 pnpm 是否已就绪。
pub fn bundled_pnpm_ok(root: &Path) -> bool {
    let d = pnpm_dir(root);
    #[cfg(windows)]
    {
        d.join("pnpm.cmd").is_file() || d.join("pnpm.exe").is_file()
    }
    #[cfg(not(windows))]
    {
        d.join("pnpm").is_file()
    }
}

/// 运行时状态：当前环境缺什么。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RuntimeStatus {
    /// 系统 Node 是否满足 dsh 要求
    pub system_node_ok: bool,
    /// 便携 Node 是否已就绪
    pub bundled_node_ok: bool,
    /// 系统 pnpm 是否可用
    pub system_pnpm_ok: bool,
    /// 便携 pnpm 是否已就绪
    pub bundled_pnpm_ok: bool,
}

/// 检测运行时应走系统还是便携：返回该注入 dsh 子进程 PATH 的额外目录列表。
///
/// 规则（与规划一致）：
/// 1. 系统 Node 满足 → 用系统的（不注入便携 node；便携存在仍保留作兜底）
/// 2. 系统 Node 不满足 → 注入便携 Node 目录（若有）
/// pnpm 同理。
pub fn runtime_path_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if !system_node_ok() && bundled_node_ok(root) {
        dirs.push(node_dir(root));
    }
    if !system_pnpm_ok() && bundled_pnpm_ok(root) {
        dirs.push(pnpm_dir(root));
    }
    dirs
}

/// 计算最终要注入子进程的环境变量列表（形如 [("PATH", "旧值;新目录")]）。
/// 无注入需求时返回 None。
pub fn path_env_additions(root: &Path) -> Option<String> {
    let dirs = runtime_path_dirs(root);
    if dirs.is_empty() {
        return None;
    }
    let cur = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = dirs
        .iter()
        .map(|d| d.to_string_lossy().to_string())
        .collect();
    if !cur.is_empty() {
        parts.push(cur);
    }
    Some(parts.join(if cfg!(windows) { ";" } else { ":" }))
}

// ───────────────────────── 系统运行时探测 ─────────────────────────

/// 系统 Node 是否满足 dsh 要求（复用 envcheck 的纯系统判断）。
pub fn system_node_ok() -> bool {
    crate::envcheck::system_node_ok()
}

/// 系统 pnpm 是否可用。
pub fn system_pnpm_ok() -> bool {
    crate::envcheck::system_pnpm_ok()
}

// ───────────────────────── 下载 ─────────────────────────

/// 下载 Node 官方 zip 到临时文件（返回临时文件路径）。
///
/// 源顺序：官方 → 淘宝镜像（npmmirror）。任一成功即返回。
/// 这是「失败换源」在运行时下载里的复用。
pub fn download_node_zip(
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let file = format!("node-v{}-win-x64.zip", NODE_VERSION);
    let urls = [
        format!("https://nodejs.org/dist/v{}/{}", NODE_VERSION, file),
        format!(
            "https://npmmirror.com/mirrors/node/v{}/{}",
            NODE_VERSION, file
        ),
    ];
    let tmp = std::env::temp_dir().join(format!(
        "dsh-multiver-node-{}.zip",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut last_err = String::from("未尝试任何下载源");
    for url in &urls {
        match download_to(url, &tmp, "正在下载 Node", on_progress) {
            Ok(_) => return Ok(tmp),
            Err(e) => last_err = format!("{} → {}", url, e),
        }
    }
    Err(format!("下载 Node 失败：{}", last_err))
}

/// 用 reqwest blocking 流式下载 `url` 到 `dest`，边下边回调进度。
///
/// `stage` 是给用户看的阶段名；`on_progress` 每次收到数据块时调用一次。
/// 总大小取 Content-Length（可能缺失）。
fn download_to(
    url: &str,
    dest: &Path,
    stage: &str,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<(), String> {
    use std::io::{Read, Write};

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;
    on_progress(RuntimeProgress { stage: stage.to_string(), done, total });
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        on_progress(RuntimeProgress { stage: stage.to_string(), done, total });
    }
    Ok(())
}

/// 解压 Node zip 到 `<root>/tools/node/`，把 zip 内的顶层目录（`node-vX-win-x64/`）内容
/// 平铺到 node 目录。
pub fn extract_node_zip(zip_path: &Path, root: &Path) -> Result<(), String> {
    let target = node_dir(root);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // 跳过顶层目录名，只保留其下的相对路径
        let rel = match name.split_once('/') {
            Some((_, rest)) => rest,
            None => continue,
        };
        if rel.is_empty() {
            continue;
        }
        let out = target.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 完整流程：下载 + 解压便携 Node。返回 node 目录。
pub fn install_bundled_node(
    root: &Path,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let zip = download_node_zip(on_progress)?;
    on_progress(RuntimeProgress { stage: "正在解压 Node".to_string(), done: 0, total: None });
    let result = extract_node_zip(&zip, root);
    let _ = std::fs::remove_file(&zip);
    result?;
    Ok(node_dir(root))
}

// ───────────────────────── 便携 pnpm ─────────────────────────

/// 便携 pnpm 版本（@pnpm/exe 的稳定版）。
pub const PNPM_VERSION: &str = "12.11.2";

/// 下载 pnpm 独立版（@pnpm/exe.win32-x64 的 tgz）到临时文件。
///
/// 源顺序：官方 registry → npmmirror。任一成功即返回。
pub fn download_pnpm_tgz(
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let rel = format!("@pnpm/exe.win32-x64/-/exe.win32-x64-{}.tgz", PNPM_VERSION);
    let urls = [
        format!("https://registry.npmjs.org/{}", rel),
        format!("https://registry.npmmirror.com/{}", rel),
    ];
    let tmp = std::env::temp_dir().join(format!(
        "dsh-multiver-pnpm-{}.tgz",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut last_err = String::from("未尝试任何下载源");
    for url in &urls {
        match download_to(url, &tmp, "正在下载 pnpm", on_progress) {
            Ok(_) => return Ok(tmp),
            Err(e) => last_err = format!("{} → {}", url, e),
        }
    }
    Err(format!("下载 pnpm 失败：{}", last_err))
}

/// 解压 pnpm tgz 到 `<root>/tools/pnpm/`，取出 `package/pnpm.exe`（及其余文件）。
pub fn extract_pnpm_tgz(tgz_path: &Path, root: &Path) -> Result<(), String> {
    let target = pnpm_dir(root);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(tgz_path).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        // 取 package/ 下的相对路径
        let rel = match path.components().next() {
            Some(std::path::Component::Normal(first)) if first == "package" => {
                path.strip_prefix("package").unwrap_or(&path)
            }
            _ => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = target.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry.unpack(&out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 完整流程：下载 + 解压便携 pnpm。返回 pnpm 目录。
pub fn install_bundled_pnpm(
    root: &Path,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let tgz = download_pnpm_tgz(on_progress)?;
    on_progress(RuntimeProgress { stage: "正在解压 pnpm".to_string(), done: 0, total: None });
    let result = extract_pnpm_tgz(&tgz, root);
    let _ = std::fs::remove_file(&tgz);
    result?;
    Ok(pnpm_dir(root))
}

// ───────────────────────── WebView2（批 2）─────────────────────────

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
        let rc = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                wide.as_ptr(),
                0,
                KEY_READ,
                &mut hkey,
            )
        };
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
        MessageBoxW, IDYES, IDNO, MB_ICONWARNING, MB_YESNO,
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
                MessageBoxW(std::ptr::null_mut(), wm.as_ptr(), wcap.as_ptr(), MB_ICONWARNING);
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
    const URL: &str =
        "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-runtime-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn paths_under_root() {
        let root = PathBuf::from("C:\\data");
        assert!(node_dir(&root).ends_with("tools\\node") || node_dir(&root).ends_with("tools/node"));
        assert!(pnpm_dir(&root).ends_with("tools\\pnpm") || pnpm_dir(&root).ends_with("tools/pnpm"));
    }

    #[test]
    fn bundled_node_ok_detects_exe() {
        let root = tmp_root("node-ok");
        assert!(!bundled_node_ok(&root));
        let dir = node_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(node_exe_name()), b"x").unwrap();
        assert!(bundled_node_ok(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_env_none_when_system_ok() {
        // 若本机系统 node/pnpm 都满足，则无需注入（测试环境相关，仅在满足时断言）
        if system_node_ok() && system_pnpm_ok() {
            assert!(path_env_additions(&PathBuf::from("C:\\nonexistent")).is_none());
        }
    }
}
