//! 运行时适配（v0.2.9）：让环境不齐的用户也能用起来。
//!
//! 目标：不内置、不偷偷下、不污染系统——按需把「便携运行时」下载到数据根内，
//! 并在启动 dsh 时把其目录注入子进程 PATH。
//!
//! 已验证（2026-10-13）：dsh 的 dsh.cmd 采用「就近 node.exe → PATH 兜底」的解析逻辑，
//! 故只要把便携 Node 目录注入 dsh 子进程 PATH，即可在无系统 Node 的环境下运行 dsh。
//!
//! 本模块只负责「检测 / 下载 / 解压 / 路径解析 / PATH 计算」；
//! 交互（提示、进度、确认）由命令层 + 前端负责。
//!
//! D12 技术债拆分（v0.3.0）：原 runtime.rs（533 行）按领域拆为两个子模块，
//! 经本模块重导出，**对外路径 crate::runtime::X 完全不变**：
//! - bundled：便携 Node / pnpm 的检测、下载、解压
//! - webview2：WebView2 运行时检测与安装引导
//!
//! 两者共用的 RuntimeProgress 与 download_to 保留在本模块。

mod bundled;
mod webview2;

pub use bundled::*;
pub use webview2::*;

#[cfg(windows)]
use std::path::Path;

/// 构建统一的 HTTP 客户端：环境变量代理优先；Windows 未设置代理环境变量时读系统代理。
///
/// reqwest 默认只读 `HTTP(S)_PROXY` / `ALL_PROXY`；Windows「Internet 选项」里的系统代理
/// 不会自动生效，而下载便携运行时 / WebView2 Bootstrapper / 更新包都可能有代理需求。
pub fn blocking_client(timeout: std::time::Duration) -> Result<reqwest::blocking::Client, String> {
    blocking_client_ua(timeout, None)
}

/// 同上，可选 User-Agent（GitHub API 必需）。
pub fn blocking_client_ua(
    timeout: std::time::Duration,
    user_agent: Option<&str>,
) -> Result<reqwest::blocking::Client, String> {
    #[allow(unused_mut)]
    let mut builder = reqwest::blocking::Client::builder().timeout(timeout);
    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }
    #[cfg(windows)]
    {
        let has_env_proxy = [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ]
        .iter()
        .any(|k| std::env::var_os(k).is_some());
        if !has_env_proxy {
            if let Some(url) = windows_system_proxy() {
                if let Ok(proxy) = reqwest::Proxy::all(&url) {
                    builder = builder.proxy(proxy);
                }
            }
        }
    }
    builder.build().map_err(|e| e.to_string())
}

#[cfg(windows)]
/// 解析 Windows `ProxyServer` 字符串。
///
/// 支持 `host:port` 与 `http=host:port;https=host:port` 两种形式；优先 https 项，
/// 缺少协议时补 `http://`。
fn parse_system_proxy(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut http: Option<String> = None;
    let mut https: Option<String> = None;
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('=') {
            Some((k, v)) if !v.trim().is_empty() => {
                let key = k.trim().to_ascii_lowercase();
                if key == "https" {
                    https = Some(v.trim().to_string());
                } else if key == "http" {
                    http = Some(v.trim().to_string());
                }
            }
            Some(_) => {}
            None => http = Some(part.to_string()),
        }
    }
    let host = https.or(http)?;
    if host.starts_with("http://") || host.starts_with("https://") {
        Some(host)
    } else {
        Some(format!("http://{host}"))
    }
}

/// 读取 Windows 系统代理（Internet 选项 → 局域网设置）。
#[cfg(windows)]
fn windows_system_proxy() -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe {
        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
        let enable_name = wide("ProxyEnable");
        let mut enable: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let rc = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            enable_name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut enable as *mut u32 as *mut _,
            &mut size,
        );
        if rc != 0 || enable == 0 {
            return None;
        }
        let server_name = wide("ProxyServer");
        let mut buf = [0u16; 2048];
        let mut size: u32 = (buf.len() * 2) as u32;
        let rc = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            server_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        );
        if rc != 0 {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        parse_system_proxy(&String::from_utf16_lossy(&buf[..len]))
    }
}

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

/// 用 reqwest blocking 流式下载 `url` 到 `dest`，边下边回调进度。
///
/// `stage` 是给用户看的阶段名；`on_progress` 每次收到数据块时调用一次。
/// 总大小取 Content-Length（可能缺失）。
#[cfg(windows)]
fn download_to(
    url: &str,
    dest: &Path,
    stage: &str,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<(), String> {
    use std::io::{Read, Write};

    let client = blocking_client(std::time::Duration::from_secs(300))?;
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;
    on_progress(RuntimeProgress {
        stage: stage.to_string(),
        done,
        total,
    });
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        on_progress(RuntimeProgress {
            stage: stage.to_string(),
            done,
            total,
        });
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::parse_system_proxy;

    #[test]
    fn parses_plain_host_port() {
        assert_eq!(
            parse_system_proxy("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn parses_scheme_map_prefers_https() {
        assert_eq!(
            parse_system_proxy("http=10.0.0.1:8080;https=10.0.0.1:8443").as_deref(),
            Some("http://10.0.0.1:8443")
        );
    }

    #[test]
    fn keeps_existing_scheme_and_ignores_empty() {
        assert_eq!(
            parse_system_proxy("https://proxy.example:443").as_deref(),
            Some("https://proxy.example:443")
        );
        assert_eq!(parse_system_proxy("   "), None);
    }
}
