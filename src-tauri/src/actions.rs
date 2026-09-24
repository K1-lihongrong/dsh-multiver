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

/// 从 npm 查询所有可安装的版本号，倒序返回。
/// 显式指定 store/cache/state，避免污染 pnpm 全局目录。
pub fn list_remote(store_dir: &Path, cache_dir: &Path, state_dir: &Path) -> (bool, Vec<String>, String) {
    let mut cmd = pnpm_command();
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("versions").arg("--json")
        .arg("--store-dir").arg(store_dir)
        .arg("--cache-dir").arg(cache_dir)
        .arg("--state-dir").arg(state_dir);
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

/// 把转发脚本写入目标目录
pub fn write_forward_script(dir: &Path, content: &str) -> std::io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("dsh.cmd");
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
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

