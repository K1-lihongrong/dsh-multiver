use std::path::Path;
use std::process::Command;

#[cfg(windows)]
fn pnpm_command() -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg("pnpm");
    c
}

#[cfg(not(windows))]
fn pnpm_command() -> Command {
    Command::new("pnpm")
}

fn run(cmd: &mut Command) -> (bool, String, String) {
    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            (out.status.success(), stdout, stderr)
        }
        Err(e) => (false, String::new(), e.to_string()),
    }
}

/// 从 npm 查询所有可安装的版本号，倒序返回
pub fn list_remote() -> (bool, Vec<String>, String) {
    let mut cmd = pnpm_command();
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("versions").arg("--json");
    let (ok, stdout, stderr) = run(&mut cmd);
    if !ok {
        let msg = if stderr.trim().is_empty() { stdout } else { stderr };
        return (false, Vec::new(), msg);
    }
    let trimmed = stdout.trim();
    let mut versions: Vec<String> = Vec::new();
    if let Ok(v) = serde_json::from_str::<Vec<String>>(trimmed) {
        versions = v;
    } else if let Ok(v) = serde_json::from_str::<Vec<Vec<String>>>(trimmed) {
        if let Some(first) = v.into_iter().next() {
            versions = first;
        }
    } else {
        versions = trimmed
            .lines()
            .map(|s| s.trim().trim_matches('"').trim_matches(',').trim_matches('[').trim_matches(']').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    versions.reverse();
    (true, versions, String::new())
}

/// 生成 dsh.cmd 转发脚本内容
/// version: 默认版本号；root_dir: 数据根目录（绝对路径）
pub fn build_forward_script(version: &str, root_dir: &str) -> String {
    // 确保 root 以反斜杠结尾，方便拼接
    let mut root = root_dir.replace('/', "\\");
    if !root.ends_with('\\') {
        root.push('\\');
    }
    let mut s = String::new();
    s.push_str("@echo off\r\n");
    s.push_str("setlocal\r\n");
    // 版本号直接写死在脚本里（由 Rust 在设默认时生成），避免解析 JSON 的脆弱性
    s.push_str(&format!("set \"VER={}\"\r\n", version));
    s.push_str(&format!("set \"ROOT={}\"\r\n", root_dir));
    s.push_str("set \"BIN=%ROOT%\\versions\\%VER%\\node_modules\\.bin\\dsh.cmd\"\r\n");
    s.push_str("if not exist \"%BIN%\" (\r\n");
    s.push_str("  echo [dsh] version %VER% not found at %BIN%\r\n");
    s.push_str("  exit /b 1\r\n");
    s.push_str(")\r\n");
    s.push_str("call \"%BIN%\" %*\r\n");
    s
}

/// 把转发脚本写入目标目录
pub fn write_forward_script(dir: &Path, content: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("dsh.cmd");
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// 启动某个版本的 dsh web
pub fn spawn_web(version_dir: &Path, home_dir: &Path, store_dir: &Path) -> (bool, String) {
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
        .env("DSH_HOME", home_dir)
        .env("npm_config_store_dir", store_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }

    match cmd.spawn() {
        Ok(_) => (true, "已启动 dsh web（新窗口）".to_string()),
        Err(e) => (false, format!("启动失败: {}", e)),
    }
}

/// 打开文件夹
pub fn open_folder(path: &Path) -> (bool, String) {
    #[cfg(windows)]
    let result = Command::new("explorer").arg(path).spawn();
    #[cfg(not(windows))]
    let result = Command::new("xdg-open").arg(path).spawn();

    match result {
        Ok(_) => (true, String::new()),
        Err(e) => (false, e.to_string()),
    }
}
