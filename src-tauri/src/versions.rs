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
}

/// 执行命令并返回 (是否成功, stdout, stderr)
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
            result.push(VersionInfo {
                version,
                path: path.to_string_lossy().to_string(),
                is_default,
                isolated: is_isolated,
            });
        }
    }
    // 按版本号排序
    result.sort_by(|a, b| version_cmp(&a.version, &b.version));
    result
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
        .arg("--store-dir")
        .arg(store_dir)
        .arg("--cache-dir")
        .arg(cache_dir)
        .arg("--state-dir")
        .arg(state_dir)
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
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, count)?;
        } else {
            // 不覆盖已存在文件
            if !dst_path.exists() {
                std::fs::copy(&src_path, &dst_path)?;
                *count += 1;
            }
        }
    }
    Ok(())
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
