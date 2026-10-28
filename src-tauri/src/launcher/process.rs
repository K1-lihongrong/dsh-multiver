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
/// M3b：为 dsh 子进程注入 DSH_MULTIVER_* 标记，供插件识别「谁启动的、哪个版本」。
///
/// - `DSH_MULTIVER_VERSION` / `DSH_MULTIVER_ROOT`：从 `<root>/versions/<ver>` 反推
/// - `DSH_MULTIVER_EXE`：multiver 自身可执行文件路径（拿不到时跳过该项）
/// - `DSH_MULTIVER_MODE`：`embedded`（内嵌窗口）/ `console`（控制台或浏览器打开）
/// - `DSH_MULTIVER_PID`：启动该 dsh 的 multiver 进程 PID
///
/// 终端 `dsh` 转发脚本不注入 MODE/PID（那里没有 launcher 进程，PID 会误导）。
fn inject_multiver_env(cmd: &mut Command, version_dir: &Path, mode: &str) {
    if let Some(version) = version_dir.file_name() {
        cmd.env("DSH_MULTIVER_VERSION", version);
    }
    if let Some(root) = version_dir.parent().and_then(|p| p.parent()) {
        cmd.env("DSH_MULTIVER_ROOT", root);
    }
    if let Ok(exe) = std::env::current_exe() {
        cmd.env("DSH_MULTIVER_EXE", exe);
    }
    cmd.env("DSH_MULTIVER_MODE", mode);
    cmd.env("DSH_MULTIVER_PID", std::process::id().to_string());
}

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
    spawn_web_inner(
        version_dir,
        home_dir,
        store_dir,
        cache_dir,
        state_dir,
        0,
        true,
    )
}

fn spawn_web_inner(
    version_dir: &Path,
    home_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    port: u16,
    attach_job: bool,
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
        .arg(port.to_string())
        .arg("--no-open")
        .env("DSH_HOME", home_dir)
        .env("npm_config_store_dir", store_dir)
        .env("npm_config_cache_dir", cache_dir)
        .env("npm_config_state_dir", state_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    inject_multiver_env(&mut cmd, version_dir, "embedded");

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
    let job = if attach_job {
        crate::jobobj::attach(&child)
    } else {
        None
    };

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
        let _ = writeln!(f, "cmd: dsh web --port {} --no-open", port);
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
/// 设计意图：给用户一个**显式的终端窗口**，行为特征与用户自己开终端敲 `dsh web` 一致。
/// - 不传 `--no-open`：让 dsh 自动打开系统默认浏览器显示 WebUI
/// - 新控制台窗口保留：用户可查看 dsh 输出（含 URL、token），关掉窗口即停止服务
/// - home 与「运行」一致（由调用方传入：非隔离用共享 home，隔离用独立 home）
pub fn spawn_web_console(
    version_dir: &Path,
    home_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    port: u16,
) -> (bool, String) {
    #[cfg(windows)]
    {
        spawn_web_console_detached(version_dir, home_dir, store_dir, cache_dir, state_dir, port)
    }
    // Linux/Unix 没有「CREATE_NEW_CONSOLE」：显式寻找终端模拟器来承载控制台窗口，
    // 这样无浏览器时用户仍能在窗口里看到 URL/token，关窗即停（交互设计约定 §2.1）。
    #[cfg(not(windows))]
    {
        spawn_web_console_terminal(version_dir, home_dir, store_dir, cache_dir, state_dir, port)
    }
}

/// 生成终端里执行的启动脚本（dsh 退出后停住窗口，便于阅读失败原因）。
#[cfg(not(windows))]
fn browser_console_script(bin: &Path, port: u16) -> String {
    let cmdline = format!("{} web --port {}", sh_quote(&bin.to_string_lossy()), port);
    format!(
        "#!/bin/sh\n\
         # generated by dsh-multiver; GUI「浏览器打开」控制台窗口\n\
         {}\n\
         status=$?\n\
         printf '\\n[dsh 已退出（状态 %s）] 按回车关闭窗口…\\n' \"$status\"\n\
         read -r _\n\
         exit \"$status\"\n",
        cmdline
    )
}

/// POSIX shell 单引号转义。
#[cfg(not(windows))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(not(windows))]
fn spawn_web_console_terminal(
    version_dir: &Path,
    home_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    port: u16,
) -> (bool, String) {
    use std::os::unix::fs::PermissionsExt;

    let bin = version_dir.join("node_modules").join(".bin").join("dsh");
    if !bin.exists() {
        return (false, format!("找不到启动入口: {}", bin.to_string_lossy()));
    }
    let root = version_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."));
    let log_dir = root.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let version_name: String = version_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let script = log_dir.join(format!(
        "browser-console-{}-{}.sh",
        version_name,
        crate::logging::now_secs()
    ));
    if let Err(e) = std::fs::write(&script, browser_console_script(&bin, port)) {
        return (false, format!("写入启动脚本失败：{}", e));
    }
    if let Ok(meta) = std::fs::metadata(&script) {
        let mut perm = meta.permissions();
        perm.set_mode(0o755);
        let _ = std::fs::set_permissions(&script, perm);
    }

    // 按常见终端模拟器顺序尝试；失败换下一个。
    let candidates: [(&str, &[&str]); 7] = [
        ("x-terminal-emulator", &["-e"]),
        ("xterm", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("alacritty", &["-e"]),
        ("kitty", &["-e"]),
    ];
    let port_text = if port == 0 {
        "由 dsh 自动分配".to_string()
    } else {
        port.to_string()
    };
    let mut last_err = String::new();
    for (term, args) in candidates {
        let mut cmd = Command::new(term);
        cmd.args(args)
            .arg(&script)
            .env("DSH_HOME", home_dir)
            .env("npm_config_store_dir", store_dir)
            .env("npm_config_cache_dir", cache_dir)
            .env("npm_config_state_dir", state_dir);
        inject_multiver_env(&mut cmd, version_dir, "console");
        if let Some(new_path) = crate::runtime::path_env_additions(root) {
            cmd.env("PATH", new_path);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(_) => {
                return (
                    true,
                    format!(
                        "已打开终端窗口运行 dsh web（端口 {}）；关闭终端窗口即停止服务",
                        port_text
                    ),
                );
            }
            Err(e) => last_err = format!("{}: {}", term, e),
        }
    }
    (
        false,
        format!(
            "未找到可用终端模拟器（尝试 x-terminal-emulator / xterm / gnome-terminal / konsole / xfce4-terminal / alacritty / kitty）。最后一次错误：{}。请安装其中之一后重试。",
            last_err
        ),
    )
}

/// CLI `run` 专用：新控制台窗口启动，由 dsh 自己打开系统浏览器（保持旧语义：
/// 关闭控制台窗口即停止服务）。GUI 的「浏览器打开」在 Linux 上走 `spawn_web_console` 的终端模拟器路径。
pub fn spawn_web_console_detached(
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

    // 启动 dsh web（不带 --no-open，由 dsh 自己打开浏览器）。
    //
    // Windows 的 #1 修复（v0.3.0 验收发现窗口全空白）：
    // GUI 进程（Tauri，windows_subsystem = "windows"）本身没有控制台。
    // 原实现只设 CREATE_NEW_CONSOLE，标准句柄没有被可靠绑定到新控制台，
    // dsh 的输出写进了空句柄，窗口存在但全空白。
    //
    // 现在显式 stdio(inherit) + CREATE_NEW_CONSOLE：
    // 子进程的标准句柄由新控制台初始化，dsh 的两行输出（URL/token 与
    // "opening the default browser"）才能显示在窗口里。
    //
    // 交互约束（交互设计约定 §2.1）：
    // - 新控制台 + dsh 自开浏览器 ✅
    // - 关窗即停 ✅（实测：关闭控制台窗口后 cmd 与 node 同时终止，无残留）
    // - 管理器退出不杀 dsh ✅（靠 configure_command 不挂 Job 保证）
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;

        let mut c = Command::new("cmd");
        c.arg("/C").arg(&bin);
        // 显式 inherit：让子进程的标准句柄由新控制台初始化，
        // 而不是从无控制台的 GUI 父进程继承空句柄。
        c.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .creation_flags(CREATE_NEW_CONSOLE);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = Command::new(&bin);

    cmd.arg("web").arg("--port").arg(port.to_string());

    cmd.env("DSH_HOME", home_dir)
        .env("npm_config_store_dir", store_dir)
        .env("npm_config_cache_dir", cache_dir)
        .env("npm_config_state_dir", state_dir);

    inject_multiver_env(&mut cmd, version_dir, "console");

    if let Some(root) = version_dir.parent().and_then(|p| p.parent()) {
        if let Some(new_path) = crate::runtime::path_env_additions(root) {
            cmd.env("PATH", new_path);
        }
    }

    #[cfg(unix)]
    {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }

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

#[cfg(all(test, not(windows)))]
mod tests {
    use super::{browser_console_script, sh_quote};
    use std::path::Path;

    #[test]
    fn shell_quote_escapes_single_quote() {
        assert_eq!(sh_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(sh_quote("/tmp/it's"), "'/tmp/it'\\''s'");
    }

    #[test]
    fn console_script_runs_dsh_without_no_open() {
        let s = browser_console_script(Path::new("/tmp/a b/dsh"), 3080);
        assert!(s.contains("'/tmp/a b/dsh' web --port 3080"));
        assert!(!s.contains("--no-open"));
        assert!(s.contains("按回车关闭窗口"));
    }
}

/// M3b：env 注入的跨平台验证。Command::get_envs 不依赖实际 spawn。
#[cfg(test)]
mod env_tests {
    use super::inject_multiver_env;
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;

    fn collect_envs(cmd: &Command) -> HashMap<String, String> {
        cmd.get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().to_string(),
                    v?.to_string_lossy().to_string(),
                ))
            })
            .collect()
    }

    #[test]
    fn injects_version_root_mode_pid() {
        let mut cmd = Command::new("dsh");
        inject_multiver_env(&mut cmd, Path::new("R:/root/versions/1.2.3"), "embedded");
        let envs = collect_envs(&cmd);
        assert_eq!(
            envs.get("DSH_MULTIVER_VERSION").map(String::as_str),
            Some("1.2.3")
        );
        assert_eq!(
            envs.get("DSH_MULTIVER_ROOT").map(String::as_str),
            Some("R:/root")
        );
        assert_eq!(
            envs.get("DSH_MULTIVER_MODE").map(String::as_str),
            Some("embedded")
        );
        assert!(envs.contains_key("DSH_MULTIVER_PID"));
        assert!(envs.contains_key("DSH_MULTIVER_EXE"));
    }

    #[test]
    fn console_mode_is_passed_through() {
        let mut cmd = Command::new("dsh");
        inject_multiver_env(&mut cmd, Path::new("R:/root/versions/2.0.0"), "console");
        assert_eq!(
            collect_envs(&cmd)
                .get("DSH_MULTIVER_MODE")
                .map(String::as_str),
            Some("console")
        );
    }
}
