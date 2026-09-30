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
    /// 是否使用共享 home（= 未开启隔离）。
    /// 非隔离版本共用 `<根>/home`，多版本混用可能导致插件/依赖版本错配
    /// （如 dsh 0.1.7 的 open-in-app 失效会连累旧版本），UI 应提示。
    pub shared_home: bool,

    // ---- 整合包实例专属（kind == "modpack" 时有值）----
    /// "version" | "modpack"
    pub kind: String,
    pub modpack_name: Option<String>,
    pub modpack_version: Option<String>,
    pub modpack_display_name: Option<String>,
    pub modpack_description: Option<String>,
    pub modpack_author: Option<String>,
    pub modpack_icon: Option<String>,
    pub packed_dsh_version: Option<String>,
    pub modpack_type: Option<String>,
    pub bundle_count: Option<usize>,
    pub skill_count: Option<usize>,
    pub launch_profile: Option<String>,
}

/// 安装进度事件（推给前端渲染阶段进度条 + 动态数字）。
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub step: usize,
    pub total: usize,
    pub stage: String,
    pub detail: String,
    pub fraction: f32,
    /// 来源："version-install"（安装 dsh 版本）| "modpack-import"（导入整合包）
    #[serde(default = "default_kind")]
    pub kind: String,
}

#[allow(dead_code)]
fn default_kind() -> String {
    "version-install".to_string()
}

/// 安装阶段序列（固定 8 步）。
pub const INSTALL_TOTAL_STEPS: usize = 8;

const STAGE_NAMES: [&str; INSTALL_TOTAL_STEPS] = [
    "准备目录",
    "写入配置",
    "启动 pnpm",
    "解析依赖",
    "下载依赖",
    "写入文件",
    "构建原生模块",
    "完成",
];

/// 构造一个 ProgressEvent（版本安装来源）。
fn progress(step: usize, detail: String, fraction: f32) -> ProgressEvent {
    ProgressEvent {
        step,
        total: INSTALL_TOTAL_STEPS,
        stage: STAGE_NAMES.get(step.saturating_sub(1)).copied().unwrap_or("").to_string(),
        detail,
        fraction: fraction.clamp(0.0, 1.0),
        kind: "version-install".to_string(),
    }
}

/// 构造一个「整合包导入」来源的 ProgressEvent（供 modpack 复用同一前端组件）。
/// `stage` 直接给中文阶段名；`percent` 0-100（该阶段内）。
pub fn import_progress(stage: &str, detail: String, percent: f32) -> ProgressEvent {
    let p = percent.clamp(0.0, 100.0);
    ProgressEvent {
        step: 0,
        total: 0,
        stage: stage.to_string(),
        detail,
        fraction: (p / 100.0).clamp(0.0, 1.0),
        kind: "modpack-import".to_string(),
    }
}

/// 整合包导入：把 `ProgressParser` 的结果转成阶段名 + 百分比。
/// 阶段映射（导入流程专属）：
/// - 4 解析依赖 → 20%
/// - 5 下载依赖 → 25-45%
/// - 6 写入文件 → 45-95%
/// - 7 构建原生模块 → 95%
/// - 8 完成 → 100%
pub fn import_progress_from_line(parser: &mut ProgressParser, line: &str) -> Option<ProgressEvent> {
    let (step, detail, frac) = parser.parse(line)?;
    let (stage, base, span) = match step {
        4 => ("正在解析整合包依赖", 20.0, 5.0),
        5 => ("正在下载整合包依赖", 25.0, 20.0),
        6 => ("正在写入整合包依赖", 45.0, 50.0),
        7 => ("正在构建原生模块", 95.0, 3.0),
        _ => ("正在安装整合包依赖", 20.0, 75.0),
    };
    let percent = base + span * frac;
    Some(import_progress(stage, detail, percent))
}

/// 进度解析器（保存跨行状态，如依赖总数），供 install 逐行喂入。
pub struct ProgressParser {
    total_packages: Option<u64>,
    max_step: usize,
}

impl ProgressParser {
    pub fn new() -> Self {
        Self { total_packages: None, max_step: 0 }
    }

    fn clamp_step(&mut self, step: usize) -> usize {
        if step > self.max_step {
            self.max_step = step;
        }
        self.max_step
    }

    /// 解析一行，返回 (step, detail, fraction)；step 单调不减。
    pub fn parse(&mut self, line: &str) -> Option<(usize, String, f32)> {
        let raw: Option<(usize, String, f32)> = 'p: {
            if line.starts_with("Packages:") {
                if let Some(n) = extract_num(line, "+") {
                    self.total_packages = Some(n);
                }
                break 'p None;
            }
            if let Some(rest) = line.strip_prefix("Progress:") {
                let resolved = extract_num(rest, "resolved");
                let downloaded = extract_num(rest, "downloaded");
                let added = extract_num(rest, "added");
                if let Some(a) = added {
                    if a > 0 {
                        break 'p Some((6, format!("已写入 {} 个包", a), self.frac(a)));
                    }
                }
                if let Some(d) = downloaded {
                    if d > 0 {
                        break 'p Some((5, format!("已下载 {} 个包", d), self.frac(d)));
                    }
                }
                if let Some(r) = resolved {
                    break 'p Some((4, format!("已解析 {} 个包", r), 0.0));
                }
                break 'p None;
            }
            if line.contains("node_modules/")
                && (line.contains("postinstall") || line.contains("preinstall") || line.contains("install:"))
            {
                break 'p Some((7, String::new(), 0.0));
            }
            if line.contains("Done in") {
                break 'p Some((8, String::new(), 0.0));
            }
            None
        };
        raw.map(|(step, detail, frac)| {
            let s = self.clamp_step(step);
            (s, detail, frac)
        })
    }

    fn frac(&self, cur: u64) -> f32 {
        match self.total_packages {
            Some(t) if t > 0 => (cur as f32 / t as f32).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
}

/// 从字符串里提取 `<key> <数字>` 的数字。
fn extract_num(s: &str, key: &str) -> Option<u64> {
    let idx = s.find(key)?;
    let after = &s[idx + key.len()..];
    let digits: String = after
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}
/// 在 Windows 上调用 pnpm 需要走 cmd，否则可能找不到 .cmd
#[cfg(windows)]
fn pnpm_command() -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new("cmd");
    c.arg("/C").arg("pnpm");
    // 静默运行，不弹终端黑框（安装时界面已有阶段式进度提示）
    c.creation_flags(CREATE_NO_WINDOW);
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
            let is_modpack = version.starts_with("modpack-");
            let is_default = default_version == Some(version.as_str());
            // 整合包实例天然隔离（有自己专属的 home）
            let is_isolated = is_modpack || isolated.iter().any(|v| v == &version);
            let installed_at = dir_created_date(&path);

            if is_modpack {
                // 读标记文件填充整合包元数据；读不到则降级为最小信息
                let meta = crate::modpack::read_meta(&path);
                result.push(VersionInfo {
                    version,
                    path: path.to_string_lossy().to_string(),
                    is_default: false,
                    isolated: true,
                    installed_at,
                    shared_home: false,
                    kind: "modpack".to_string(),
                    modpack_name: meta.as_ref().map(|m| m.modpack_name.clone()),
                    modpack_version: meta.as_ref().map(|m| m.modpack_version.clone()),
                    modpack_display_name: meta.as_ref().map(|m| m.display_name.clone()),
                    modpack_description: meta.as_ref().map(|m| m.description.clone()),
                    modpack_author: meta.as_ref().map(|m| m.author.clone()),
                    modpack_icon: meta.as_ref().map(|m| m.icon.clone()),
                    packed_dsh_version: meta.as_ref().map(|m| m.packed_dsh_version.clone()),
                    modpack_type: meta.as_ref().map(|m| m.modpack_type.clone()),
                    bundle_count: meta.as_ref().map(|m| m.bundle_count),
                    skill_count: meta.as_ref().map(|m| m.skill_count),
                    launch_profile: meta.as_ref().map(|m| m.launch_profile.clone()),
                });
            } else {
                result.push(VersionInfo {
                    version,
                    path: path.to_string_lossy().to_string(),
                    is_default,
                    isolated: is_isolated,
                    installed_at,
                    shared_home: !is_isolated,
                    kind: "version".to_string(),
                    modpack_name: None,
                    modpack_version: None,
                    modpack_display_name: None,
                    modpack_description: None,
                    modpack_author: None,
                    modpack_icon: None,
                    packed_dsh_version: None,
                    modpack_type: None,
                    bundle_count: None,
                    skill_count: None,
                    launch_profile: None,
                });
            }
        }
    }
    // 按版本号排序
    result.sort_by(|a, b| version_cmp(&a.version, &b.version));
    result
}

/// 列出整合包实例（扫描 modpacks 目录）。
///
/// 实例全部视为 kind="modpack"，天然隔离（各有专属 home），不参与「默认版本」。
pub fn list_modpacks(modpacks_dir: &Path) -> Vec<VersionInfo> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(modpacks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // 必须含有 node_modules 才算有效实例
            if !path.join("node_modules").exists() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().to_string();
            let meta = crate::modpack::read_meta(&path);
            result.push(VersionInfo {
                version,
                path: path.to_string_lossy().to_string(),
                is_default: false,
                isolated: true,
                installed_at: dir_created_date(&path),
                shared_home: false,
                kind: "modpack".to_string(),
                modpack_name: meta.as_ref().map(|m| m.modpack_name.clone()),
                modpack_version: meta.as_ref().map(|m| m.modpack_version.clone()),
                modpack_display_name: meta.as_ref().map(|m| m.display_name.clone()),
                modpack_description: meta.as_ref().map(|m| m.description.clone()),
                modpack_author: meta.as_ref().map(|m| m.author.clone()),
                modpack_icon: meta.as_ref().map(|m| m.icon.clone()),
                packed_dsh_version: meta.as_ref().map(|m| m.packed_dsh_version.clone()),
                modpack_type: meta.as_ref().map(|m| m.modpack_type.clone()),
                bundle_count: meta.as_ref().map(|m| m.bundle_count),
                skill_count: meta.as_ref().map(|m| m.skill_count),
                launch_profile: meta.as_ref().map(|m| m.launch_profile.clone()),
            });
        }
    }
    result
}

/// 版本号比较（供外部合并排序用）。
pub fn version_cmp_pub(a: &str, b: &str) -> std::cmp::Ordering {
    version_cmp(a, b)
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

/// 安装指定版本。on_progress 用于推送阶段进度。返回 (是否成功, 消息)
pub fn install(
    versions_dir: &Path,
    store_dir: &Path,
    cache_dir: &Path,
    state_dir: &Path,
    version: &str,
    on_progress: &dyn Fn(&ProgressEvent),
    registry: Option<&str>,
) -> (bool, String) {
    let target = versions_dir.join(version);
    if target.join("node_modules").exists() {
        return (false, format!("版本 {} 已安装", version));
    }
    on_progress(&progress(1, String::new(), 0.0));
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

    on_progress(&progress(2, String::new(), 0.0));

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
        .arg("--config.dangerouslyAllowAllBuilds=true");
    // 可选：自定义 npm 源（换源重试用）
    if let Some(reg) = registry {
        if !reg.trim().is_empty() {
            cmd.arg(format!("--config.registry={}", reg.trim()));
        }
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    on_progress(&progress(3, String::new(), 0.0));

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&target);
            return (false, format!("启动 pnpm 失败: {}", e));
        }
    };

    // pnpm 的 Progress 行走 stdout，错误走 stderr。两流都读（防管道填满假死），
    // 通过 channel 把行发回主线程统一解析（on_progress 是 &dyn Fn，非 Send）。
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_out = tx.clone();
    let h_out = child.stdout.take().map(move |stdout| {
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_out.send(line);
            }
        })
    });
    let h_err = child.stderr.take().map(move |stderr| {
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
    });

    let mut last_step = 0usize;
    let mut last_detail = String::new();
    let mut last_frac = -1.0f32;
    let mut collected = String::new();
    let mut parser = ProgressParser::new();
    for line in rx {
        collected.push_str(&line);
        collected.push('\n');
        if let Some((step, detail, frac)) = parser.parse(&line) {
            let advanced = step > last_step;
            let detail_changed = detail != last_detail;
            let frac_moved = (frac - last_frac).abs() >= 0.01;
            if advanced || detail_changed || frac_moved {
                last_step = step;
                last_detail = detail.clone();
                last_frac = frac;
                on_progress(&progress(step, detail, frac));
            }
        }
    }
    if let Some(h) = h_out { let _ = h.join(); }
    if let Some(h) = h_err { let _ = h.join(); }

    let status = child.wait();
    let ok = matches!(status, Ok(s) if s.success());
    if ok {
        on_progress(&progress(INSTALL_TOTAL_STEPS, String::new(), 1.0));
        (true, format!("已安装 {}", version))
    } else {
        // 失败时清理残缺目录
        let _ = std::fs::remove_dir_all(&target);
        (false, friendly_error(collected.trim()))
    }
}

/// 把原始错误转成更易懂的提示。
///
/// 复用整合包模块的统一诊断器，保证版本安装与整合包导入的错误文案一致。
fn friendly_error(raw: &str) -> String {
    let (_, msg) = crate::modpack::diagnose(raw);
    msg
}

/// 错误分类（供前端决定弹哪种提示）。
///
/// 复用整合包模块的统一诊断器（modpack::diagnose），把 DiagKind 映射为
/// 前端期望的四类字符串；另补一条「官方子包 404 下架」判定（diagnose 未覆盖）。
pub fn classify_error(raw: &str) -> &'static str {
    // 官方子包下架：404 且涉及 @deepseek-ai（优先判定，diagnose 未覆盖此情形）
    let lower = raw.to_lowercase();
    if (lower.contains("err_pnpm_fetch_404") || lower.contains("not found - 404"))
        && lower.contains("@deepseek-ai")
    {
        return "private-package";
    }
    match crate::modpack::diagnose(raw).0 {
        crate::modpack::DiagKind::Network => "network",
        crate::modpack::DiagKind::DshRuntime => "incomplete-version",
        crate::modpack::DiagKind::PluginDep => "incomplete-version",
        _ => "unknown",
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
pub(crate) fn create_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // 分开传参给 cmd：cmd /C mklink /J <link> <target>。
        // 不要拼成单个字符串再 .arg()，否则 Rust 会二次转义导致路径解析失败。
        //
        // 注意：mklink 是 cmd 内建命令，对**正斜杠路径**解析异常（会把 E:/x 当参数切分），
        // 因此这里统一把路径分隔符换成反斜杠。
        let link_s = link.to_string_lossy().replace('/', "\\");
        let target_s = target.to_string_lossy().replace('/', "\\");
        let out = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link_s)
            .arg(&target_s)
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
