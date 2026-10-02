use std::path::Path;
use std::process::Command;

#[cfg(windows)]
fn pnpm_command() -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new("cmd");
    c.arg("/C").arg("pnpm");
    // 静默运行，不弹终端黑框（刷新版本列表等场景避免误导用户）
    c.creation_flags(CREATE_NO_WINDOW);
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

/// 从 npm 查询所有可安装的版本号，倒序返回。
/// 显式指定 store/cache/state，避免污染 pnpm 全局目录。
pub fn list_remote(store_dir: &Path, cache_dir: &Path, state_dir: &Path) -> (bool, Vec<String>, String) {
    let mut cmd = pnpm_command();
    // 注意：pnpm view 不接受顶层 --store-dir/--cache-dir/--state-dir（会报 Unknown option），
    // 要用 --config.xxx=value 形式传，既不报错也保持"不污染全局目录"的意图。
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("versions").arg("--json")
        .arg(format!("--config.store-dir={}", store_dir.to_string_lossy()))
        .arg(format!("--config.cache-dir={}", cache_dir.to_string_lossy()))
        .arg(format!("--config.state-dir={}", state_dir.to_string_lossy()));
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

/// 生成 dsh.cmd 转发脚本内容。
///
/// - version: 默认版本号
/// - root_dir: 数据根目录（绝对路径）
/// - isolated: 该默认版本是否开启隔离（决定 DSH_HOME 指向共享 home 还是独立 home）
///
/// 版本号、根目录、DSH_HOME 都**直接写死在脚本里**（由 Rust 在生成时算好），
/// 避免批处理解析 config.json 的脆弱性。
/// 脚本内带保险：若 DSH_HOME 目录不存在，则不设该变量，回退到 dsh 默认（~/.dsh）。
///
/// 平台分派：Windows 生成 .cmd 批处理，Unix 生成 POSIX sh 脚本。
#[cfg(windows)]
pub fn build_forward_script(version: &str, root_dir: &str, isolated: bool) -> String {
    // 确保 root 以反斜杠结尾，方便拼接
    let mut root = root_dir.replace('/', "\\");
    if !root.ends_with('\\') {
        root.push('\\');
    }
    // DSH_HOME：隔离版本 -> <根>versions\<版本>\home；非隔离 -> <根>home
    // 注意：root 已以 \ 结尾，这里不要再加前导 \
    let home = if isolated {
        format!("{}versions\\{}\\home", root, version)
    } else {
        format!("{}home", root)
    };
    let mut s = String::new();
    s.push_str("@echo off\r\n");
    s.push_str("setlocal\r\n");
    // 版本号直接写死在脚本里（由 Rust 在设默认时生成），避免解析 JSON 的脆弱性
    s.push_str(&format!("set \"VER={}\"\r\n", version));
    s.push_str(&format!("set \"ROOT={}\"\r\n", root_dir));
    s.push_str(&format!("set \"DSH_HOME_CANDIDATE={}\"\r\n", home));
    s.push_str("set \"BIN=%ROOT%\\versions\\%VER%\\node_modules\\.bin\\dsh.cmd\"\r\n");
    s.push_str("if not exist \"%BIN%\" (\r\n");
    s.push_str("  echo [dsh] version %VER% not found at %BIN%\r\n");
    s.push_str("  exit /b 1\r\n");
    s.push_str(")\r\n");
    // 保险：DSH_HOME 目录不存在则不设该变量，回退到 dsh 默认位置
    s.push_str("if exist \"%DSH_HOME_CANDIDATE%\" (\r\n");
    s.push_str("  set \"DSH_HOME=%DSH_HOME_CANDIDATE%\"\r\n");
    s.push_str(") else (\r\n");
    s.push_str("  echo [dsh] 数据目录不存在，回退到 dsh 默认位置: %DSH_HOME_CANDIDATE%\r\n");
    s.push_str(")\r\n");
    s.push_str("call \"%BIN%\" %*\r\n");
    s
}

/// Unix 版转发脚本（POSIX sh）。生成 `dsh`（无扩展名，需可执行）。
///
/// 注意：本分支在 Windows 上不参与编译，尚未经真机验证（见 docs/开发缺口.md GAP-005）。
#[cfg(unix)]
pub fn build_forward_script(version: &str, root_dir: &str, isolated: bool) -> String {
    let root = root_dir.trim_end_matches('/');
    let home = if isolated {
        format!("{}/versions/{}/home", root, version)
    } else {
        format!("{}/home", root)
    };
    format!(
        "#!/bin/sh\n\
         # dsh-multiver 转发脚本（自动生成，请勿手改）\n\
         VER='{version}'\n\
         ROOT='{root}'\n\
         BIN=\"$ROOT/versions/$VER/node_modules/.bin/dsh\"\n\
         if [ ! -x \"$BIN\" ]; then\n\
         \x20 echo \"[dsh] 找不到版本 $VER 的入口: $BIN\" >&2\n\
         \x20 exit 1\n\
         fi\n\
         if [ -d '{home}' ]; then\n\
         \x20 export DSH_HOME='{home}'\n\
         else\n\
         \x20 echo \"[dsh] 数据目录不存在，回退到 dsh 默认位置: {home}\" >&2\n\
         fi\n\
         exec \"$BIN\" \"$@\"\n",
        version = version,
        root = root,
        home = home
    )
}

/// 把转发脚本写入目标目录（文件名与可执行权限按平台区分）
pub fn write_forward_script(dir: &Path, content: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    #[cfg(windows)]
    let path = dir.join("dsh.cmd");
    #[cfg(unix)]
    let path = dir.join("dsh");
    std::fs::write(&path, content)?;
    // Unix：设为可执行，否则无法作为命令运行
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(path.to_string_lossy().to_string())
}

/// 用系统默认浏览器打开 URL（跨平台）。
pub fn open_url(url: &str) -> (bool, String) {
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    };
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();

    match result {
        Ok(_) => (true, String::new()),
        Err(e) => (false, e.to_string()),
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

// 转发脚本的测试断言针对 Windows 批处理格式；Unix 版内容不同，
// 故仅在 Windows 下编译（Unix 分支待真机验证，见 GAP-005）。
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn forward_script_shared_home() {
        let s = build_forward_script("0.1.7", "D:\\DSH", false);
        // 非隔离：DSH_HOME 指向 <根>\home
        assert!(s.contains("set \"VER=0.1.7\""));
        assert!(s.contains("set \"ROOT=D:\\DSH\""));
        assert!(s.contains("set \"DSH_HOME_CANDIDATE=D:\\DSH\\home\""));
        // 入口指向版本目录下的 dsh.cmd
        assert!(s.contains("%ROOT%\\versions\\%VER%\\node_modules\\.bin\\dsh.cmd"));
        assert!(s.contains("call \"%BIN%\" %*"));
        // 必须是 CRLF（Windows 批处理）
        assert!(s.contains("\r\n"));
        assert!(!s.contains("\n\n") || s.contains("\r\n"));
    }

    #[test]
    fn forward_script_isolated_home() {
        let s = build_forward_script("0.2.0", "D:\\DSH", true);
        // 隔离：DSH_HOME 指向 <根>\versions\<版本>\home
        assert!(s.contains("set \"DSH_HOME_CANDIDATE=D:\\DSH\\versions\\0.2.0\\home\""));
    }

    #[test]
    fn forward_script_normalizes_slashes_and_trailing_backslash() {
        // root 用正斜杠、且不以反斜杠结尾 → 应归一化为反斜杠并补尾
        let s = build_forward_script("1.0.0", "D:/data/root", false);
        assert!(s.contains("set \"DSH_HOME_CANDIDATE=D:\\data\\root\\home\""));
    }

    #[test]
    fn forward_script_root_already_trailing_backslash() {
        // root 已以反斜杠结尾 → 不应出现双反斜杠
        let s = build_forward_script("1.0.0", "D:\\data\\", false);
        assert!(s.contains("set \"DSH_HOME_CANDIDATE=D:\\data\\home\""));
        assert!(!s.contains("D:\\data\\\\home"));
    }
}

