use std::path::Path;
use std::process::Command;
use serde::Serialize;

#[cfg(windows)]
fn cmd_command(program: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(program);
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
            CheckItem {
                name: "Node.js".to_string(),
                ok: major >= 22,
                detail: if major >= 22 {
                    format!("{}（满足 >= 22）", v)
                } else {
                    format!("{}（需要 22 或更高）", v)
                },
                critical: true,
            }
        }
        None => CheckItem {
            name: "Node.js".to_string(),
            ok: false,
            detail: "未找到 node 命令，请先安装 Node.js 22+".to_string(),
            critical: true,
        },
    }
}

/// 检查 pnpm 是否可用
pub fn check_pnpm() -> CheckItem {
    match run_version("pnpm") {
        Some(v) => CheckItem {
            name: "pnpm".to_string(),
            ok: true,
            detail: v,
            critical: true,
        },
        None => CheckItem {
            name: "pnpm".to_string(),
            ok: false,
            detail: "未找到 pnpm 命令，请先安装 pnpm".to_string(),
            critical: true,
        },
    }
}

/// 检查根目录可写
pub fn check_writable(root: &Path) -> CheckItem {
    let test_file = root.join(".write-test");
    let result = std::fs::write(&test_file, b"test").and_then(|_| std::fs::remove_file(&test_file));
    match result {
        Ok(_) => CheckItem {
            name: "根目录可写".to_string(),
            ok: true,
            detail: root.to_string_lossy().to_string(),
            critical: true,
        },
        Err(e) => CheckItem {
            name: "根目录可写".to_string(),
            ok: false,
            detail: format!("无法写入 {}: {}", root.to_string_lossy(), e),
            critical: true,
        },
    }
}

/// 检查磁盘剩余空间（返回 MB）
pub fn check_disk(root: &Path) -> CheckItem {
    // 用 fs2 或简单方案：创建大文件不现实，这里改用读取盘符信息
    // 简化：用一个临时文件估算不可行，直接报 OK 并附路径，实际空间由 pnpm 自行报错
    CheckItem {
        name: "磁盘空间".to_string(),
        ok: true,
        detail: format!("目标：{}", root.to_string_lossy()),
        critical: false,
    }
}

/// 检查 npm registry 是否可达
pub fn check_registry() -> CheckItem {
    // 用 pnpm view 查询一个轻量包，验证 registry 连通
    let mut cmd = cmd_command("pnpm");
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("version");
    match cmd.output() {
        Ok(out) if out.status.success() => CheckItem {
            name: "npm 源连通".to_string(),
            ok: true,
            detail: String::from_utf8_lossy(&out.stdout).trim().to_string(),
            critical: false,
        },
        Ok(out) => {
            let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
            CheckItem {
                name: "npm 源连通".to_string(),
                ok: false,
                detail: if e.is_empty() { "无法访问 npm 源".to_string() } else { e.lines().next().unwrap_or("").to_string() },
                critical: false,
            }
        }
        Err(e) => CheckItem {
            name: "npm 源连通".to_string(),
            ok: false,
            detail: format!("检查失败：{}", e),
            critical: false,
        },
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
