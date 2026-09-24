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
) -> Result<(Child, String), String> {
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

    let mut child = cmd.spawn().map_err(|e| format!("启动 dsh web 失败: {}", e))?;

    // stdout 用独立线程读取：解析到完整 URL 后，**继续读到 EOF 并丢弃**，
    // 避免 dsh 运行中往 stdout 写满管道缓冲区导致进程阻塞（假死）。
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法捕获 dsh web 输出".to_string())?;

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut sent = false;
        for line in reader.lines().map_while(Result::ok) {
            if !sent {
                if let Some(url) = parse_url(&line) {
                    let _ = tx.send(url);
                    sent = true;
                }
            }
            // 拿到 URL 后仍继续消费，不做任何事（丢弃），直到 EOF
        }
    });

    // stderr 也保留 piped 并由独立线程读到 EOF，避免其管道填满阻塞 dsh。
    // 同时把 stderr 落盘到 <home>/logs/dsh-stderr.log，便于排查。
    if let Some(stderr) = child.stderr.take() {
        let log_path = home_dir.join("logs").join("dsh-stderr.log");
        std::thread::spawn(move || {
            use std::io::Write;
            let reader = BufReader::new(stderr);
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok();
            for line in reader.lines().map_while(Result::ok) {
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{}", line);
                }
            }
        });
    }

    // 30 秒超时等待 URL
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(url) => return Ok((child, url)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    return Err("启动超时：30 秒内未解析到 dsh web 地址".to_string());
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
                return Err("未能从 dsh web 输出中解析到访问地址（输出格式可能已变化）".to_string());
            }
        }
    }
}

/// 结束一个 dsh 进程及其整棵子进程树（Windows 用 taskkill /T /F）。
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

/// 为某版本在桌面创建快捷方式：DSH <版本号>.lnk
///
/// 目标为 <exe_path> --launch-version <版本号>，用 PowerShell COM 生成（零依赖）。
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir().ok_or_else(|| "无法定位桌面目录".to_string())?;
    let lnk_name = format!("DSH {}.lnk", version);
    let lnk_path = desktop.join(&lnk_name);

    let script = build_shortcut_script(&lnk_path, exe_path, version);

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

fn build_shortcut_script(lnk: &Path, exe_path: &Path, version: &str) -> String {
    let work = exe_path.parent().unwrap_or(Path::new("."));
    format!(
        "$ws = New-Object -ComObject WScript.Shell; $lnk = $ws.CreateShortcut('{}'); $lnk.TargetPath = '{}'; $lnk.Arguments = '--launch-version {}'; $lnk.WorkingDirectory = '{}'; $lnk.Save()",
        escape_ps(&lnk.to_string_lossy()),
        escape_ps(&exe_path.to_string_lossy()),
        escape_ps(version),
        escape_ps(&work.to_string_lossy()),
    )
}

/// PowerShell 单引号字符串内转义（把 ' 变成 ''）
fn escape_ps(s: &str) -> String {
    s.replace('\'', "''")
}

/// 桌面目录：优先 USERPROFILE\\Desktop，回退 USERPROFILE\\OneDrive\\Desktop
fn desktop_dir() -> Option<PathBuf> {
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
    #[cfg(not(windows))]
    let mut cmd = Command::new(&bin);

    cmd.arg("web")
        .arg("--port")
        .arg("0")
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

    match cmd.spawn() {
        Ok(_) => (true, "已在新终端启动 dsh web，稍候会自动打开浏览器".to_string()),
        Err(e) => (false, format!("启动失败: {}", e)),
    }
}
