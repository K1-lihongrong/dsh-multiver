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
}

/// 安装进度事件（推给前端渲染阶段进度条 + 动态数字）。
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// 阶段序号（1-based）
    pub step: usize,
    /// 阶段总数（固定）
    pub total: usize,
    /// 阶段名（中文）
    pub stage: String,
    /// 阶段内动态细节（如 "已解析 234 个包"），可能为空
    pub detail: String,
    /// 当前阶段内的完成比例（0.0-1.0）。无法确定时为 0.0。
    /// 用于让进度条在阶段内平滑推进（如写入阶段按 added/总数）。
    pub fraction: f32,
}

/// 安装阶段序列（固定 8 步）。step 即阶段序号。
pub const INSTALL_TOTAL_STEPS: usize = 8;

/// 每个阶段名（下标 = step - 1）。
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

/// 构造一个 ProgressEvent。
fn progress(step: usize, detail: String, fraction: f32) -> ProgressEvent {
    ProgressEvent {
        step,
        total: INSTALL_TOTAL_STEPS,
        stage: STAGE_NAMES.get(step.saturating_sub(1)).copied().unwrap_or("").to_string(),
        detail,
        fraction: fraction.clamp(0.0, 1.0),
    }
}

/// 进度解析器（保存跨行状态，如依赖总数），供 install 逐行喂入。
pub struct ProgressParser {
    /// 从 `Packages: +N` 解析出的依赖总数，用于计算阶段内 fraction。
    total_packages: Option<u64>,
    /// 已到达的最大 step，用于防止阶段回退
    /// （pnpm 的 Packages/Progress 行交错出现会导致 step 反复横跳）。
    max_step: usize,
}

impl ProgressParser {
    pub fn new() -> Self {
        Self { total_packages: None, max_step: 0 }
    }

    /// 归一化 step：不允许小于已达到的最大值（防回退）。
    fn clamp_step(&mut self, step: usize) -> usize {
        if step > self.max_step {
            self.max_step = step;
        }
        self.max_step
    }

    /// 解析一行，返回 (step, detail, fraction)；无法识别返回 None。
    /// step 单调不减（防 pnpm 交错输出导致的回退）。
    pub fn parse(&mut self, line: &str) -> Option<(usize, String, f32)> {
        // 先算出本行的「原始 step 与 detail」，最后统一 clamp。
        let raw: Option<(usize, String, f32)> = 'p: {
            // Packages: +445  → 记下总数（本身不推进阶段，只更新总数）
            if line.starts_with("Packages:") {
                if let Some(n) = extract_num(line, "+") {
                    self.total_packages = Some(n);
                }
                // 不产生事件，避免与 Progress 行交错导致 step 回退
                break 'p None;
            }
            // Progress: resolved 234, reused 222, downloaded 5, added 0
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
                    // 解析阶段没有可靠总数（resolved 会超过实际包数），不细分
                    break 'p Some((4, format!("已解析 {} 个包", r), 0.0));
                }
                break 'p None;
            }
            // 原生模块构建
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

        // 归一化：step 不倒退
        raw.map(|(step, detail, frac)| {
            let s = self.clamp_step(step);
            (s, detail, frac)
        })
    }

    /// 当前数字 / 总数 → fraction（总数未知时返回 0.0）。
    fn frac(&self, cur: u64) -> f32 {
        match self.total_packages {
            Some(t) if t > 0 => (cur as f32 / t as f32).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
}

/// 从字符串里提取 `<key> <数字>` 的数字（key 如 "resolved"）。
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
            let is_default = default_version == Some(version.as_str());
            let is_isolated = isolated.iter().any(|v| v == &version);
            let installed_at = dir_created_date(&path);
            result.push(VersionInfo {
                version,
                path: path.to_string_lossy().to_string(),
                is_default,
                isolated: is_isolated,
                installed_at,
                shared_home: !is_isolated,
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

/// 安装阶段（旧版阶段文字；整合包分支仍在用，保留）
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_has_stage_names() {
        let p1 = progress(1, String::new(), 0.0);
        assert_eq!(p1.stage, "准备目录");
        assert_eq!(p1.step, 1);
        assert_eq!(p1.total, 8);
        let p8 = progress(8, String::new(), 1.0);
        assert_eq!(p8.stage, "完成");
        assert_eq!(p8.fraction, 1.0);
    }

    #[test]
    fn parses_pnpm_progress_lines() {
        let mut p = ProgressParser::new();
        // Packages 行不再产生事件，只记下总数
        assert!(p.parse("Packages: +445").is_none());
        // resolved 阶段
        let (s, d, _) = p.parse("Progress: resolved 234, reused 222, downloaded 0, added 0").unwrap();
        assert_eq!(s, 4);
        assert!(d.contains("234"), "detail={}", d);
        // downloaded 阶段
        let (s, d, _) = p.parse("Progress: resolved 234, reused 222, downloaded 5, added 0").unwrap();
        assert_eq!(s, 5);
        assert!(d.contains("5"));
        // added 阶段：fraction = 12/445
        let (s, d, f) = p.parse("Progress: resolved 234, reused 222, downloaded 5, added 12").unwrap();
        assert_eq!(s, 6);
        assert!(d.contains("12"));
        assert!((f - 12.0 / 445.0).abs() < 0.001, "frac={}", f);
        // Done
        let (s, _, _) = p.parse("Done in 47.6s using pnpm v11").unwrap();
        assert_eq!(s, 8);
    }

    #[test]
    fn step_never_goes_backwards() {
        let mut p = ProgressParser::new();
        // 先到 step 5（下载）
        let (s, _, _) = p.parse("Progress: resolved 100, reused 0, downloaded 5, added 0").unwrap();
        assert_eq!(s, 5);
        // 交错出现 resolved 行（raw step 4）——不应回退
        let (s2, _, _) = p.parse("Progress: resolved 200, reused 0, downloaded 0, added 0").unwrap();
        assert_eq!(s2, 5, "阶段不应回退");
        // 再到 step 6
        let (s3, _, _) = p.parse("Progress: resolved 200, reused 0, downloaded 5, added 10").unwrap();
        assert_eq!(s3, 6);
        // 又出现 resolved 行（raw 4）——仍不回退
        let (s4, _, _) = p.parse("Progress: resolved 300, reused 0, downloaded 0, added 0").unwrap();
        assert_eq!(s4, 6, "阶段不应回退");
    }

    #[test]
    fn extract_num_works() {
        assert_eq!(extract_num("resolved 234, reused 222", "resolved"), Some(234));
        assert_eq!(extract_num("resolved 234, reused 222", "reused"), Some(222));
        assert_eq!(extract_num("no number here", "xyz"), None);
    }

    #[test]
    fn classify_private_package() {
        let raw = "ERR_PNPM_FETCH_404  GET https://registry.npmjs.org/@deepseek-ai/dsh: Not Found - 404";
        assert_eq!(classify_error(raw), "private-package");
        // 需要同时含 @deepseek-ai 才归为 private-package
        assert_eq!(classify_error("ERR_PNPM_FETCH_404 other-pkg"), "network");
    }

    #[test]
    fn classify_incomplete_version() {
        assert_eq!(
            classify_error("ERR_PNPM_NO_MATCHING_VERSION No matching version found"),
            "incomplete-version"
        );
        assert_eq!(
            classify_error("No matching version found for x"),
            "incomplete-version"
        );
    }

    #[test]
    fn classify_network() {
        assert_eq!(classify_error("ERR_PNPM_FETCH something failed"), "network");
        assert_eq!(classify_error("connect ETIMEDOUT 1.2.3.4:443"), "network");
        assert_eq!(classify_error("ECONNRESET"), "network");
        assert_eq!(classify_error("getaddrinfo ENOTFOUND registry"), "network");
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify_error("some totally unrelated failure"), "unknown");
        assert_eq!(classify_error(""), "unknown");
    }

    // ── uninstall_fast：rename 到回收站 ──

    fn uninstall_tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-uninstall-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn uninstall_fast_moves_version_to_trash() {
        let base = uninstall_tmp("fast");
        let versions = base.join("versions");
        let trash = base.join("trash");
        // 造一个"版本目录"
        std::fs::create_dir_all(versions.join("0.1.0").join("node_modules")).unwrap();
        assert!(versions.join("0.1.0").exists());

        let (ok, msg) = uninstall_fast(&versions, &trash, "0.1.0");
        assert!(ok, "msg={}", msg);
        // 版本目录应立即消失
        assert!(!versions.join("0.1.0").exists(), "版本目录应已移走");
        // 回收站里应有一份（后台可能已删，故只验证"移走"而非"存在"）
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn uninstall_fast_missing_version_fails() {
        let base = uninstall_tmp("missing");
        let versions = base.join("versions");
        let trash = base.join("trash");
        std::fs::create_dir_all(&versions).unwrap();
        let (ok, msg) = uninstall_fast(&versions, &trash, "9.9.9");
        assert!(!ok);
        assert!(msg.contains("不存在"), "msg={}", msg);
        let _ = std::fs::remove_dir_all(&base);
    }
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

    // pnpm 的 Progress 行实际走 **stdout**（实测），错误信息走 stderr。
    // 两个流都必须读（否则管道填满会假死），且都要解析进度。
    // on_progress 是 &dyn Fn（非 Send），只能在主线程调用——
    // 故两个读线程通过 channel 把「行」发回主线程，主线程统一解析 + 回调。
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

    // 主线程消费两个流（所有 sender drop 后 rx 循环自然结束）
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

/// 错误分类（供前端决定弹哪种提示）。
pub fn classify_error(raw: &str) -> &'static str {
    // 私有包下架：ERR_PNPM_FETCH_404 且涉及 @deepseek-ai/*
    if (raw.contains("ERR_PNPM_FETCH_404") || raw.contains("Not Found - 404"))
        && raw.contains("@deepseek-ai")
    {
        return "private-package";
    }
    // 版本不完整（子包缺失）
    if raw.contains("ERR_PNPM_NO_MATCHING_VERSION") || raw.contains("No matching version found") {
        return "incomplete-version";
    }
    // 网络类
    if raw.contains("ERR_PNPM_FETCH")
        || raw.contains("UND_ERR")
        || raw.contains("ETIMEDOUT")
        || raw.contains("ECONNRESET")
        || raw.contains("ENOTFOUND")
    {
        return "network";
    }
    "unknown"
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

/// 快速卸载：把版本目录 **rename** 到回收站（瞬时），实际删除交给后台。
///
/// Windows 上 `remove_dir_all` 删 26000 个文件可能要几十秒（NTFS + Defender），
/// 用户盯着"卸载中..."干等。rename 只改目录项，**瞬时完成**；真正的删除放到后台，
/// 由 `maintenance::cleanup_trash`（启动时）或本函数的后台线程负责。
///
/// 返回：(是否成功, 消息)。成功时版本目录已"消失"（rename 到了 trash）。
/// 若 rename 失败（跨卷/被占用），回退到直接 `remove_dir_all`。
pub fn uninstall_fast(versions_dir: &Path, trash_dir: &Path, version: &str) -> (bool, String) {
    let target = versions_dir.join(version);
    if !target.exists() {
        return (false, format!("版本 {} 不存在", version));
    }

    // 确保回收站存在
    let _ = std::fs::create_dir_all(trash_dir);
    // 唯一名：<版本>-<时间戳>-<pid>，避免同名冲突
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = trash_dir.join(format!("{}-{}-{}", version, stamp, std::process::id()));

    match std::fs::rename(&target, &dest) {
        Ok(_) => {
            // 后台线程慢慢删回收站里的这份（不阻塞命令返回）
            std::thread::spawn(move || {
                let _ = std::fs::remove_dir_all(&dest);
            });
            (true, format!("已卸载 {}", version))
        }
        Err(_) => {
            // rename 失败（跨卷等）→ 回退直接删（可能慢，但保证功能）
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

/// 版本占用的详细信息：逻辑总量 + 与 pnpm store 共享（硬链接）的部分。
#[derive(Debug, Clone, Serialize)]
pub struct SizeInfo {
    /// 逻辑总大小（所有文件按自身大小累加，硬链接会重复计）
    pub total: u64,
    /// 与其他版本共享（nlink > 1）的文件大小之和
    pub shared_size: u64,
    /// 共享文件的数量
    pub shared_count: u64,
    /// 独占大小 = total - shared_size
    pub exclusive_size: u64,
}

/// 读取文件的硬链接数（nlink）。读不到时保守返回 1（视为不共享）。
#[cfg(windows)]
fn file_nlink(path: &Path) -> u32 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let handle = f.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    if ok != 0 && info.nNumberOfLinks > 0 {
        info.nNumberOfLinks
    } else {
        1
    }
}

#[cfg(unix)]
fn file_nlink(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| m.nlink() as u32).unwrap_or(1)
}

#[cfg(not(any(windows, unix)))]
fn file_nlink(_path: &Path) -> u32 {
    1
}

/// 递归统计目录：总量 + 共享(硬链接)部分。用于"占用大小"展示复用情况。
fn dir_size_detail(path: &Path, acc: &mut SizeInfo) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                dir_size_detail(&p, acc);
            } else if let Ok(meta) = entry.metadata() {
                let len = meta.len();
                acc.total += len;
                if file_nlink(&p) > 1 {
                    acc.shared_size += len;
                    acc.shared_count += 1;
                }
            }
        }
    }
}

/// 整个版本目录的占用详情（含共享/独占）。用于"扫描占用"。
pub fn version_size_detail(versions_dir: &Path, version: &str) -> SizeInfo {
    let mut info = SizeInfo { total: 0, shared_size: 0, shared_count: 0, exclusive_size: 0 };
    dir_size_detail(&versions_dir.join(version), &mut info);
    info.exclusive_size = info.total.saturating_sub(info.shared_size);
    info
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
