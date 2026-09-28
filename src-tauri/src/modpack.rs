//! DSH 整合包（.dspack）支持。
//!
//! 一期实现「导入」：容器解包、manifest 解析校验、四阶段导入与回滚。
//! 协议见 DSH-PackForge（`protocol-2026-09`），方案见项目文档
//! `.cuckooCode/兼容DSH-PackForge-方案.md`。
//!
//! 本模块第一步只做：容器识别（`dspack.json`）与解包。

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `.dspack` 根部的容器标记文件内容。
#[derive(Debug, Clone, Deserialize)]
pub struct DspackMarker {
    pub format: String,
    pub version: u32,
}

/// 支持的容器版本：现行 v3，兼容 v2（manifest v4）。
pub const SUPPORTED_CONTAINER_VERSIONS: &[u32] = &[2, 3];

/// 解包并预检的结果。此时尚未落地任何用户文件。
#[derive(Debug)]
pub struct Extracted {
    /// 解包到的临时目录（调用方负责在结束后清理）
    pub dir: PathBuf,
    /// 容器标记
    pub marker: DspackMarker,
}

impl Extracted {
    /// 清理临时目录。忽略失败（临时目录残留无害）。
    pub fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 解包 `.dspack` 到系统临时目录，并校验容器标记。
///
/// 失败时自动清理临时目录；成功后由调用方持有 `Extracted`，用完调 `cleanup()`。
pub fn extract(dspack_path: &Path) -> Result<Extracted, String> {
    if !dspack_path.is_file() {
        return Err(format!("文件不存在: {}", dspack_path.display()));
    }

    // 临时目录：dsh-multiver-import-<纳秒>，避免并发冲突
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("dsh-multiver-import-{}", stamp));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;

    // 用闭包包住解包+校验，失败时统一清理临时目录
    let result = (|| -> Result<DspackMarker, String> {
        let file = std::fs::File::open(dspack_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            // 非 ZIP（普通压缩包 / 损坏文件）走这里
            format!("这不是 dspack 整合包（无法作为 ZIP 打开）: {}", e)
        })?;

        // 逐条解压。zip crate 的 by_index 保证 name 是相对路径，
        // 但仍要防御性地拒绝越界路径（`..` / 绝对路径）。
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取归档条目失败: {}", e))?;
            let rel = match entry.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => {
                    return Err(format!("归档内存在非法路径: {}", entry.name()));
                }
            };
            let out = tmp.join(&rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)
                    .map_err(|e| format!("创建目录失败 {}: {}", out.display(), e))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("读取条目内容失败 {}: {}", rel.display(), e))?;
            std::fs::write(&out, &buf)
                .map_err(|e| format!("写入文件失败 {}: {}", out.display(), e))?;
        }

        // 容器标记必须在归档根
        let marker_path = tmp.join("dspack.json");
        if !marker_path.is_file() {
            return Err("这不是 dspack 整合包（根目录缺少 dspack.json）".to_string());
        }
        let text = std::fs::read_to_string(&marker_path)
            .map_err(|e| format!("读取 dspack.json 失败: {}", e))?;
        let marker: DspackMarker = serde_json::from_str(&text)
            .map_err(|e| format!("dspack.json 格式非法: {}", e))?;

        if marker.format != "dspack" {
            return Err(format!(
                "这不是 dspack 整合包（format 为 \"{}\"）",
                marker.format
            ));
        }
        if !SUPPORTED_CONTAINER_VERSIONS.contains(&marker.version) {
            return Err(format!(
                "不支持的容器版本 v{}（本管理器支持 v2 / v3），请升级管理器",
                marker.version
            ));
        }

        Ok(marker)
    })();

    match result {
        Ok(marker) => Ok(Extracted { dir: tmp, marker }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// 读取解包目录内的 manifest.json 原文（解析留给下一步）。
pub fn read_manifest_text(dir: &Path) -> Result<String, String> {
    let p = dir.join("manifest.json");
    if !p.is_file() {
        return Err("整合包内缺少 manifest.json".to_string());
    }
    std::fs::read_to_string(&p).map_err(|e| format!("读取 manifest.json 失败: {}", e))
}

// ===================== 路径安全与落盘原语 =====================

/// 判断某个归档内相对路径是否命中「敏感文件」过滤。
///
/// 打包侧已做五类过滤，导入侧仍要防御式拒绝——防止恶意包写敏感文件。
/// 返回 Some(原因) 表示应拒绝。
pub fn sensitive_reason(rel: &Path) -> Option<String> {
    use std::path::Component;

    // 逐段检查
    let segs: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();

    let basename = segs.last().cloned().unwrap_or_default();
    let lower = basename.to_lowercase();

    // 精确名（任意路径段命中）
    for seg in &segs {
        let s = seg.to_lowercase();
        if matches!(
            s.as_str(),
            "node_modules"
                | ".env"
                | ".credentials.yaml"
                | ".anonymous-user-id"
                | "settings.yaml"
                | ".dshpkcfg"
                | "credentials.yaml"
                | "credentials.yml"
                | "id_rsa"
                | "id_ed25519"
        ) {
            return Some(format!("含敏感文件/目录 \"{}\"", seg));
        }
        // attachments/ 与安装基线模板
        if s == "attachments" || s == ".system" {
            return Some(format!("含运行时/系统目录 \"{}\"", seg));
        }
    }

    // 扩展名
    if let Some(ext) = rel.extension().and_then(|e| e.to_str()) {
        let e = ext.to_lowercase();
        if matches!(
            e.as_str(),
            "key" | "pem" | "p12" | "pfx" | "crt" | "der" | "asc"
        ) {
            return Some(format!("含密钥/证书文件 \"{}\"", basename));
        }
        // 嵌套压缩包
        if matches!(e.as_str(), "zip" | "dspack" | "tgz" | "gz" | "tar") {
            return Some(format!("含嵌套压缩包 \"{}\"", basename));
        }
    }

    // 文件名正则类（用简单前缀/包含判断，避免引入 regex 依赖）
    let is_cred = (lower.starts_with("credentials") && (lower.ends_with(".yml") || lower.ends_with(".yaml")))
        || lower.ends_with(".credentials")
        || (lower.starts_with("secrets") && (lower.ends_with(".json") || lower.ends_with(".yml") || lower.ends_with(".yaml")))
        || lower.contains("token")
        || lower.contains("api_key")
        || lower.starts_with("id_rsa")
        || lower.starts_with("id_ed25519")
        || (lower.starts_with(".env"));
    if is_cred {
        return Some(format!("含疑似凭据文件 \"{}\"", basename));
    }

    None
}

/// 安全拼接：把归档内相对路径 rel 拼到 base 下，拒绝越界。
///
/// rel 必须是纯相对路径（不含 \`..\`、不以盘符/根开头）。
pub fn safe_join(base: &Path, rel: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!("非法路径（含 ..）: {}", rel.display()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("非法路径（绝对路径）: {}", rel.display()));
            }
        }
    }
    Ok(base.join(rel))
}

/// 递归把 src 下的内容复制到 dst（保持相对结构），逐条施加安全校验。
///
/// - 跳过命中 sensitive_reason 的条目（记录到 skipped）
/// - 目标已存在则覆盖
/// - 返回 (复制的文件数, 被跳过的条目列表)
pub fn copy_tree_checked(
    src: &Path,
    dst: &Path,
    skipped: &mut Vec<String>,
) -> Result<u64, String> {
    if !src.exists() {
        return Ok(0);
    }
    let mut count = 0u64;
    copy_tree_inner(src, src, dst, skipped, &mut count)?;
    Ok(count)
}

fn copy_tree_inner(
    root: &Path,
    cur: &Path,
    dst: &Path,
    skipped: &mut Vec<String>,
    count: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(cur).map_err(|e| format!("读取目录失败 {}: {}", cur.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| format!("路径解析失败: {}", e))?;

        if let Some(reason) = sensitive_reason(rel) {
            skipped.push(format!("{}（{}）", rel.display(), reason));
            continue;
        }

        let out = safe_join(dst, rel)?;
        if path.is_dir() {
            std::fs::create_dir_all(&out)
                .map_err(|e| format!("创建目录失败 {}: {}", out.display(), e))?;
            copy_tree_inner(root, &path, dst, skipped, count)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
            }
            std::fs::copy(&path, &out)
                .map_err(|e| format!("复制失败 {} → {}: {}", path.display(), out.display(), e))?;
            *count += 1;
        }
    }
    Ok(())
}

// ===================== 实例元数据标记 =====================

/// 实例目录内记录的元数据（供 list() 快速读取，避免每次解包）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceMeta {
    pub kind: String,
    pub modpack_name: String,
    pub modpack_version: String,
    pub display_name: String,
    pub description: String,
    pub author: String,
    pub icon: String,
    pub packed_dsh_version: String,
    pub modpack_type: String,
    pub bundle_count: usize,
    pub skill_count: usize,
    /// 启动该实例应使用的 profile 名
    pub launch_profile: String,
    /// 导入日期 YYYY-MM-DD
    pub imported_at: String,
    /// 被安全过滤跳过的条目数
    pub skipped_count: usize,
}

/// 标记文件名
pub const META_FILE: &str = ".dsh-multiver-meta.json";

/// 由 manifest 与导入结果构造标记。
pub fn build_meta(m: &Manifest, outcome: &ImportOutcome) -> InstanceMeta {
    let (bundles_len, skills_len) = if m.is_dshhome() {
        let dp = m.default_profile.clone().unwrap_or_default();
        let bl = m.profiles.get(&dp).map(|u| u.bundles.len()).unwrap_or(0);
        (bl, m.raw.get("skills").and_then(|s| s.as_array()).map(|a| a.len()).unwrap_or(0))
    } else {
        (m.bundles.len(), 0)
    };

    InstanceMeta {
        kind: "modpack".to_string(),
        modpack_name: m.name.clone().unwrap_or_default(),
        modpack_version: m.version.clone().unwrap_or_default(),
        display_name: m.display_name_text(),
        description: m.description_text(),
        author: m.author.clone().unwrap_or_default(),
        icon: m.icon.clone().unwrap_or_default(),
        packed_dsh_version: outcome.dsh_version.clone(),
        modpack_type: m.kind.clone(),
        bundle_count: bundles_len,
        skill_count: skills_len,
        launch_profile: m.launch_profile().unwrap_or_else(|| "web".to_string()),
        imported_at: today_local(),
        skipped_count: outcome.skipped.len(),
    }
}

/// 写标记文件到实例目录。
pub fn write_meta(instance_dir: &Path, meta: &InstanceMeta) -> Result<(), String> {
    let text = serde_json::to_string_pretty(meta).map_err(|e| format!("序列化元数据失败: {}", e))?;
    std::fs::write(instance_dir.join(META_FILE), text)
        .map_err(|e| format!("写入元数据失败: {}", e))
}

/// 读标记文件（失败返回 None）。
pub fn read_meta(instance_dir: &Path) -> Option<InstanceMeta> {
    let text = std::fs::read_to_string(instance_dir.join(META_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 今天（东八区）YYYY-MM-DD。
fn today_local() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let local = secs + 8 * 3600;
    let days = local.div_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 由「1970-01-01 起的天数」换算 (年, 月, 日)。算法同 versions.rs。
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

// ===================== files[] 下载与校验 =====================

/// 计算字节的 sha256 十六进制串。
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// 下载单个 FileRef 并校验，落到 \`root\` 下的 \`f.path\`。
///
/// - 依次尝试 \`urls\`，任一成功即止
/// - 校验 size 与 sha256，不符则视为失败
/// - 任一环节失败 → 删除已落文件（若已写）并返回错误
pub fn download_file_ref(root: &Path, f: &FileRef) -> Result<(), String> {
    let target = safe_join(root, Path::new(&f.path))?;

    let mut last_err = String::from("没有可用的下载地址");
    for url in &f.urls {
        match fetch_bytes(url) {
            Ok(bytes) => {
                if bytes.len() as u64 != f.size {
                    last_err = format!(
                        "{}：大小不符（期望 {} 字节，实际 {} 字节）",
                        url,
                        f.size,
                        bytes.len()
                    );
                    continue;
                }
                let got = sha256_hex(&bytes);
                if !got.eq_ignore_ascii_case(&f.sha256) {
                    last_err = format!(
                        "{}：sha256 不符（期望 {}，实际 {}）",
                        url, f.sha256, got
                    );
                    continue;
                }
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
                }
                std::fs::write(&target, &bytes)
                    .map_err(|e| format!("写入文件失败 {}: {}", target.display(), e))?;
                return Ok(());
            }
            Err(e) => {
                last_err = format!("{}：{}", url, e);
            }
        }
    }

    // 全部失败：清掉可能的半成品
    let _ = std::fs::remove_file(&target);
    Err(format!("下载失败（{}）", last_err))
}

/// 下载一批 FileRef，全部成功才返回 Ok；任一失败 → 清理本批已下文件。
pub fn download_all(root: &Path, files: &[FileRef]) -> Result<(), String> {
    let mut done: Vec<PathBuf> = Vec::new();
    for f in files {
        match download_file_ref(root, f) {
            Ok(()) => {
                if let Ok(p) = safe_join(root, Path::new(&f.path)) {
                    done.push(p);
                }
            }
            Err(e) => {
                for p in &done {
                    let _ = std::fs::remove_file(p);
                }
                return Err(format!("下载 {} 失败：{}", f.path, e));
            }
        }
    }
    Ok(())
}

/// HTTP GET 取字节（阻塞）。
fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("dsh-multiver")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取响应失败: {}", e))
}

// ===================== 导入编排 =====================

/// 导入进度回调（阶段文案）。
pub type StageFn<'a> = &'a dyn Fn(&str);

/// 导入用到的路径集合。
pub struct ImportDirs<'a> {
    pub versions_dir: &'a Path,
    pub store_dir: &'a Path,
    pub cache_dir: &'a Path,
    pub state_dir: &'a Path,
}

/// 导入结果。
#[derive(Debug)]
pub struct ImportOutcome {
    /// 实例目录名（versions/ 下的目录名）
    pub instance_name: String,
    /// 实例目录绝对路径
    pub instance_dir: PathBuf,
    /// 被安全过滤跳过的条目（供 UI 提示）
    pub skipped: Vec<String>,
    /// 实际使用的 dsh 版本
    pub dsh_version: String,
}

/// Windows 上调用 pnpm 需走 cmd。
#[cfg(windows)]
fn pnpm_cmd() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = std::process::Command::new("cmd");
    c.arg("/C").arg("pnpm");
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(not(windows))]
fn pnpm_cmd() -> std::process::Command {
    std::process::Command::new("pnpm")
}

/// 在指定目录跑 \`pnpm install\`（用包依赖），流式读 stderr 推阶段。
fn run_pnpm_install(
    dir: &Path,
    dirs: &ImportDirs,
    on_stage: StageFn,
) -> Result<(), String> {
    let mut cmd = pnpm_cmd();
    cmd.current_dir(dir)
        .arg("install")
        .arg(format!("--config.store-dir={}", dirs.store_dir.to_string_lossy()))
        .arg(format!("--config.cache-dir={}", dirs.cache_dir.to_string_lossy()))
        .arg(format!("--config.state-dir={}", dirs.state_dir.to_string_lossy()))
        .arg("--config.confirmModulesPurge=false")
        .arg("--config.dangerouslyAllowAllBuilds=true")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动 pnpm install 失败: {}", e))?;

    use std::io::BufRead;
    use std::sync::{Arc, Mutex};

    let stdout_text = Arc::new(Mutex::new(String::new()));
    let stdout_handle = child.stdout.take().map(|stdout| {
        let c = stdout_text.clone();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut g) = c.lock() {
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        })
    });

    let mut err_text = String::new();
    let mut last = String::new();
    if let Some(stderr) = child.stderr.take() {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            err_text.push_str(&line);
            err_text.push('\n');
            if let Some(stage) = crate::versions::parse_stage(&line) {
                if stage != last {
                    last = stage.to_string();
                    on_stage(stage);
                }
            }
        }
    }
    let status = child.wait();
    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    let out_text = stdout_text.lock().map(|g| g.clone()).unwrap_or_default();
    let combined = format!("{}{}", err_text.trim(), if out_text.trim().is_empty() { String::new() } else { format!("\n{}", out_text.trim()) });
    if matches!(status, Ok(s) if s.success()) {
        Ok(())
    } else {
        Err(format!("安装依赖失败：\n{}", combined.trim()))
    }
}

/// 生成实例目录名：\`modpack-<name>-<version>\`。
/// \`suffix\` 非空时（保留两份场景）追加 \`-<suffix>\`。
pub fn instance_dir_name(m: &Manifest, suffix: Option<&str>) -> String {
    let name = m.name.clone().unwrap_or_else(|| "unnamed".to_string());
    let ver = m.version.clone().unwrap_or_else(|| "0.0.0".to_string());
    let base = format!("modpack-{}-{}", sanitize(&name), sanitize(&ver));
    match suffix {
        Some(s) if !s.trim().is_empty() => format!("{}-{}", base, sanitize(s)),
        _ => base,
    }
}

/// 删除目录，带退避重试（应对 Windows 上短暂的文件占用）。
fn remove_dir_with_retry(dir: &Path, attempts: u32) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut last = None;
    for i in 0..attempts {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(150 * (i as u64 + 1)));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "未知错误")))
}

/// 把标识/版本里的非法文件名字符替换为 \`-\`。
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') { c } else { '-' })
        .collect()
}

/// 四阶段导入（不含下载 files[]，那部分独立）。
///
/// 任一阶段失败 → 删除整个新建实例目录，回到「目录不存在」状态。
///
/// \`dsh_version\` 由调用方在阶段 0 决定（本函数不负责弹框安装 dsh 本体，
/// 但会调用 \`ensure_dsh\` 回调来装）。
pub fn import(
    extracted: &Extracted,
    manifest: &Manifest,
    dirs: &ImportDirs,
    instance_name: &str,
    dsh_version: &str,
    ensure_dsh: &dyn Fn(&str) -> Result<(), String>,
    on_stage: StageFn,
) -> Result<ImportOutcome, String> {
    let instance_dir = dirs.versions_dir.join(instance_name);
    if instance_dir.exists() {
        return Err(format!("实例目录已存在: {}", instance_dir.display()));
    }

    let mut skipped: Vec<String> = Vec::new();

    // 用闭包包住全部阶段，失败统一回滚
    let result = (|| -> Result<(), String> {
        // ---- 阶段 1：落盘 ----
        on_stage("正在创建实例目录...");
        std::fs::create_dir_all(&instance_dir)
            .map_err(|e| format!("创建实例目录失败: {}", e))?;

        // 重建 package.json（manifest 权威）
        let pkg = build_package_json(manifest)?;
        std::fs::write(instance_dir.join("package.json"), pkg)
            .map_err(|e| format!("写入 package.json 失败: {}", e))?;

        // .npmrc：hoisted（必须，与现有版本安装一致）
        std::fs::write(instance_dir.join(".npmrc"), "node-linker=hoisted\n")
            .map_err(|e| format!("写入 .npmrc 失败: {}", e))?;

        // overrides/ → profile 根（即实例目录）
        on_stage("正在展开 profile 内容...");
        let ov = extracted.dir.join("overrides");
        copy_tree_checked(&ov, &instance_dir, &mut skipped)?;

        // patch 兜底：文件不存在时用 manifest.patch 写入
        let patch_file = instance_dir.join("cordis.patch.yml");
        if !patch_file.exists() {
            if let Some(p) = &manifest.patch {
                let _ = std::fs::write(&patch_file, p);
            }
        }

        // home/ → 隔离 home 根
        let home_root = instance_dir.join("home");
        std::fs::create_dir_all(&home_root)
            .map_err(|e| format!("创建 home 目录失败: {}", e))?;
        let hov = extracted.dir.join("home");
        if hov.exists() {
            on_stage("正在展开 home 内容...");
            copy_tree_checked(&hov, &home_root, &mut skipped)?;
        }

        // 创建 profile 定义（协议 §3.2.2：名称取 profileName，缺省 pack）
        // dsh 启动时 --profile <名> 会在 $DSH_HOME/profiles/<名>/ 找 package.json，
        // 缺失会直接报 "profile does not exist"。
        on_stage("正在创建 profile...");
        write_profile(&home_root, manifest)?;

        // ---- 阶段 2：运行时与依赖 ----
        on_stage("正在准备 dsh 运行时...");
        ensure_dsh(dsh_version)?;

        // 用 dsh 本体所在的版本目录作为 dsh 来源；这里采用「独立安装」策略：
        // 把 dsh 本体装进本实例（与现有版本目录同构）。
        // 若本机已有该 dsh 版本，则从其 node_modules 复制 dsh 的 .bin 入口不可行
        // （dsh 需要完整依赖树），故统一走 pnpm add 到本实例。
        on_stage("正在安装 dsh 本体...");
        install_dsh_into(&instance_dir, dirs, dsh_version, on_stage)?;

        on_stage("正在安装整合包依赖...");
        run_pnpm_install(&instance_dir, dirs, on_stage)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            on_stage("导入完成");
            Ok(ImportOutcome {
                instance_name: instance_name.to_string(),
                instance_dir,
                skipped,
                dsh_version: dsh_version.to_string(),
            })
        }
        Err(e) => {
            // 回滚：删除整个实例目录。
            // Windows 上 pnpm 退出后可能仍短暂持有目录句柄（worker/杀毒/索引），
            // 故带退避重试；仍失败则如实报告，让用户手动清理，而不是静默留半成品。
            match remove_dir_with_retry(&instance_dir, 8) {
                Ok(()) => Err(e),
                Err(del_err) => Err(format!(
                    "{}\n\n（回滚未完成：删除实例目录失败 {}。请关闭占用该目录的进程后手动删除 {}）",
                    e,
                    del_err,
                    instance_dir.display()
                )),
            }
        }
    }
}

/// 把 dsh 本体装进实例目录（pnpm add @deepseek-ai/dsh@<version>）。
fn install_dsh_into(
    instance_dir: &Path,
    dirs: &ImportDirs,
    dsh_version: &str,
    on_stage: StageFn,
) -> Result<(), String> {
    let spec = format!("@deepseek-ai/dsh@{}", dsh_version);
    let mut cmd = pnpm_cmd();
    cmd.current_dir(instance_dir)
        .arg("add")
        .arg(&spec)
        .arg(format!("--config.store-dir={}", dirs.store_dir.to_string_lossy()))
        .arg(format!("--config.cache-dir={}", dirs.cache_dir.to_string_lossy()))
        .arg(format!("--config.state-dir={}", dirs.state_dir.to_string_lossy()))
        .arg("--config.confirmModulesPurge=false")
        .arg("--config.dangerouslyAllowAllBuilds=true")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动 pnpm add 失败: {}", e))?;

    // stdout 与 stderr 都要读：pnpm 的进度走 stderr，但部分错误（尤其经 cmd /C）
    // 会走 stdout。两个流都用独立线程读到 EOF，避免管道填满导致假死。
    use std::io::BufRead;
    use std::sync::{Arc, Mutex};

    // stdout 用独立线程读到 EOF（丢弃内容，仅防管道填满）
    let stdout_text = Arc::new(Mutex::new(String::new()));
    let stdout_handle = child.stdout.take().map(|stdout| {
        let c = stdout_text.clone();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut g) = c.lock() {
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        })
    });

    // stderr 在主线程逐行读（顺带推阶段）——pnpm 的进度与多数错误走 stderr
    let mut err_text = String::new();
    let mut last = String::new();
    if let Some(stderr) = child.stderr.take() {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            err_text.push_str(&line);
            err_text.push('\n');
            if let Some(stage) = crate::versions::parse_stage(&line) {
                if stage != last {
                    last = stage.to_string();
                    on_stage(stage);
                }
            }
        }
    }
    let status = child.wait();
    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    let out_text = stdout_text.lock().map(|g| g.clone()).unwrap_or_default();
    let combined = format!("{}{}", err_text.trim(), if out_text.trim().is_empty() { String::new() } else { format!("\n{}", out_text.trim()) });
    if matches!(status, Ok(s) if s.success()) {
        Ok(())
    } else {
        Err(format!("安装 dsh {} 失败：\n{}", dsh_version, combined.trim()))
    }
}

// ===================== package.json 重建（依赖坐标转换） =====================

/// 把 manifest 的「坐标 → 固定版本」转成 package.json 的「包名 → pnpm spec」。
///
/// 转换规则（协议 manifest v3 §5）：
/// - \`"dsh-pet": "0.2.0"\`                       → \`"dsh-pet": "0.2.0"\`
/// - \`"github:owner/repo": "<sha>"\`              → \`"repo": "github:owner/repo#<sha>"\`
/// - \`"github:owner/repo#path:/pkg": "<sha>"\`    → \`"pkg": "github:owner/repo#<sha>&path:pkg"\`
pub fn coord_to_dep(coord: &str, version: &str) -> (String, String) {
    if let Some(rest) = coord.strip_prefix("github:") {
        // rest 形如 owner/repo 或 owner/repo#path:/pkg
        if let Some((repo_part, path_part)) = rest.split_once("#path:") {
            let path = path_part.trim_start_matches('/');
            // 包名取自 path 的最后一段（该仓库里的子包），而非仓库名
            let pkg_name = path.rsplit('/').next().unwrap_or(path).to_string();
            return (pkg_name, format!("github:{}#{}&path:{}", repo_part, version, path));
        }
        let pkg_name = rest.rsplit('/').next().unwrap_or(rest).to_string();
        return (pkg_name, format!("github:{}#{}", rest, version));
    }
    // npm 坐标（含带 scope 的包名）：原样
    (coord.to_string(), version.to_string())
}

/// 在实例的 home 下写 profile 定义。
///
/// dsh 启动时 \`--profile <名>\` 会在 \`$DSH_HOME/profiles/<名>/package.json\` 找定义，
/// 缺失会报 "profile does not exist"。
///
/// - profile 形态：profile 名取 manifest.profileName（缺省 "pack"），
///   bundles/dependencies 用 manifest 顶层字段
/// - dshhome 形态：为每个 profiles.<name> 写一份
pub fn write_profile(home_root: &Path, m: &Manifest) -> Result<(), String> {
    if m.is_dshhome() {
        for (name, unit) in &m.profiles {
            write_one_profile(home_root, name, &unit.bundles, &unit.dependencies)?;
        }
        Ok(())
    } else {
        let name = m
            .profile_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "pack".to_string());
        write_one_profile(home_root, &name, &m.bundles, &m.dependencies)
    }
}

fn write_one_profile(
    home_root: &Path,
    name: &str,
    bundles: &[String],
    dependencies: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let dir = home_root.join("profiles").join(name);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建 profile 目录失败 {}: {}", dir.display(), e))?;

    // dependencies 用坐标转换后的结果（与实例 package.json 一致）
    let mut deps = serde_json::Map::new();
    let mut coords: Vec<(&String, &String)> = dependencies.iter().collect();
    coords.sort_by(|a, b| a.0.cmp(b.0));
    for (coord, ver) in coords {
        let (dep_name, spec) = coord_to_dep(coord, ver);
        deps.insert(dep_name, serde_json::Value::String(spec));
    }

    let mut root = serde_json::Map::new();
    root.insert(
        "name".into(),
        serde_json::Value::String(format!("dsh-profile-{}", name)),
    );
    root.insert("private".into(), serde_json::Value::Bool(true));
    root.insert("dependencies".into(), serde_json::Value::Object(deps));

    let mut profile = serde_json::Map::new();
    profile.insert(
        "bundles".into(),
        serde_json::Value::Array(
            bundles
                .iter()
                .map(|b| serde_json::Value::String(b.clone()))
                .collect(),
        ),
    );
    let mut dsh = serde_json::Map::new();
    dsh.insert("profile".into(), serde_json::Value::Object(profile));
    root.insert("dsh".into(), serde_json::Value::Object(dsh));

    let text = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .map_err(|e| format!("生成 profile 定义失败: {}", e))?;
    std::fs::write(dir.join("package.json"), text)
        .map_err(|e| format!("写入 profile 定义失败: {}", e))?;

    // dsh 的 profile 还需要这两个文件（见 dsh-app-boot 的 initProfile）：
    // - cordis.patch.yml：用户 patch 层（空数组即可）
    // - pnpm-workspace.yaml：out-of-tree 插件所需的 pnpm 配置
    // 缺它们时 profile 能加载，但插件安装/覆盖行为异常。
    let patch_path = dir.join("cordis.patch.yml");
    if !patch_path.exists() {
        let _ = std::fs::write(
            &patch_path,
            "# Your patch layer for this dsh profile, applied after every bundle layer:\n# a top-level YAML array of loader patch entries.\n[]\n",
        );
    }
    let ws_path = dir.join("pnpm-workspace.yaml");
    if !ws_path.exists() {
        let _ = std::fs::write(
            &ws_path,
            "packages:\n  - .\n\nnodeLinker: hoisted\nautoInstallPeers: false\n",
        );
    }
    Ok(())
}

/// 由 manifest 构建 package.json 的 JSON 文本（带缩进、无 BOM）。
pub fn build_package_json(m: &Manifest) -> Result<String, String> {
    let mut deps = serde_json::Map::new();

    // profile 形态取顶层 dependencies；dshhome 形态取 defaultProfile 指向的那个 profile 的
    let (bundles, dependencies): (&[String], &std::collections::HashMap<String, String>) =
        if m.is_dshhome() {
            let dp = m
                .default_profile
                .as_ref()
                .ok_or_else(|| "dshhome 形态缺少 defaultProfile".to_string())?;
            let unit = m
                .profiles
                .get(dp)
                .ok_or_else(|| format!("defaultProfile \"{}\" 不在 profiles 中", dp))?;
            (&unit.bundles, &unit.dependencies)
        } else {
            (&m.bundles, &m.dependencies)
        };

    // 按坐标字典序输出，保证可复现
    let mut coords: Vec<(&String, &String)> = dependencies.iter().collect();
    coords.sort_by(|a, b| a.0.cmp(b.0));
    for (coord, ver) in coords {
        let (name, spec) = coord_to_dep(coord, ver);
        deps.insert(name, serde_json::Value::String(spec));
    }

    let name = format!(
        "dsh-modpack-{}",
        m.name.clone().unwrap_or_else(|| "unnamed".to_string())
    );
    let mut root = serde_json::Map::new();
    root.insert("name".into(), serde_json::Value::String(name));
    root.insert("private".into(), serde_json::Value::Bool(true));
    root.insert(
        "version".into(),
        serde_json::Value::String(m.version.clone().unwrap_or_else(|| "0.0.0".to_string())),
    );
    root.insert("dependencies".into(), serde_json::Value::Object(deps));

    // bundles 也写进 dsh.profile.bundles，便于 dsh 读取层栈
    let mut dsh = serde_json::Map::new();
    let mut profile = serde_json::Map::new();
    profile.insert(
        "bundles".into(),
        serde_json::Value::Array(
            bundles
                .iter()
                .map(|b| serde_json::Value::String(b.clone()))
                .collect(),
        ),
    );
    dsh.insert("profile".into(), serde_json::Value::Object(profile));
    root.insert("dsh".into(), serde_json::Value::Object(dsh));

    serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .map_err(|e| format!("生成 package.json 失败: {}", e))
}

// ===================== manifest 解析与校验 =====================

/// 支持的 manifest 版本：现行 v5，兼容 v4。
/// （协议另列 v3 / v2 / v1，但它们绑定 .tgz 历史格式，一期不支持。）
pub const SUPPORTED_MANIFEST_VERSIONS: &[u32] = &[4, 5];

/// 多语言文本：既可能是普通字符串，也可能是「语言码 → 文本」的 map。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocalizedText {
    Plain(String),
    Localized(std::collections::HashMap<String, String>),
}

impl LocalizedText {
    /// 取展示文本：优先 zh-CN，其次 en-US，再取 map 中字典序最小的项，最后空串。
    pub fn display(&self) -> String {
        match self {
            LocalizedText::Plain(s) => s.clone(),
            LocalizedText::Localized(m) => {
                for key in ["zh-CN", "zh", "en-US", "en"] {
                    if let Some(v) = m.get(key) {
                        return v.clone();
                    }
                }
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                keys.first().and_then(|k| m.get(*k)).cloned().unwrap_or_default()
            }
        }
    }

    /// 是否为空（无任何文本）
    pub fn is_empty(&self) -> bool {
        match self {
            LocalizedText::Plain(s) => s.trim().is_empty(),
            LocalizedText::Localized(m) => m.is_empty(),
        }
    }
}

/// dshhome 形态的单个 profile 单元（= v4 单 profile 契约去掉 profileName）。
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileUnit {
    pub bundles: Vec<String>,
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub patch: Option<String>,
}

/// files[] 条目（重内容下载清单）。
#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub urls: Vec<String>,
}

/// manifest 的原始结构。缺失的必填项在 validate() 里统一收集报告
/// （而非 serde 遇第一个错误就退出）。
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    #[serde(rename = "type")]
    pub kind: String,

    // 必填字段用 Option 承接：缺失时走 validate() 统一报告，
    // 而不是让 serde 在解析阶段就失败（那样只能报第一个错误）。
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,

    #[serde(default, rename = "displayName")]
    pub display_name: Option<LocalizedText>,
    #[serde(default)]
    pub description: Option<LocalizedText>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub category: Option<String>,

    #[serde(default, rename = "dshVersion")]
    pub dsh_version: Option<String>,
    #[serde(default, rename = "dshVersions")]
    pub dsh_versions: Option<Vec<String>>,

    #[serde(default)]
    pub files: Vec<FileRef>,

    // ---- profile 形态 ----
    #[serde(default, rename = "profileName")]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub bundles: Vec<String>,
    #[serde(default)]
    pub dependencies: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub patch: Option<String>,

    // ---- dshhome 形态 ----
    #[serde(default, rename = "defaultProfile")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, ProfileUnit>,

    /// 保留原始 JSON，供后续未建模字段（如 vendored / launchers）按需读取。
    #[serde(skip)]
    pub raw: serde_json::Value,
}

impl Manifest {
    /// 该 manifest 是否为 dshhome 形态。
    pub fn is_dshhome(&self) -> bool {
        self.kind == "dshhome"
    }

    /// 展示名（回退到 name）。
    pub fn display_name_text(&self) -> String {
        match &self.display_name {
            Some(t) if !t.is_empty() => t.display(),
            _ => self.name.clone().unwrap_or_default(),
        }
    }

    /// 描述（可能为空）。
    pub fn description_text(&self) -> String {
        self.description.as_ref().map(|t| t.display()).unwrap_or_default()
    }

    /// 启动该包应使用的 profile 名。
    /// profile 形态取 profileName（缺省 "pack"）；dshhome 形态取 defaultProfile。
    pub fn launch_profile(&self) -> Option<String> {
        if self.is_dshhome() {
            self.default_profile.clone()
        } else {
            Some(
                self.profile_name
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "pack".to_string()),
            )
        }
    }

    /// 解析并校验 manifest 原文。返回所有问题（一次性收集）。
    pub fn parse(text: &str) -> Result<Manifest, Vec<String>> {
        let mut m: Manifest = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => return Err(vec![format!("manifest.json 解析失败: {}", e)]),
        };
        m.raw = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);

        let errs = m.validate();
        if errs.is_empty() {
            Ok(m)
        } else {
            Err(errs)
        }
    }

    /// 字段校验，返回全部问题。
    fn validate(&self) -> Vec<String> {
        let mut e: Vec<String> = Vec::new();

        if !SUPPORTED_MANIFEST_VERSIONS.contains(&self.manifest_version) {
            if self.manifest_version == 1 {
                e.push("已废弃的包格式（manifest v1），无法安装".to_string());
            } else if self.manifest_version < 1 || self.manifest_version > 5 {
                e.push(format!(
                    "不支持的 manifest 版本 v{}（支持 v4 / v5）",
                    self.manifest_version
                ));
            } else {
                e.push(format!(
                    "该包为历史格式（manifest v{}，.tgz），一期暂不支持",
                    self.manifest_version
                ));
            }
        }

        match &self.name {
            None => e.push("缺少必填字段 name".to_string()),
            Some(n) if n.trim().is_empty() => e.push("必填字段 name 为空".to_string()),
            _ => {}
        }
        match &self.version {
            None => e.push("缺少必填字段 version".to_string()),
            Some(v) if v.trim().is_empty() => e.push("必填字段 version 为空".to_string()),
            _ => {}
        }

        match self.kind.as_str() {
            "profile" => {
                if self.bundles.is_empty() {
                    e.push("缺少必填字段 bundles（profile 形态必须有层栈）".to_string());
                }
            }
            "dshhome" => {
                if self.profiles.is_empty() {
                    e.push("dshhome 形态必须有非空的 profiles".to_string());
                }
                for bad in ["web", "headless"] {
                    if self.profiles.contains_key(bad) {
                        e.push(format!(
                            "profiles 不得包含安装基线模板 \"{}\"（由 dshVersion 决定，不进包）",
                            bad
                        ));
                    }
                }
                match &self.default_profile {
                    None => e.push("dshhome 形态必须有 defaultProfile".to_string()),
                    Some(dp) => {
                        if !self.profiles.contains_key(dp) {
                            e.push(format!(
                                "defaultProfile 指向的 \"{}\" 不在 profiles 中",
                                dp
                            ));
                        }
                    }
                }
            }
            "collection" => {
                e.push("type: \"collection\" 为预留形态，暂不支持".to_string());
            }
            other => {
                e.push(format!(
                    "字段 type 值非法：\"{}\"（应为 \"profile\" 或 \"dshhome\"）",
                    other
                ));
            }
        }

        if let Some(set) = &self.dsh_versions {
            if set.is_empty() {
                e.push("dshVersions 若出现则不得为空数组".to_string());
            }
            let mut seen = std::collections::HashSet::new();
            for v in set {
                if v.trim().is_empty() {
                    e.push("dshVersions 含空字符串项".to_string());
                }
                if !seen.insert(v) {
                    e.push(format!("dshVersions 含重复项 \"{}\"", v));
                }
            }
            if let Some(dv) = &self.dsh_version {
                if !set.iter().any(|v| v == dv) {
                    e.push(format!("dshVersion \"{}\" 不在 dshVersions 集合中", dv));
                }
            }
        }

        for (i, f) in self.files.iter().enumerate() {
            if f.path.trim().is_empty() {
                e.push(format!("files[{}].path 为空", i));
            }
            if f.urls.is_empty() {
                e.push(format!("files[{}].urls 为空（至少一个下载地址）", i));
            }
        }

        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用代码动态构造一个最小 `.dspack`（ZIP + dspack.json + manifest.json）。
    /// 只用于本地构造的测试，不依赖网络。
    fn build_min_dspack(dir: &Path, marker_json: &str, with_manifest: bool) -> PathBuf {
        let path = dir.join("test.dspack");
        let file = std::fs::File::create(&path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("dspack.json", opts).unwrap();
        use std::io::Write;
        zw.write_all(marker_json.as_bytes()).unwrap();
        if with_manifest {
            zw.start_file("manifest.json", opts).unwrap();
            zw.write_all(b"{\"manifestVersion\":5,\"type\":\"profile\"}").unwrap();
        }
        zw.finish().unwrap();
        path
    }

    #[test]
    fn rejects_non_zip() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nonzip");
        let _ = std::fs::create_dir_all(&tmp);
        let p = tmp.join("not-a-zip.dspack");
        std::fs::write(&p, b"hello world").unwrap();
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不是 dspack 整合包"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_missing_marker() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nomarker");
        let _ = std::fs::create_dir_all(&tmp);
        // 一个合法 ZIP，但根没有 dspack.json
        let p = tmp.join("empty.dspack");
        let file = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("readme.txt", opts).unwrap();
        use std::io::Write;
        zw.write_all(b"hi").unwrap();
        zw.finish().unwrap();

        let err = extract(&p).unwrap_err();
        assert!(err.contains("缺少 dspack.json"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_unsupported_container_version() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-badver");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":9}"#, true);
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不支持的容器版本 v9"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- files[] 下载校验 ----------

    #[test]
    fn sha256_known_vector() {
        // "abc" 的 sha256
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn download_rejects_path_traversal() {
        let f = FileRef {
            path: "../evil.txt".to_string(),
            sha256: "x".to_string(),
            size: 1,
            urls: vec!["https://example.invalid/x".to_string()],
        };
        let tmp = std::env::temp_dir().join("dsh-multiver-test-dl-escape");
        let _ = std::fs::create_dir_all(&tmp);
        let err = download_file_ref(&tmp, &f).unwrap_err();
        assert!(err.contains("非法路径"), "实际: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn download_all_cleans_on_failure() {
        // 两个条目，第一个假 URL 必失败 → 应报错且不留下文件
        let files = vec![FileRef {
            path: "a.bin".to_string(),
            sha256: "00".to_string(),
            size: 1,
            urls: vec!["http://127.0.0.1:9/nope".to_string()],
        }];
        let tmp = std::env::temp_dir().join("dsh-multiver-test-dl-clean");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);
        let err = download_all(&tmp, &files).unwrap_err();
        assert!(err.contains("下载"), "实际: {}", err);
        assert!(!tmp.join("a.bin").exists(), "失败后不应留下文件");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- 依赖坐标转换 ----------

    #[test]
    fn coord_npm_passthrough() {
        let (name, spec) = coord_to_dep("dsh-pet", "0.2.0");
        assert_eq!(name, "dsh-pet");
        assert_eq!(spec, "0.2.0");
    }

    #[test]
    fn coord_github_simple() {
        let (name, spec) = coord_to_dep("github:DViridescent/dafy-whale-theme", "99e8c57");
        assert_eq!(name, "dafy-whale-theme");
        assert_eq!(spec, "github:DViridescent/dafy-whale-theme#99e8c57");
    }

    #[test]
    fn coord_github_with_path() {
        let (name, spec) = coord_to_dep("github:owner/repo#path:/pkg", "abc123");
        assert_eq!(name, "pkg");
        assert_eq!(spec, "github:owner/repo#abc123&path:pkg");
    }

    #[test]
    fn package_json_has_deps_and_bundles() {
        let m = Manifest::parse(r#"{
            "manifestVersion": 5, "type": "profile", "name": "demo", "version": "1.0.0",
            "bundles": ["@deepseek-ai/dsh-base"],
            "dependencies": {"dsh-pet": "0.2.0", "github:a/b": "sha1"}
        }"#).unwrap();
        let txt = build_package_json(&m).unwrap();
        let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["name"], "dsh-modpack-demo");
        assert_eq!(v["private"], true);
        assert_eq!(v["dependencies"]["dsh-pet"], "0.2.0");
        assert_eq!(v["dependencies"]["b"], "github:a/b#sha1");
        assert_eq!(v["dsh"]["profile"]["bundles"][0], "@deepseek-ai/dsh-base");
    }

    #[test]
    fn instance_name_sanitized() {
        let m = Manifest::parse(r#"{
            "manifestVersion": 5, "type": "profile", "name": "Better Pack", "version": "1.0.0",
            "bundles": ["a"]
        }"#).unwrap();
        assert_eq!(instance_dir_name(&m, None), "modpack-Better-Pack-1.0.0");
        assert_eq!(
            instance_dir_name(&m, Some("20260928-143052")),
            "modpack-Better-Pack-1.0.0-20260928-143052"
        );
    }

    // ---------- 路径安全与落盘原语 ----------

    #[test]
    fn safe_join_rejects_parent_dir() {
        let base = Path::new("C:/base");
        assert!(safe_join(base, Path::new("a/b.txt")).is_ok());
        assert!(safe_join(base, Path::new("../evil.txt")).is_err());
        assert!(safe_join(base, Path::new("a/../../evil.txt")).is_err());
    }

    #[test]
    fn sensitive_reason_blocks_common_cases() {
        assert!(sensitive_reason(Path::new(".env")).is_some());
        assert!(sensitive_reason(Path::new("config/.env")).is_some());
        assert!(sensitive_reason(Path::new("node_modules/x.js")).is_some());
        assert!(sensitive_reason(Path::new("keys/server.pem")).is_some());
        assert!(sensitive_reason(Path::new("creds/credentials.yaml")).is_some());
        assert!(sensitive_reason(Path::new("nested/inner.zip")).is_some());
        assert!(sensitive_reason(Path::new("settings.yaml")).is_some());
        assert!(sensitive_reason(Path::new(".dshpkcfg")).is_some());

        // 正常文件应放行
        assert!(sensitive_reason(Path::new("cordis.patch.yml")).is_none());
        assert!(sensitive_reason(Path::new("skills/demo/SKILL.md")).is_none());
        assert!(sensitive_reason(Path::new("package.json")).is_none());
    }

    #[test]
    fn copy_tree_skips_sensitive_files() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-copytree");
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(src.join("skills/demo")).unwrap();
        std::fs::write(src.join("cordis.patch.yml"), b"[]").unwrap();
        std::fs::write(src.join("skills/demo/SKILL.md"), b"# demo").unwrap();
        std::fs::write(src.join(".env"), b"SECRET=1").unwrap();
        std::fs::write(src.join("server.pem"), b"key").unwrap();

        let mut skipped = Vec::new();
        let n = copy_tree_checked(&src, &dst, &mut skipped).unwrap();

        assert_eq!(n, 2, "应只复制 2 个正常文件");
        assert_eq!(skipped.len(), 2, "应跳过 2 个敏感文件");
        assert!(dst.join("cordis.patch.yml").is_file());
        assert!(dst.join("skills/demo/SKILL.md").is_file());
        assert!(!dst.join(".env").exists());
        assert!(!dst.join("server.pem").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---------- manifest 解析 ----------

    const MIN_PROFILE: &str = r#"{
        "manifestVersion": 5,
        "type": "profile",
        "name": "demo",
        "version": "1.0.0",
        "bundles": ["@deepseek-ai/dsh-base"],
        "dependencies": {"dsh-pet": "0.2.0"}
    }"#;

    #[test]
    fn parses_min_profile() {
        let m = Manifest::parse(MIN_PROFILE).unwrap();
        assert_eq!(m.kind, "profile");
        assert!(!m.is_dshhome());
        assert_eq!(m.launch_profile().as_deref(), Some("pack"));
        assert_eq!(m.display_name_text(), "demo");
    }

    #[test]
    fn localized_text_plain_and_map() {
        let plain: LocalizedText = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(plain.display(), "hello");

        let map: LocalizedText =
            serde_json::from_str(r#"{"en-US":"Hi","zh-CN":"你好"}"#).unwrap();
        assert_eq!(map.display(), "你好");
    }

    #[test]
    fn collects_all_errors() {
        // 缺 name、bundles 为空、type 非法 —— 应一次全报出来
        let bad = r#"{
            "manifestVersion": 5,
            "type": "banana",
            "version": "1.0.0"
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        let joined = errs.join("\n");
        assert!(joined.contains("name"), "应报缺 name: {}", joined);
        assert!(joined.contains("type"), "应报 type 非法: {}", joined);
    }

    #[test]
    fn rejects_deprecated_v1() {
        let bad = r#"{"manifestVersion":1,"type":"profile","name":"x","version":"1.0.0","bundles":["a"]}"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("v1")), "应报 v1 已废弃: {:?}", errs);
    }

    #[test]
    fn rejects_dshhome_without_default() {
        let bad = r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "profiles": {"a": {"bundles": ["b"]}}
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("defaultProfile")),
            "应报缺 defaultProfile: {:?}",
            errs
        );
    }

    #[test]
    fn rejects_dshhome_containing_web() {
        let bad = r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "defaultProfile": "web",
            "profiles": {"web": {"bundles": ["b"]}}
        }"#;
        let errs = Manifest::parse(bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("基线模板")),
            "应报含 web 模板: {:?}",
            errs
        );
    }

    #[test]
    fn accepts_v4_manifest() {
        let v4 = r#"{
            "manifestVersion": 4,
            "type": "profile",
            "name": "old",
            "version": "1.0.0",
            "bundles": ["a"],
            "dependencies": {}
        }"#;
        let m = Manifest::parse(v4).unwrap();
        assert_eq!(m.manifest_version, 4);
    }

    #[test]
    fn dshhome_launch_profile_is_default_profile() {
        let m = Manifest::parse(r#"{
            "manifestVersion": 5,
            "type": "dshhome",
            "name": "x",
            "version": "1.0.0",
            "defaultProfile": "whale",
            "profiles": {"whale": {"bundles": ["b"]}}
        }"#).unwrap();
        assert_eq!(m.launch_profile().as_deref(), Some("whale"));
    }

    #[test]
    fn accepts_v3_container() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-ok");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":3}"#, true);
        let ex = extract(&p).unwrap();
        assert_eq!(ex.marker.version, 3);
        assert_eq!(ex.marker.format, "dspack");
        assert!(ex.dir.join("manifest.json").is_file());
        ex.cleanup();
        assert!(!ex.dir.exists(), "cleanup 后临时目录应被删除");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

