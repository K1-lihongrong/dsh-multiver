use serde::Serialize;
use std::path::Path;
use std::process::Command;

/// 单个已安装版本的信息
#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    /// 版本号，如 0.1.5
    pub version: String,
    /// 版本目录的绝对路径
    pub path: String,
    /// 是否为当前默认版本
    pub is_default: bool,
    /// 是否开启了数据隔离
    pub isolated: bool,
    /// 安装日期（版本目录创建时间，格式 YYYY-MM-DD；读不到则为空）
    pub installed_at: String,
}

/// 在 Windows 上调用 pnpm 需要走 cmd，否则可能找不到 .cmd
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

/// 列出已安装的版本（扫描 versions 目录）
pub fn list(versions_dir: &Path, default_version: Option<&str>, isolated: &[String]) -> Vec<VersionInfo> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(versions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // 必须含有 node_modules 才算有效版本
            if !path.join("node_modules").exists() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().to_string();
            let is_default = default_version == Some(version.as_str());
            let is_isolated = isolated.iter().any(|v| v == &version);
            let installed_at = dir_created_date(&path);
            result.push(VersionInfo {
                version,
                path: path.to_string_lossy().to_string(),
                is_default,
                isolated: is_isolated,
                installed_at,
            });
        }
    }
    // 按版本号排序
    result.sort_by(|a, b| version_cmp(&a.version, &b.version));
    result
}

/// 读取目录创建时间并格式化为 YYYY-MM-DD（本地时区）。
/// 读不到（权限/文件系统不支持）则返回空字符串。
fn dir_created_date(path: &Path) -> String {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    let created = match meta.created() {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    // 转成本地时间：用 SystemTime -> 时区偏移需要外部 crate；
    // 这里用 std 自带方式（Windows 上 created() 已是本地时间的 UTC 表示），
    // 简化为按 UTC+8 处理（面向国内用户），偏移由 chrono 不存在，故用手工换算。
    format_date_local(created)
}

/// 把 SystemTime 格式化为本地日期（YYYY-MM-DD）。
/// 仅用于展示"安装日期"，对时区误差不敏感；按东八区（UTC+8）处理。
fn format_date_local(t: std::time::SystemTime) -> String {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => return String::new(),
    };
    let local = secs + 8 * 3600; // UTC+8
    let days = local.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 由"自 1970-01-01 起的天数"换算为 (年, 月, 日)。
/// 算法来自 Howard Hinnant 的 civil_from_days。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 版本号比较：把数字段拆开按数值比较
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<i64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|x| !x.is_empty())
            .filter_map(|x| x.parse::<i64>().ok())
            .collect()
    };
    parse(a).cmp(&parse(b))
}

/// 安装阶段
pub fn parse_stage(line: &str) -> Option<&'static str> {
    if line.contains("resolved") && line.contains("reused") {
        return Some("正在解析依赖...");
    }
    if line.starts_with("Packages:") || line.contains("Packages: +") {
        return Some("正在计算依赖树...");
    }
    if line.contains("Progress: resolved") && line.contains("downloaded") && !line.contains("added") {
        return Some("正在下载依赖包...");
    }
    if line.contains("Progress: resolved") && line.contains("added") {
        return Some("正在写入文件...");
    }
    if line.contains("node_modules/") && (line.contains("postinstall") || line.contains("preinstall") || line.contains("install:")) {
        return Some("正在构建原生模块...");
    }
    if line.contains("dependencies:") || line.contains("+ @deepseek-ai/dsh") {
        return Some("正在完成安装...");
    }
    if line.contains("Done in") {
        return Some("安装完成");
    }
    None
}

/// 安装指定版本。on_stage 用于推送阶段进度。返回 (是否成功, 消息)
pub fn install(
    versions_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    version: &str,
    on_stage: &dyn Fn(&str),
) -> (bool, String) {
    let target = versions_dir.join(version);
    if target.join("node_modules").exists() {
        return (false, format!("版本 {} 已安装", version));
    }
    if let Err(e) = std::fs::create_dir_all(&target) {
        return (false, format!("创建目录失败: {}", e));
    }
    // 写入一个最小的 package.json
    let pkg = format!(
        "{{\n  \"name\": \"dsh-{}\",\n  \"private\": true,\n  \"version\": \"0.0.0\"\n}}\n",
        version
    );
    if let Err(e) = std::fs::write(target.join("package.json"), pkg) {
        return (false, format!("写入 package.json 失败: {}", e));
    }
    // 关键：hoisted 链接模式。dsh 的运行时按 Node 常规方式解析依赖，
    // pnpm 默认的符号链接结构会导致 "Cannot find package" 错误。
    // hoisted 生成扁平化的真实 node_modules（兼容 dsh），
    // 同时底层仍用 store 硬链接，保持磁盘去重。
    if let Err(e) = std::fs::write(target.join(".npmrc"), "node-linker=hoisted
") {
        return (false, format!("写入 .npmrc 失败: {}", e));
    }

    let spec = format!("@deepseek-ai/dsh@{}", version);
    let mut cmd = pnpm_command();
    cmd.current_dir(&target)
        .arg("add")
        .arg(&spec)
        // 注意：pnpm add 不接受顶层 --state-dir（报 Unknown option），
        // 统一改用 --config.xxx=value 形式，既不报错也保持"不污染全局目录"的意图。
        .arg(format!("--config.store-dir={}", store_dir.to_string_lossy()))
        .arg(format!("--config.cache-dir={}", cache_dir.to_string_lossy()))
        .arg(format!("--config.state-dir={}", state_dir.to_string_lossy()))
        .arg("--config.confirmModulesPurge=false")
        .arg("--config.dangerouslyAllowAllBuilds=true")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    on_stage("正在启动 pnpm...");

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&target);
            return (false, format!("启动 pnpm 失败: {}", e));
        }
    };

    // 流式读取 stderr（pnpm 的进度输出走 stderr）
    let mut last_stage = String::new();
    let mut collected = String::new();
    if let Some(stderr) = child.stderr.take() {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            collected.push_str(&line);
            collected.push('\n');
            if let Some(stage) = parse_stage(&line) {
                if stage != last_stage {
                    last_stage = stage.to_string();
                    on_stage(stage);
                }
            }
        }
    }

    let status = child.wait();
    let ok = matches!(status, Ok(s) if s.success());
    if ok {
        on_stage("安装完成");
        (true, format!("已安装 {}", version))
    } else {
        // 失败时清理残缺目录
        let _ = std::fs::remove_dir_all(&target);
        (false, friendly_error(collected.trim()))
    }
}

/// 把原始错误转成更易懂的提示
fn friendly_error(raw: &str) -> String {
    if raw.contains("ERR_PNPM_NO_MATCHING_VERSION") || raw.contains("No matching version found") {
        format!(
            "该版本在官方源上发布不完整（依赖子包缺失），无法安装。\n建议换一个版本重试。\n\n原始信息：\n{}",
            raw
        )
    } else if raw.contains("ERR_PNPM_FETCH") || raw.contains("UND_ERR") || raw.contains("ETIMEDOUT") {
        format!("网络连接失败，请检查网络或稍后重试。\n\n原始信息：\n{}", raw)
    } else {
        format!("安装失败：\n{}", raw)
    }
}

/// 卸载指定版本
pub fn uninstall(versions_dir: &Path, version: &str) -> (bool, String) {
    let target = versions_dir.join(version);
    if !target.exists() {
        return (false, format!("版本 {} 不存在", version));
    }
    match std::fs::remove_dir_all(&target) {
        Ok(_) => (true, format!("已卸载 {}", version)),
        Err(e) => {
            let hint = if e.raw_os_error() == Some(32) {
                "\n\n可能该版本的 dsh 进程正在运行，持有文件。\n请先关闭对应的 dsh 窗口/终端，再重试。"
            } else {
                ""
            };
            (false, format!("卸载失败: {}{}", e, hint))
        }
    }
}

/// 查询某个版本是否有效
pub fn exists(versions_dir: &Path, version: &str) -> bool {
    versions_dir.join(version).join("node_modules").exists()
}

/// 递归统计目录大小（字节）。目录不存在返回 0。
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// 整个版本目录的大小（字节）。用于"占用大小"展示。
pub fn version_size(versions_dir: &Path, version: &str) -> u64 {
    dir_size(&versions_dir.join(version))
}

/// 隔离 home 的路径
pub fn isolated_home(versions_dir: &Path, version: &str) -> std::path::PathBuf {
    versions_dir.join(version).join("home")
}

/// 复制共享 home 到隔离 home（不覆盖已存在文件）
pub fn copy_shared_to_isolated(shared_home: &Path, isolated: &Path) -> (bool, String, u64) {
    let mut copied = 0u64;
    if let Err(e) = copy_dir_recursive(shared_home, isolated, &mut copied) {
        return (false, format!("复制失败: {}", e), copied);
    }
    (true, format!("已复制 {} 个文件", copied), copied)
}

fn copy_dir_recursive(src: &Path, dst: &Path, count: &mut u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst.join(&name);

        // 目标已存在则跳过（不覆盖）
        if dst_path.exists() {
            continue;
        }

        // 关键：保留 junction / symlink，而不是实体化。
        // 共享 home 的 node_modules 多为 JUNCTION（目录联接），朴素复制会把它
        // 实体化成普通目录，破坏 dsh 的模块代理机制（报 "exists and is not a
        // symlink or dsh-managed module proxy"）。这里在目标处**重建一个指向
        // 相同目标的 junction**，从而"复制共享数据"的同时保持模块可解析。
        if is_reparse_point(&src_path) {
            if let Some(target) = read_link_target(&src_path) {
                if create_junction(&dst_path, &target).is_ok() {
                    *count += 1;
                    continue;
                }
                // 建 junction 失败则退回普通处理（下面按目录/文件走）
            }
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, count)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
            *count += 1;
        }
    }
    Ok(())
}

/// 判断路径是否为 reparse point（junction 或 symlink）。
fn is_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        match std::fs::symlink_metadata(path) {
            Ok(md) => md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
    }
}

/// 读取 junction / symlink 指向的目标路径。
fn read_link_target(path: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_link(path).ok()
}

/// 在 Windows 上创建目录 junction（mklink /J），无需管理员权限。
/// 非 Windows 上退回 symlink_dir。
fn create_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // 分开传参给 cmd：cmd /C mklink /J <link> <target>。
        // 不要拼成单个字符串再 .arg()，否则 Rust 会二次转义导致路径解析失败。
        let out = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                String::from_utf8_lossy(&out.stderr).to_string(),
            ))
        }
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link)
    }
}

/// 清理隔离 home（删除目录内容但保留目录本身）
pub fn clear_isolated(versions_dir: &Path, version: &str) -> (bool, String) {
    let home = isolated_home(versions_dir, version);
    if !home.exists() {
        return (true, "隔离目录不存在，无需清理".to_string());
    }
    match std::fs::remove_dir_all(&home) {
        Ok(_) => {
            let _ = std::fs::create_dir_all(&home);
            (true, format!("已清理 {} 的隔离数据", version))
        }
        Err(e) => {
            let hint = if e.raw_os_error() == Some(32) {
                "\n\n可能该版本的 dsh 进程正在运行，持有数据文件。\n请先关闭对应的 dsh 窗口/终端，再重试。"
            } else {
                ""
            };
            (false, format!("清理失败: {}{}", e, hint))
        }
    }
}
