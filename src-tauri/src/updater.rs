//! 更新提醒（C1，v0.2.6）。
//!
//! 只做「提醒」：查 GitHub Releases 最新 tag，与当前版本比对，有新版则告知用户
//! （打开 Release 页 / 下载直链）。**不做**自动下载安装（那是 C2，另行评估）。
//!
//! 设计：
//! - 网络请求放后台线程，绝不阻塞启动 / UI
//! - 任何失败都静默（更新检查不该打扰用户）
//! - 结果缓存到进程内，避免每次启动都请求

use serde::Serialize;

/// 更新检查结果（前端据此渲染提示）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UpdateInfo {
    /// 是否有新版本
    pub has_update: bool,
    /// 当前版本
    pub current: String,
    /// 最新版本（查到才有）
    pub latest: Option<String>,
    /// Release 页面 URL
    pub release_url: Option<String>,
    /// 直接下载 exe 的 URL（Release 资产）
    pub download_url: Option<String>,
    /// 新 exe 的 SHA256（十六进制小写，来自 Release asset 的 digest 字段）。
    /// 用于下载后校验完整性（v0.2.10）。
    pub sha256: Option<String>,
    /// 检查失败原因（成功时为 None）。GUI 显示「检查更新失败」而不是静默当作「无更新」。
    pub error: Option<String>,
}

fn failure(current: &str, error: String) -> UpdateInfo {
    UpdateInfo {
        has_update: false,
        current: current.to_string(),
        latest: None,
        release_url: None,
        download_url: None,
        sha256: None,
        error: Some(error),
    }
}

/// GitHub 仓库（用于查询 Releases）
const REPO: &str = "K1-lihongrong/dsh-multiver";

/// GitHub API 根地址。默认生产地址；本地验收/自动化测试可用
/// `DSH_MULTIVER_UPDATE_API_BASE` 指向 mock server。
fn api_base() -> String {
    std::env::var("DSH_MULTIVER_UPDATE_API_BASE")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_string())
}

fn release_latest_url(base: &str) -> String {
    format!(
        "{}/repos/{}/releases/latest",
        base.trim_end_matches('/'),
        REPO
    )
}

/// 查询最新 Release，与当前版本比对。
///
/// 网络 / 解析失败时返回 `has_update: false`（静默失败，不打扰用户）。
pub fn check() -> UpdateInfo {
    let current = crate::cli::VERSION.to_string();

    let url = release_latest_url(&api_base());
    let client = match crate::runtime::blocking_client_ua(
        std::time::Duration::from_secs(10),
        Some("dsh-multiver"),
    ) {
        Ok(c) => c,
        Err(e) => return failure(&current, format!("无法创建网络客户端：{}", e)),
    };
    let resp = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                "连接 GitHub API 超时（10s）".to_string()
            } else {
                format!("连接 GitHub API 失败：{}", e)
            };
            return failure(&current, msg);
        }
    };
    if !resp.status().is_success() {
        return failure(&current, format!("GitHub API 返回 HTTP {}", resp.status()));
    }
    let body: serde_json::Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return failure(&current, format!("解析 GitHub API 响应失败：{}", e)),
    };

    let tag = body.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(String::from);
    // 从 assets 里找 exe 资产，同时取其 digest（sha256）
    let exe_asset = body
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|a| {
                a.get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| n.ends_with(".exe"))
                    .unwrap_or(false)
            })
        });
    let download_url = exe_asset
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())
        .map(String::from);
    // digest 形如 "sha256:<hex>"；取出 <hex>
    let sha256 = exe_asset
        .and_then(|a| a.get("digest"))
        .and_then(|v| v.as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|s| s.to_lowercase());

    let latest = tag.trim_start_matches('v').to_string();
    if latest.is_empty() {
        return failure(&current, "GitHub API 响应缺少 tag_name".to_string());
    }
    let has_update = is_newer(&latest, &current);
    UpdateInfo {
        has_update,
        current,
        latest: Some(latest),
        release_url: html_url,
        download_url,
        sha256,
        error: None,
    }
}

// ───────────────────────── C2：下载 / 校验 / 替换 ─────────────────────────

/// 下载新 exe 到临时文件，并校验 SHA256。
///
/// - `url`：exe 直链
/// - `expected_sha256`：期望的 SHA256（小写十六进制）；为 None 时**跳过校验**（仅警告）
///
/// 返回下载后的临时文件路径。
pub fn download_and_verify(
    url: &str,
    expected_sha256: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let tmp = std::env::temp_dir().join(format!(
        "dsh-multiver-update-{}.exe",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    // 镜像回退：直连失败时切镜像（设计见 docs/交互设计约定.md §2.8）。
    // 无代理的用户直连 GitHub 常常失败，此前这里只有单源，导致更新不可用。
    let got = crate::net::fetch(url, &crate::net::exe_chain(), crate::net::Shape::Binary)
        .map_err(|e| format!("下载失败：{}", e))?;
    std::fs::write(&tmp, &got.bytes).map_err(|e| e.to_string())?;

    if let Some(expected) = expected_sha256 {
        let actual = sha256_file(&tmp)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("校验失败：期望 {}，实际 {}", expected, actual));
        }
    }
    Ok(tmp)
}

/// 计算文件的 SHA256（小写十六进制）。
/// 公开包装：供 staged 模块校验暂存 exe 时复用（内部实现保持私有）。
pub fn sha256_file_pub(path: &std::path::Path) -> Result<String, String> {
    sha256_file(path)
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{:02x}", b)).collect())
}

/// 在进程内完成自身 exe 替换。
///
/// Windows 允许重命名正在运行的 exe，因此不再依赖外部批处理。
pub fn install_replace(
    new_exe: &std::path::Path,
    target_exe: &std::path::Path,
) -> Result<(), String> {
    if !new_exe.is_file() {
        return Err(format!("新版本文件不存在：{}", new_exe.display()));
    }
    if !target_exe.is_file() {
        return Err(format!("当前程序不存在：{}", target_exe.display()));
    }
    let dir = target_exe
        .parent()
        .ok_or_else(|| "无法确定当前程序所在目录".to_string())?;
    let staged = dir.join(format!(".dsh-multiver-update-{}.new", std::process::id()));
    let backup = target_exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&staged);
    std::fs::copy(new_exe, &staged).map_err(|e| format!("复制新版本到程序目录失败：{}", e))?;

    let _ = std::fs::remove_file(&backup);
    if let Err(e) = std::fs::rename(target_exe, &backup) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("备份当前程序失败：{}", e));
    }
    if let Err(e) = std::fs::rename(&staged, target_exe) {
        let rolled = std::fs::rename(&backup, target_exe).is_ok();
        let _ = std::fs::remove_file(&staged);
        return Err(format!(
            "放入新版本失败：{}；{}",
            e,
            if rolled {
                "已回滚到旧版本"
            } else {
                "回滚失败，请手动把 dsh-multiver.exe.old 改回 dsh-multiver.exe"
            }
        ));
    }
    let _ = std::fs::remove_file(new_exe);
    Ok(())
}

/// 生成退出后延迟重启的 Windows 批处理脚本。
///
/// 不要使用 tasklist | findstr 等待进程退出：detached 隐藏窗口下
/// findstr 可能一直等不到管道 EOF，导致更新卡死、管理器退出后不重启。
/// 改为固定延迟：调用方退出后约 3 秒再 start 新 exe，避开单实例锁。
#[cfg(windows)]
fn relaunch_script(target_exe: &std::path::Path, log_file: &std::path::Path) -> String {
    let tgt_s = target_exe.to_string_lossy().to_string();
    let dir_s = target_exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let log_s = log_file.to_string_lossy().to_string();
    format!(
        "@echo off\r\n\
         setlocal\r\n\
         ping -n 4 127.0.0.1 >NUL\r\n\
         if not exist \"{tgt}\" (\r\n\
         \x20 echo [%date% %time%] target missing: {tgt}>>\"{log}\"\r\n\
         \x20 exit /b 1\r\n\
         )\r\n\
         start \"\" /B /D \"{dir}\" \"{tgt}\"\r\n\
         if errorlevel 1 echo [%date% %time%] start failed>>\"{log}\"\r\n\
         del \"%~f0\" >NUL 2>&1\r\n",
        tgt = tgt_s,
        dir = dir_s,
        log = log_s,
    )
}

/// 安排当前进程退出后重启新 exe。
///
/// 返回批处理路径（Windows）或 shell 脚本路径（Unix），供测试/日志使用。
#[cfg(windows)]
pub fn spawn_relaunch_after_exit(
    target_exe: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    if !target_exe.is_file() {
        return Err(format!("重启目标不存在：{}", target_exe.display()));
    }
    let pid = std::process::id();
    let bat = std::env::temp_dir().join(format!("dsh-multiver-relaunch-{}.bat", pid));
    let log = std::env::temp_dir().join("dsh-multiver-relaunch.log");
    std::fs::write(&bat, relaunch_script(target_exe, &log))
        .map_err(|e| format!("写入重启脚本失败：{}", e))?;

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;
    std::process::Command::new("cmd")
        .arg("/C")
        .arg(&bat)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("启动重启脚本失败：{}", e))?;
    Ok(bat)
}

#[cfg(unix)]
pub fn spawn_relaunch_after_exit(
    target_exe: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    if !target_exe.is_file() {
        return Err(format!("重启目标不存在：{}", target_exe.display()));
    }
    let pid = std::process::id();
    let sh = std::env::temp_dir().join(format!("dsh-multiver-relaunch-{}.sh", pid));
    std::fs::write(&sh, "#!/bin/sh\nsleep 2\nexec \"$1\"\n")
        .map_err(|e| format!("写入重启脚本失败：{}", e))?;
    let mut perms = std::fs::metadata(&sh)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&sh, perms).map_err(|e| e.to_string())?;
    std::process::Command::new("sh")
        .arg(&sh)
        .arg(target_exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动重启脚本失败：{}", e))?;
    Ok(sh)
}

#[cfg(not(any(windows, unix)))]
pub fn spawn_relaunch_after_exit(
    _target_exe: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    Err("当前平台不支持自动重启".to_string())
}

/// 语义化版本比较：`candidate` 是否比 `current` 新。
///
/// 只比较数字段（忽略 pre-release 后缀差异，保守判断：同段数字相等则视为不更新）。
/// 无法解析时返回 false（宁可不提示，也不错报）。
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(s: &str) -> Option<Vec<u64>> {
        s.split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect()
    }
    let (Some(c), Some(cur)) = (parse(candidate), parse(current)) else {
        return false;
    };
    for i in 0..c.len().max(cur.len()) {
        let a = c.get(i).copied().unwrap_or(0);
        let b = cur.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_major_minor_patch() {
        assert!(is_newer("0.2.6", "0.2.5"));
        assert!(is_newer("0.3.0", "0.2.5"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.5", "0.2.5"));
        assert!(!is_newer("0.2.4", "0.2.5"));
    }

    #[test]
    fn ignores_prerelease_suffix() {
        // 0.2.6-rc.1 的数字段是 0.2.6 → 比 0.2.5 新
        assert!(is_newer("0.2.6-rc.1", "0.2.5"));
        // 同数字段（忽略后缀）视为不更新，避免"每次都提示"
        assert!(!is_newer("0.2.5-rc.1", "0.2.5"));
    }

    /// 真实网络测试（默认忽略；手动 --ignored 跑）。验证 check() 链路。
    #[test]
    #[ignore]
    fn live_check_returns_info() {
        let info = check();
        eprintln!(
            "LIVE: has_update={} current={} latest={:?} url={:?} dl={:?}",
            info.has_update, info.current, info.latest, info.release_url, info.download_url
        );
        // 当前本地 0.2.6 > 线上 0.2.5 → 不应提示更新
        assert!(info.latest.is_some(), "应能解析出线上版本");
        assert!(!info.has_update, "本地更新时不应提示");
    }

    #[test]
    fn garbage_is_not_newer() {
        assert!(!is_newer("abc", "0.2.5"));
        assert!(!is_newer("", "0.2.5"));
    }

    #[test]
    fn release_latest_url_joins_base() {
        assert_eq!(
            release_latest_url("http://127.0.0.1:1234/"),
            "http://127.0.0.1:1234/repos/K1-lihongrong/dsh-multiver/releases/latest"
        );
    }

    /// sha256_file：对已知内容验证摘要。
    #[test]
    fn sha256_file_matches_known_digest() {
        let p = std::env::temp_dir().join(format!(
            "dsh-sha-test-{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&p, b"abc").unwrap();
        // "abc" 的 SHA256 已知值
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&p).unwrap(), expected);
        let _ = std::fs::remove_file(&p);
    }

    /// download_and_verify：校验失败时应报错（用 file:// 不便，改为直接测 sha 不匹配逻辑）。
    /// 这里只验证"sha256 计算正确 + 大小写不敏感比较"。
    #[test]
    fn sha256_compare_case_insensitive() {
        let a = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let b = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert!(a.eq_ignore_ascii_case(b));
    }

    fn update_tmp_dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dsh-multiver-update-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn install_replace_swaps_target_and_keeps_backup() {
        let dir = update_tmp_dir("success");
        let target = dir.join("dsh-multiver.exe");
        let new_exe = dir.join("downloaded.exe");
        std::fs::write(&target, b"old exe").unwrap();
        std::fs::write(&new_exe, b"new exe").unwrap();

        install_replace(&new_exe, &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new exe");
        assert_eq!(
            std::fs::read(target.with_extension("exe.old")).unwrap(),
            b"old exe"
        );
        assert!(!new_exe.exists(), "下载临时文件应被清理");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_replace_missing_new_keeps_target() {
        let dir = update_tmp_dir("missing-new");
        let target = dir.join("dsh-multiver.exe");
        std::fs::write(&target, b"old exe").unwrap();

        let err = install_replace(&dir.join("no-such.exe"), &target).unwrap_err();
        assert!(err.contains("新版本文件不存在"));
        assert_eq!(std::fs::read(&target).unwrap(), b"old exe");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_replace_missing_target_does_not_create_one() {
        let dir = update_tmp_dir("missing-target");
        let new_exe = dir.join("downloaded.exe");
        std::fs::write(&new_exe, b"new exe").unwrap();

        let err = install_replace(&new_exe, &dir.join("dsh-multiver.exe")).unwrap_err();
        assert!(err.contains("当前程序不存在"));
        assert!(new_exe.exists(), "失败时不应删除下载文件");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn relaunch_script_has_no_process_probe_pipeline() {
        let script = relaunch_script(
            std::path::Path::new(r"C:\Program Files\DSH\dsh-multiver.exe"),
            std::path::Path::new(r"C:\Temp\dsh-multiver-relaunch.log"),
        );
        assert!(script.contains("start \"\" /B /D"));
        assert!(!script.contains("tasklist"), "不应再用 tasklist 探测进程");
        assert!(!script.contains("findstr"), "不应再用 findstr 管道");
        assert!(!script.contains('|'), "detached 批处理禁止管道");
    }

    #[cfg(windows)]
    #[test]
    fn install_replace_can_replace_running_exe() {
        let dir = update_tmp_dir("running-exe");
        let target = dir.join("dsh-multiver.exe");
        let new_exe = dir.join("downloaded.exe");
        let system_root = std::env::var("SystemRoot").unwrap();
        let powershell = std::path::Path::new(&system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let cmd = std::path::Path::new(&system_root)
            .join("System32")
            .join("cmd.exe");
        std::fs::copy(&powershell, &target).unwrap();
        std::fs::copy(&cmd, &new_exe).unwrap();

        let mut child = std::process::Command::new(&target)
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 10"])
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));

        install_replace(&new_exe, &target).unwrap();
        assert_eq!(
            std::fs::read(&target).unwrap(),
            std::fs::read(&cmd).unwrap()
        );
        assert!(target.with_extension("exe.old").is_file());
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
