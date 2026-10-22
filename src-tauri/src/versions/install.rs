//! 安装 / 卸载 / 错误分类（D5 拆分：原 versions.rs 的 install 相关）。
use serde::Serialize;
use std::path::Path;
use std::process::Command;

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
pub(crate) fn progress(step: usize, detail: String, fraction: f32) -> ProgressEvent {
    ProgressEvent {
        step,
        total: INSTALL_TOTAL_STEPS,
        stage: STAGE_NAMES
            .get(step.saturating_sub(1))
            .copied()
            .unwrap_or("")
            .to_string(),
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

impl Default for ProgressParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser {
    pub fn new() -> Self {
        Self {
            total_packages: None,
            max_step: 0,
        }
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
                && (line.contains("postinstall")
                    || line.contains("preinstall")
                    || line.contains("install:"))
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
pub(crate) fn extract_num(s: &str, key: &str) -> Option<u64> {
    let idx = s.find(key)?;
    let after = &s[idx + key.len()..];
    let digits: String = after
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 按字节把流切成行（保留不含换行符的原始字节）。
///
/// D16：不能用 `BufRead::lines()`——它按 UTF-8 解析，遇非法字节会**整行丢弃**，
/// 中文 Windows 下 pnpm 的报错行会直接消失。这里只按 \n 切分，解码交给调用方。
fn read_lines_bytes<R: std::io::Read>(mut reader: R) -> Vec<Vec<u8>> {
    let mut buf = Vec::new();
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        for &b in &chunk[..n] {
            if b == b'\n' {
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                out.push(std::mem::take(&mut buf));
            } else {
                buf.push(b);
            }
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
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

/// 安装阶段（旧版阶段文字；整合包分支仍在用，保留）
#[allow(dead_code)]
pub fn parse_stage(line: &str) -> Option<&'static str> {
    if line.contains("resolved") && line.contains("reused") {
        return Some("正在解析依赖...");
    }
    if line.starts_with("Packages:") || line.contains("Packages: +") {
        return Some("正在计算依赖树...");
    }
    if line.contains("Progress: resolved") && line.contains("downloaded") && !line.contains("added")
    {
        return Some("正在下载依赖包...");
    }
    if line.contains("Progress: resolved") && line.contains("added") {
        return Some("正在写入文件...");
    }
    if line.contains("node_modules/")
        && (line.contains("postinstall")
            || line.contains("preinstall")
            || line.contains("install:"))
    {
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
    if let Err(e) = std::fs::write(
        target.join(".npmrc"),
        "node-linker=hoisted
",
    ) {
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
        .arg(format!(
            "--config.store-dir={}",
            store_dir.to_string_lossy()
        ))
        .arg(format!(
            "--config.cache-dir={}",
            cache_dir.to_string_lossy()
        ))
        .arg(format!(
            "--config.state-dir={}",
            state_dir.to_string_lossy()
        ))
        .arg("--config.confirmModulesPurge=false")
        .arg("--config.dangerouslyAllowAllBuilds=true");
    // 可选：自定义 npm 源（换源重试用）
    if let Some(reg) = registry {
        if !reg.trim().is_empty() {
            cmd.arg(format!("--config.registry={}", reg.trim()));
        }
    }
    // 运行时适配（v0.2.9）：若系统缺 Node/pnpm，把便携运行时目录注入子进程 PATH，
    // 使 pnpm 能用便携 Node 运行（root 由 versions_dir 的父目录推出）。
    if let Some(root) = versions_dir.parent() {
        if let Some(new_path) = crate::runtime::path_env_additions(root) {
            cmd.env("PATH", new_path);
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
    // D16：不能用 BufRead::lines()——它按 UTF-8 解析，遇到 GBK 字节会整行丢弃
    // （中文 Windows 下 pnpm 的报错行直接消失）。改为按字节读到 \n，再用统一解码。
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_out = tx.clone();
    let h_out = child.stdout.take().map(move |stdout| {
        std::thread::spawn(move || {
            for line in read_lines_bytes(stdout) {
                let _ = tx_out.send(crate::console_decode::decode_console(&line));
            }
        })
    });
    let h_err = child.stderr.take().map(move |stderr| {
        std::thread::spawn(move || {
            for line in read_lines_bytes(stderr) {
                let _ = tx.send(crate::console_decode::decode_console(&line));
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
    if let Some(h) = h_out {
        let _ = h.join();
    }
    if let Some(h) = h_err {
        let _ = h.join();
    }

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
            "该版本依赖的子包在当前 npm 源缺失（可能是镜像源未同步，或版本发布不完整），无法安装。\n可切换官方源重试；若仍失败请换一个版本。\n\n原始信息：\n{}",
            raw
        )
    } else if raw.contains("ERR_PNPM_FETCH") || raw.contains("UND_ERR") || raw.contains("ETIMEDOUT")
    {
        format!(
            "网络连接失败，请检查网络或稍后重试。\n\n原始信息：\n{}",
            raw
        )
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

#[cfg(test)]
mod read_lines_bytes_tests {
    use super::read_lines_bytes;

    #[test]
    fn splits_on_lf() {
        let got = read_lines_bytes(&b"a\nb\nc"[..]);
        assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn strips_crlf() {
        let got = read_lines_bytes(&b"a\r\nb\r\n"[..]);
        assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[test]
    fn keeps_trailing_line_without_newline() {
        let got = read_lines_bytes(&b"a\nb"[..]);
        assert_eq!(got, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[test]
    fn preserves_non_utf8_bytes() {
        // GBK 的 "安装失败"，必须原样保留，不丢行
        let raw: &[u8] = &[0xB0, 0xB2, 0xD7, 0xB0, 0xCA, 0xA7, 0xB0, 0xDC];
        let got = read_lines_bytes(raw);
        assert_eq!(got, vec![raw.to_vec()]);
    }

    #[test]
    fn empty_input_yields_no_lines() {
        assert!(read_lines_bytes(&b""[..]).is_empty());
    }

    #[test]
    fn blank_line_is_kept() {
        let got = read_lines_bytes(&b"a\n\nb"[..]);
        assert_eq!(got, vec![b"a".to_vec(), Vec::new(), b"b".to_vec()]);
    }
}
