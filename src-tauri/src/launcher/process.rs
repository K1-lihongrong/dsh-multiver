//! dsh 进程启动与终止（D14 拆分：原 launcher.rs 的进程管理）。
//!
//! - spawn_web_hidden：内嵌窗口用，--port 0 --no-open，捕获 URL
//! - spawn_web_console：新控制台窗口用，dsh 自动打开浏览器
//! - kill_tree：按平台杀整棵进程树
//! - port_in_use：端口占用探测

use super::diagnose::{parse_url, read_recent_errors};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
/// 启动 dsh web（隐藏窗口），返回子进程句柄与**完整访问 URL**（含 token）。
///
/// 用 --port 0 让系统自动分配空闲端口；dsh 启动后会打印：
///   dsh web: http://127.0.0.1:<port>/?token=<token>
/// 该 URL 含鉴权 token，必须整体使用，否则页面空白。
/// 加 --no-open 避免 dsh 自动打开系统默认浏览器。
pub fn spawn_web_hidden(
    version_dir: &Path,
    home_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
) -> Result<(Child, String, Option<crate::jobobj::ProcessGuard>), String> {
    let bin = version_dir
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if !bin.exists() {
        return Err(format!("找不到启动入口: {}", bin.to_string_lossy()));
    }

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&bin);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = Command::new(&bin);

    cmd.arg("web")
        .arg("--port")
        .arg("0")
        .arg("--no-open")
        .env("DSH_HOME", home_dir)
        .env("npm_config_store_dir", store_dir)
        .env("npm_config_cache_dir", cache_dir)
        .env("npm_config_state_dir", state_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // 运行时适配（v0.2.9）：若系统缺 Node，把便携 Node 目录注入 dsh 子进程 PATH，
    // 使 dsh.cmd 的 "就近 node.exe → PATH 兜底" 解析到便携 Node（已实测有效）。
    // version_dir = <root>/versions/<ver>，上两级即 <root>。
    if let Some(root) = version_dir.parent().and_then(|p| p.parent()) {
        if let Some(new_path) = crate::runtime::path_env_additions(root) {
            cmd.env("PATH", new_path);
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    // spawn 前：平台相关配置（Unix 下设为新进程组；Windows 无操作）
    crate::jobobj::configure_command(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 dsh web 失败: {}", e))?;

    // spawn 后：加入进程守卫。
    // - Windows：加入 Job（KILL_ON_JOB_CLOSE），管理器退出即由 OS 杀光组内进程
    // - Unix：进程组已在 configure 阶段设好，此处返回占位句柄
    // 根除"窗口关了但 node 还在"的孤儿进程问题。
    let job = crate::jobobj::attach(&child);

    // ── 会话日志（阶段一：可观测性）──
    // 每次启动 dsh 生成一个带时间戳的会话文件，记录 stdout + stderr 全量输出，
    // 便于事后排查「无法复现」的偶发问题。日志落在 <root>/logs/（与「打开日志目录」一致）。
    let version_name = version_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    // version_dir = <root>/versions/<ver>，故上两级即 <root>
    let log_dir = version_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|r| r.join("logs"))
        .unwrap_or_else(|| home_dir.join("logs"));
    crate::logging::ensure_dir(&log_dir);
    // 限制会话日志数量，避免无限增长（保留最新 20 份）
    crate::logging::prune_old(&log_dir, "session-", ".log", 20);

    let session_path = log_dir.join(format!(
        "session-{}-{}.log",
        crate::logging::stamp_compact(crate::logging::now_secs()),
        version_name
    ));
    // 用同一文件句柄（Mutex 保护）承接 stdout / stderr 两个读线程的写入
    let session_file = crate::logging::open_append(&session_path).map(|f| {
        use std::io::Write;
        let mut f = f;
        let stamp = crate::logging::stamp_compact(crate::logging::now_secs());
        let _ = writeln!(f, "===== DSH {} session @ {} =====", version_name, stamp);
        let _ = writeln!(f, "cmd: dsh web --port 0 --no-open");
        let _ = writeln!(f, "env DSH_HOME={}", home_dir.to_string_lossy());
        let _ = writeln!(
            f,
            "env store={} cache={} state={}",
            store_dir.to_string_lossy(),
            cache_dir.to_string_lossy(),
            state_dir.to_string_lossy()
        );
        let _ = writeln!(f, "pid: {}", child.id());
        std::sync::Arc::new(std::sync::Mutex::new(f))
    });

    // stdout 用独立线程读取：解析到完整 URL 后**继续读到 EOF 并落盘**，
    // 避免 dsh 运行中往 stdout 写满管道缓冲区导致进程阻塞（假死）。
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法捕获 dsh web 输出".to_string())?;

    let (tx, rx) = mpsc::channel::<String>();
    let sf_out = session_file.clone();
    std::thread::spawn(move || {
        use std::io::Write;
        let reader = BufReader::new(stdout);
        let mut sent = false;
        for line in reader.lines().map_while(Result::ok) {
            if !sent {
                if let Some(url) = parse_url(&line) {
                    let _ = tx.send(url);
                    sent = true;
                }
            }
            if let Some(f) = sf_out.as_ref() {
                if let Ok(mut g) = f.lock() {
                    let _ = writeln!(g, "[out] {}", line);
                }
            }
        }
    });

    // stderr 同样由独立线程读到 EOF 并落盘（同一会话文件），避免管道填满阻塞 dsh。
    if let Some(stderr) = child.stderr.take() {
        let sf_err = session_file.clone();
        std::thread::spawn(move || {
            use std::io::Write;
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(f) = sf_err.as_ref() {
                    if let Ok(mut g) = f.lock() {
                        let _ = writeln!(g, "[err] {}", line);
                    }
                }
            }
        });
    }

    // 等待 URL 的超时时间。
    //
    // 注意：dsh 首次（冷）启动可能很慢——需加载大量 node 模块与原生模块（node-pty/koffi），
    // 叠加杀软扫描，可能显著超过 30 秒，且启动期间**不打印任何输出**（静默到最后才输出 URL）。
    // 因此这里放宽到 90 秒，避免把「首次启动慢」误判为失败（现象：首次点击超时、再点即成功）。
    const LAUNCH_TIMEOUT_SECS: u64 = 90;
    let deadline = Instant::now() + Duration::from_secs(LAUNCH_TIMEOUT_SECS);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(url) => return Ok((child, url, job)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    return Err(format!(
                        "启动超时：{} 秒内未解析到 dsh web 地址。首次启动通常较慢，可稍后重试。\n若反复超时，请查看日志目录：\n{}",
                        LAUNCH_TIMEOUT_SECS,
                        log_dir.to_string_lossy()
                    ));
                }
                // 进程若已退出，提前报错
                if let Ok(Some(status)) = child.try_wait() {
                    let _ = child.wait();
                    std::thread::sleep(Duration::from_millis(150)); // 给 stderr 读线程一点落盘时间
                    let detail = read_recent_errors(&session_path, 6);
                    if detail.is_empty() {
                        return Err(format!(
                            "dsh web 进程提前退出（状态: {}）。\n请查看日志目录：\n{}",
                            status,
                            log_dir.to_string_lossy()
                        ));
                    }
                    return Err(format!(
                        "dsh web 启动失败（进程退出，状态: {}）。dsh 输出：\n{}\n\n完整日志目录：\n{}",
                        status, detail, log_dir.to_string_lossy()
                    ));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // 读线程结束但未解析到 URL（进程结束）
                kill_tree(&mut child);
                let _ = child.wait();
                std::thread::sleep(Duration::from_millis(150)); // 给 stderr 读线程一点落盘时间
                let detail = read_recent_errors(&session_path, 6);
                if detail.is_empty() {
                    return Err(format!(
                        "dsh web 未输出访问地址就退出了（dsh 未打印错误）。\n请查看日志目录：\n{}",
                        log_dir.to_string_lossy()
                    ));
                }
                return Err(format!(
                    "dsh web 启动失败。dsh 输出：\n{}\n\n完整日志目录：\n{}",
                    detail,
                    log_dir.to_string_lossy()
                ));
            }
        }
    }
}

/// 结束一个 dsh 进程及其整棵子进程树。
///
/// - Windows：`taskkill /T /F`（杀整棵进程树）
/// - Unix：`killpg`（杀整个进程组——子进程由 configure_command 设为组长）
pub fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let pid = child.id();
        let _ = Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F")
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(unix)]
    {
        // 子进程是新进程组组长（pgid = pid），杀整组覆盖其所有后代
        let pid = child.id() as i32;
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

/// 用**新控制台窗口**启动某版本的 dsh web（供「浏览器打开」按钮）。
///
/// - 不传 --no-open：让 dsh 自动打开系统默认浏览器显示 WebUI
/// - 传 --port 0：避免多版本撞端口；dsh 会把完整 URL（含 token）打印在终端里，供用户复制
/// - 新控制台窗口保留，用户可查看 dsh 输出（含 URL、token）
/// - home 与「运行」一致（由调用方传入：非隔离用共享 home，隔离用独立 home）
pub fn spawn_web_console(
    version_dir: &Path,
    home_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    port: u16,
) -> (bool, String) {
    let bin = version_dir
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if !bin.exists() {
        return (false, format!("找不到启动入口: {}", bin.to_string_lossy()));
    }

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&bin);
        c
    };
    // Unix：不依赖终端模拟器（探测脆弱），直接后台启动；dsh 会自行打开默认浏览器
    #[cfg(not(windows))]
    let mut cmd = Command::new(&bin);

    cmd.arg("web")
        .arg("--port")
        .arg(port.to_string())
        .env("DSH_HOME", home_dir)
        .env("npm_config_store_dir", store_dir)
        .env("npm_config_cache_dir", cache_dir)
        .env("npm_config_state_dir", state_dir);

    // 运行时适配（v0.2.9）：同 spawn_web_hidden，注入便携运行时 PATH。
    if let Some(root) = version_dir.parent().and_then(|p| p.parent()) {
        if let Some(new_path) = crate::runtime::path_env_additions(root) {
            cmd.env("PATH", new_path);
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 新控制台窗口，让用户看到 dsh 输出（含 URL/token）
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }

    // Unix：脱离父会话/终端，避免随管理器退出被 SIGHUP（配合新进程组，由 configure 设定）
    #[cfg(unix)]
    {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }

    // spawn 前：平台相关配置（Unix 下设为新进程组；Windows 无操作）
    crate::jobobj::configure_command(&mut cmd);

    match cmd.spawn() {
        Ok(_) => (
            true,
            format!("已用端口 {} 启动 dsh web，稍候会自动打开浏览器", port),
        ),
        Err(e) => (false, format!("启动失败: {}", e)),
    }
}

/// 检测本机 127.0.0.1:<port> 是否已被占用（能连上即视为占用）。
pub fn port_in_use(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}
