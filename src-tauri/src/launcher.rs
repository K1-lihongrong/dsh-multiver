use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    // spawn 前：平台相关配置（Unix 下设为新进程组 + PDEATHSIG；Windows 无操作）
    crate::jobobj::configure_command(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("启动 dsh web 失败: {}", e))?;

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
        let _ = writeln!(f, "env store={} cache={} state={}",
            store_dir.to_string_lossy(), cache_dir.to_string_lossy(), state_dir.to_string_lossy());
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
                    return Err(format!("dsh web 进程提前退出（状态: {}）", status));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // 读线程结束但未解析到 URL（进程结束 / 输出格式变化）
                kill_tree(&mut child);
                let _ = child.wait();
                return Err(format!(
                    "未能从 dsh web 输出中解析到访问地址（输出格式可能已变化）。\n请查看日志目录：\n{}",
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

/// 从一行文本中解析完整 URL：取 `dsh web: <URL>` 之后的 URL 整体（到行尾/空格前）。
/// 兼容 127.0.0.1 与 localhost。返回形如 http://127.0.0.1:<port>/?token=<token>。
fn parse_url(line: &str) -> Option<String> {
    // 优先识别 `dsh web:` 标记行
    let candidate = if let Some(idx) = line.find("dsh web:") {
        line[idx + "dsh web:".len()..].trim()
    } else {
        line.trim()
    };
    let start = candidate.find("http://")?;
    let rest = &candidate[start..];
    // URL 到行尾或第一个空白字符结束
    let url: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    if url.contains("127.0.0.1:") || url.contains("localhost:") {
        Some(url)
    } else {
        None
    }
}

/// 为某版本在桌面创建快捷方式。
///
/// - Windows：`DSH <版本>.lnk`，用 PowerShell COM 生成（零依赖）
/// - Unix：`DSH <版本>.desktop`，写 Desktop Entry 文件并置可执行
#[cfg(windows)]
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir().ok_or_else(|| "无法定位桌面目录".to_string())?;
    let lnk_name = format!("DSH {}.lnk", version);
    let lnk_path = desktop.join(&lnk_name);

    let script = build_shortcut_script(&lnk_path, exe_path, version);

    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("调用 PowerShell 失败: {}", e))?
    };
    #[cfg(not(windows))]
    let out = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .output()
        .map_err(|e| format!("调用 PowerShell 失败: {}", e))?;

    if out.status.success() {
        Ok(lnk_path.to_string_lossy().to_string())
    } else {
        let e = String::from_utf8_lossy(&out.stderr);
        Err(format!("创建快捷方式失败: {}", e.trim()))
    }
}

/// Unix：生成 `.desktop` 桌面入口。
///
/// 无桌面环境时返回清晰错误（见 desktop_dir）。已在 WSL 上验证（GAP-005）。
#[cfg(unix)]
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir().ok_or_else(|| {
        "未找到桌面目录（当前环境可能没有桌面，如 WSL/服务器/容器）".to_string()
    })?;
    let file = desktop.join(format!("DSH {}.desktop", version));
    let work = exe_path.parent().unwrap_or(Path::new("."));
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=DSH {ver}\n\
         Comment=运行 DeepSeek Harness {ver}\n\
         Exec=\"{exe}\" --launch-version {ver}\n\
         Path={work}\n\
         Terminal=false\n\
         Categories=Development;\n",
        ver = version,
        exe = exe_path.to_string_lossy(),
        work = work.to_string_lossy()
    );
    std::fs::write(&file, content).map_err(|e| e.to_string())?;
    // 置为可执行（部分桌面环境要求）
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
    }
    // GNOME 需要"信任"标记才会允许直接双击；失败不阻塞（KDE/XFCE 无需）
    let _ = Command::new("gio")
        .args(["set", &file.to_string_lossy(), "metadata::trusted", "true"])
        .output();
    Ok(file.to_string_lossy().to_string())
}

#[cfg(windows)]
fn build_shortcut_script(lnk: &Path, exe_path: &Path, version: &str) -> String {
    let work = exe_path.parent().unwrap_or(Path::new("."));
    // AppUserModelID 必须与进程启动时设置的（lib.rs 的 set_windows_app_user_model_id，
    // 取自 app identifier）完全一致，否则 Windows 任务栏会把进程与快捷方式分到不同组，
    // 导致图标丢失。
    let aumid = "io.github.K1-lihongrong.dsh-multiver";
    format!(
        "$ws = New-Object -ComObject WScript.Shell; $lnk = $ws.CreateShortcut('{}'); $lnk.TargetPath = '{}'; $lnk.Arguments = '--launch-version {}'; $lnk.WorkingDirectory = '{}'; $lnk.AppUserModelID = '{}'; $lnk.Save()",
        escape_ps(&lnk.to_string_lossy()),
        escape_ps(&exe_path.to_string_lossy()),
        escape_ps(version),
        escape_ps(&work.to_string_lossy()),
        escape_ps(aumid),
    )
}

/// PowerShell 单引号字符串内转义（把 ' 变成 ''）
#[cfg(windows)]
fn escape_ps(s: &str) -> String {
    s.replace('\'', "''")
}

/// 桌面目录。
///
/// - Windows：优先读注册表 Shell Folders\Desktop（OneDrive/域策略/重定向都能覆盖），
///   读不到再回退常见路径猜测
/// - Unix：优先 `xdg-user-dir DESKTOP`，其次 `~/Desktop`、`~/桌面`
#[cfg(windows)]
fn desktop_dir() -> Option<PathBuf> {
    if let Some(dir) = desktop_from_registry() {
        if dir.exists() {
            return Some(dir);
        }
    }
    let user = std::env::var("USERPROFILE").ok()?;
    let plain = PathBuf::from(&user).join("Desktop");
    if plain.exists() {
        return Some(plain);
    }
    let onedrive = PathBuf::from(&user).join("OneDrive").join("Desktop");
    if onedrive.exists() {
        return Some(onedrive);
    }
    Some(plain)
}

/// 从注册表读取真实桌面路径：HKCU\...\Explorer\Shell Folders 的 Desktop 值。
/// 用 reg query 读取（零依赖）；失败返回 None。
#[cfg(windows)]
fn desktop_from_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let out = Command::new("reg")
        .args([
            "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders",
            "/v",
            "Desktop",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // 输出形如：    Desktop    REG_SZ    D:\Users\ALING\Desktop
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Desktop") {
            let value = rest
                .trim_start()
                .strip_prefix("REG_SZ")
                .or_else(|| rest.trim_start().strip_prefix("REG_EXPAND_SZ"))?
                .trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

/// Unix 桌面目录：优先 `xdg-user-dir DESKTOP`，其次 `~/Desktop`、`~/桌面`。
/// 注意：本分支在 Windows 上不参与编译，尚未经真机验证（见 GAP-005）。
#[cfg(unix)]
fn desktop_dir() -> Option<PathBuf> {
    // 1) XDG 标准：xdg-user-dir 会读 ~/.config/user-dirs.dirs
    if let Ok(out) = Command::new("xdg-user-dir").arg("DESKTOP").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() && Path::new(&p).exists() {
                return Some(PathBuf::from(p));
            }
        }
    }
    // 2) 回退常见路径（存在则直接用）
    let home = std::env::var("HOME").ok()?;
    for name in ["Desktop", "桌面"] {
        let p = PathBuf::from(&home).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    // 3) 无桌面环境（如 WSLg/服务器/容器）：返回 None，由调用方给出清晰错误，
    //    而不是返回一个不存在的路径导致 "No such file or directory (os error 2)"。
    None
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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 新控制台窗口，让用户看到 dsh 输出（含 URL/token）
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }

    // Unix：脱离父会话/终端，避免随管理器退出被 SIGHUP（配合新进程组 + PDEATHSIG 由 configure 设定）
    #[cfg(unix)]
    {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }

    // spawn 前：平台相关配置（Unix 下设为新进程组 + PDEATHSIG；Windows 无操作）
    crate::jobobj::configure_command(&mut cmd);

    match cmd.spawn() {
        Ok(_) => (true, format!("已用端口 {} 启动 dsh web，稍候会自动打开浏览器", port)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_marker_line() {
        let line = "dsh web: http://127.0.0.1:44179/?token=abc123";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:44179/?token=abc123".to_string())
        );
    }

    #[test]
    fn parse_url_bare_url() {
        let line = "  http://127.0.0.1:8080/?token=xyz  ";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:8080/?token=xyz".to_string())
        );
    }

    #[test]
    fn parse_url_localhost_variant() {
        let line = "dsh web: http://localhost:3000/";
        assert_eq!(parse_url(line), Some("http://localhost:3000/".to_string()));
    }

    #[test]
    fn parse_url_ignores_trailing_text() {
        // URL 后跟空白/其他内容时只取到空白前
        let line = "dsh web: http://127.0.0.1:5000/?token=t  (按 Ctrl+C 退出)";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:5000/?token=t".to_string())
        );
    }

    #[test]
    fn parse_url_rejects_non_loopback() {
        // 非 127.0.0.1 / localhost 的地址不识别
        assert_eq!(parse_url("http://192.168.1.5:8080/"), None);
        assert_eq!(parse_url("no url here"), None);
    }
}
