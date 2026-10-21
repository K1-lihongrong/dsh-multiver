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

use std::path::Path;

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
