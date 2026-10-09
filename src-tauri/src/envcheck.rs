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

/// 解析版本号为 (major, minor, patch)，如 "v22.19.0" -> (22, 19, 0)。
/// 缺失的段按 0 处理（"22" -> (22, 0, 0)）。
fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut it = cleaned.split('.');
    let major = it.next()?.parse::<u32>().ok()?;
    let minor = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// dsh 需要的 Node 版本：`^22.19.0` 或 `>=24`。
///
/// 为什么不是简单的 ">= 22"：dsh 的入口用了 `import.meta.main`（Node 22.18/22.19 一线
/// 回溯支持、24.2 正式引入）。**22.16 这类">= 22 但不含该特性"的版本会静默失败**——
/// `dsh` 零输出、exit 0，表现为内嵌窗口连不上。故下限收紧到 22.19。
fn node_version_ok((major, minor, _patch): (u32, u32, u32)) -> bool {
    (major == 22 && minor >= 19) || major >= 24
}

/// 特性探测：当前 node 是否支持 `import.meta.main`（dsh 启动的必要条件）。
///
/// 返回 Some(true)=支持 / Some(false)=不支持 / None=探测失败（不阻塞，仅作参考）。
/// 版本号可能骗人（发行版 backport / 自定义构建），故实际能力比版本号更可靠。
fn probe_import_meta_main() -> Option<bool> {
    let mut cmd = cmd_command("node");
    cmd.arg("-e").arg("console.log(typeof import.meta.main)");
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            match s.as_str() {
                "boolean" => Some(true),
                "undefined" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

/// 检查 Node.js 是否可用，且版本与能力都满足 dsh 的要求。
///
/// 两层判断：
/// 1. **版本号**（快速拦截）—— 需 `^22.19.0` 或 `>=24`
/// 2. **特性探测**（兜底）—— 实测 `import.meta.main` 是否可用；仅在**版本号通过但探测失败**时报警
pub fn check_node() -> CheckItem {
    match run_version("node") {
        Some(v) => {
            let ok_ver = parse_semver(&v).map(node_version_ok).unwrap_or(false);
            if !ok_ver {
                return fail_item(
                    "Node.js",
                    format!("{}（需要 22.19+ 或 24+）", v),
                    true,
                    "dsh 需要 Node.js 22.19 或更高（或 24+）。旧版 22.x 缺少 dsh 依赖的 \"import.meta.main\" 特性，会导致内嵌窗口连不上。请到官网下载 LTS 版（22.19+ / 24+）安装后重新检查。",
                    "https://nodejs.org/zh-cn/download",
                );
            }
            // 版本号通过 → 特性探测兜底
            match probe_import_meta_main() {
                Some(false) => fail_item(
                    "Node.js",
                    format!("{}（版本看似满足，但缺少 import.meta.main 特性）", v),
                    true,
                    "当前 Node 虽满足版本号要求，但实测不支持 dsh 依赖的 \"import.meta.main\"（可能是定制构建或异常版本）。建议改用 Node.js 官网的 LTS 版（22.19+ / 24+）。",
                    "https://nodejs.org/zh-cn/download",
                ),
                _ => ok_item("Node.js", format!("{}（满足 22.19+ / 24+）", v), true),
            }
        }
        None => fail_item(
            "Node.js",
            "未找到 node 命令，请先安装 Node.js 22.19+（或 24+）".to_string(),
            true,
            "请到 Node.js 官网下载 LTS 版（22.19 或更高，或 24+）安装。安装时勾选「Add to PATH」，装完重开本工具。",
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

/// 检查数据根所在磁盘的剩余空间。
///
/// 返回可用空间（人类可读）。查不到时降级为"未知"（非致命，不误报失败）。
pub fn check_disk(root: &Path) -> CheckItem {
    match free_space(root) {
        Some(bytes) => ok_item(
            "磁盘空间",
            format!("可用 {}", human_bytes(bytes)),
            false,
        ),
        None => ok_item(
            "磁盘空间",
            format!("无法获取（目标 {}）", root.to_string_lossy()),
            false,
        ),
    }
}

/// 查询指定路径所在卷的可用字节数（Windows / Unix）。
#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut free as *mut u64,
        )
    };
    if ok != 0 { Some(free) } else { None }
}

#[cfg(unix)]
fn free_space(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if rc == 0 {
        Some(st.f_bavail as u64 * st.f_frsize as u64)
    } else {
        None
    }
}

#[cfg(not(any(windows, unix)))]
fn free_space(_path: &Path) -> Option<u64> {
    None
}

/// 字节数 -> 人类可读（GB / MB，保留一位小数）。
fn human_bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else {
        format!("{:.0} MB", f / MB)
    }
}

/// 检查 npm registry 是否可达
pub fn check_registry() -> CheckItem {
    // 用 pnpm view 查询一个轻量包，验证 registry 连通
    let mut cmd = cmd_command("pnpm");
    cmd.arg("view").arg("@deepseek-ai/dsh").arg("version");
    match cmd.output() {
        Ok(out) if out.status.success() => {
            // pnpm view 返回的是最新版本号；包成"可达（最新 X）"让用户明白查到了什么
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let detail = if ver.is_empty() {
                "可达".to_string()
            } else {
                format!("可达（最新 {}）", ver)
            };
            ok_item("npm 源连通", detail, false)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats_gb_and_mb() {
        assert_eq!(human_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(human_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
        assert_eq!(human_bytes(128u64 * 1024 * 1024 * 1024), "128.0 GB");
    }

    #[test]
    fn parse_semver_basic() {
        assert_eq!(parse_semver("v22.19.0"), Some((22, 19, 0)));
        assert_eq!(parse_semver("24.2.0"), Some((24, 2, 0)));
        assert_eq!(parse_semver("22"), Some((22, 0, 0)));
        assert_eq!(parse_semver("v20.19.2"), Some((20, 19, 2)));
    }

    /// 核心回归：22.16 不满足（旧阈值 22 会误放行）。
    #[test]
    fn node_version_requires_22_19_or_24() {
        // 22.x 需次版本 >= 19
        assert!(!node_version_ok((22, 0, 0)));
        assert!(!node_version_ok((22, 16, 0)));   // ← 本次修复的关键用例
        assert!(!node_version_ok((22, 18, 0)));
        assert!(node_version_ok((22, 19, 0)));
        assert!(node_version_ok((22, 20, 1)));
        // 23 处于 22 与 24 之间，按 ^22.19 || >=24 的语义**不满足**
        assert!(!node_version_ok((23, 5, 0)));
        // 24+ 任意次版本均可
        assert!(node_version_ok((24, 0, 0)));
        assert!(node_version_ok((24, 2, 0)));
        assert!(node_version_ok((25, 0, 0)));
    }

    #[test]
    fn parse_semver_rejects_garbage() {
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("not-a-version"), None);
    }
}
