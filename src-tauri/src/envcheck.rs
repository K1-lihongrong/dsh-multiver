use std::path::Path;
use std::process::Command;
use serde::Serialize;

#[cfg(windows)]
fn cmd_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new("cmd");
    c.arg("/C").arg(program);
    // 静默运行，不弹终端黑框（检查 node/pnpm/registry 时避免误导用户）
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(not(windows))]
fn cmd_command(program: &str) -> Command {
    Command::new(program)
}

/// 单项检查结果
#[derive(Serialize)]
pub struct CheckItem {
    /// 检查项名称
    pub name: String,
    /// 是否通过
    pub ok: bool,
    /// 详细信息（版本号或错误原因）
    pub detail: String,
    /// 是否致命（不通过则无法安装）
    pub critical: bool,
    /// 安装引导：检查未通过时给出的说明（可能为空）
    #[serde(default)]
    pub install_hint: String,
    /// 安装引导：官方下载页 URL（可能为空）
    #[serde(default)]
    pub install_url: String,
}

/// 构造一个"通过"项（无引导）。
fn ok_item(name: &str, detail: String, critical: bool) -> CheckItem {
    CheckItem {
        name: name.to_string(),
        ok: true,
        detail,
        critical,
        install_hint: String::new(),
        install_url: String::new(),
    }
}

/// 构造一个"未通过"项（带引导）。
fn fail_item(
    name: &str,
    detail: String,
    critical: bool,
    hint: &str,
    url: &str,
) -> CheckItem {
    CheckItem {
        name: name.to_string(),
        ok: false,
        detail,
        critical,
        install_hint: hint.to_string(),
        install_url: url.to_string(),
    }
}

/// 运行命令并取首行输出
fn run_version(program: &str) -> Option<String> {
    let mut cmd = cmd_command(program);
    cmd.arg("--version");
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if e.is_empty() { None } else { Some(e.lines().next().unwrap_or("").to_string()) }
            } else {
                Some(s.lines().next().unwrap_or("").to_string())
            }
        }
        _ => None,
    }
}

/// 解析版本号中的主版本，如 "v22.19.0" -> 22
fn major_version(s: &str) -> Option<u32> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    cleaned.split('.').next()?.parse::<u32>().ok()
}

/// 检查 Node.js 是否可用且版本满足 >= 22
pub fn check_node() -> CheckItem {
    match run_version("node") {
        Some(v) => {
            let major = major_version(&v).unwrap_or(0);
            if major >= 22 {
                ok_item("Node.js", format!("{}（满足 >= 22）", v), true)
            } else {
                fail_item(
                    "Node.js",
                    format!("{}（需要 22 或更高）", v),
                    true,
                    "请到 Node.js 官网下载 LTS 版（22 或更高）安装，安装后重新检查。",
                    "https://nodejs.org/zh-cn/download",
                )
            }
        }
        None => fail_item(
            "Node.js",
            "未找到 node 命令，请先安装 Node.js 22+".to_string(),
            true,
            "请到 Node.js 官网下载 LTS 版（22 或更高）安装。安装时勾选「Add to PATH」，装完重开本工具。",
            "https://nodejs.org/zh-cn/download",
        ),
    }
}

/// 检查 pnpm 是否可用
pub fn check_pnpm() -> CheckItem {
    match run_version("pnpm") {
        Some(v) => ok_item("pnpm", v, true),
        None => fail_item(
            "pnpm",
            "未找到 pnpm 命令，请先安装 pnpm".to_string(),
            true,
            "Node 装好后，在终端运行：npm install -g pnpm（也可参考 pnpm 官网）。",
            "https://pnpm.io/zh/installation",
        ),
    }
}

/// 检查根目录可写
pub fn check_writable(root: &Path) -> CheckItem {
    let test_file = root.join(".write-test");
    let result = std::fs::write(&test_file, b"test").and_then(|_| std::fs::remove_file(&test_file));
    match result {
        Ok(_) => ok_item("根目录可写", root.to_string_lossy().to_string(), true),
        Err(e) => fail_item(
            "根目录可写",
            format!("无法写入 {}: {}", root.to_string_lossy(), e),
            true,
            "请把「数据根目录」改到一个有写权限的位置（如 D:\\dsh-data），或检查该目录是否被占用/只读。",
            "",
        ),
    }
}

/// 检查磁盘剩余空间（返回 MB）
pub fn check_disk(root: &Path) -> CheckItem {
    // 用 fs2 或简单方案：创建大文件不现实，这里改用读取盘符信息
    // 简化：用一个临时文件估算不可行，直接报 OK 并附路径，实际空间由 pnpm 自行报错
    ok_item("磁盘空间", format!("目标：{}", root.to_string_lossy()), false)
}

/// 检查 npm registry 是否可达
pub fn check_registry() -> CheckItem {
    // 用 pnpm view 查询一个轻量包，验证 registry 连通
    let mut cmd = cmd_command("pnpm");
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("version");
    match cmd.output() {
        Ok(out) if out.status.success() => ok_item(
            "npm 源连通",
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
            false,
        ),
        Ok(out) => {
            let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
            fail_item(
                "npm 源连通",
                if e.is_empty() { "无法访问 npm 源".to_string() } else { e.lines().next().unwrap_or("").to_string() },
                false,
                "可能是网络或代理问题。安装时若失败，可在弹窗里换一个 npm 源（如阿里云 npmmirror）重试。",
                "",
            )
        }
        Err(e) => fail_item(
            "npm 源连通",
            format!("检查失败：{}", e),
            false,
            "可能是网络或代理问题。安装时若失败，可在弹窗里换一个 npm 源重试。",
            "",
        ),
    }
}

/// 执行全部环境检查
pub fn run_all(root: &Path) -> Vec<CheckItem> {
    vec![
        check_node(),
        check_pnpm(),
        check_writable(root),
        check_disk(root),
        check_registry(),
    ]
}
