//! dsh 插件管理（v0.3.0 · 双向桥 B1）。
//!
//! 作用：让 dsh-multiver 界面管理「某个 dsh 版本」的 dsh 插件。
//! 原理：dsh 插件 = npm 包，装在 `$DSH_HOME/profiles/<profile>/`。
//!       dsh 提供 `dsh plugin --profile <p> <pnpm-args>`（转发 pnpm）。
//!
//! 约束：
//! - **只读/操作 profile 目录与调 dsh CLI**，不碰 dsh 内部实现。
//! - dsh CLI 在 `<版本>/node_modules/.bin/dsh`。
//! - DSH_HOME 由调用方（commands）解析后传入（隔离/共享/官方）。

use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 单次插件操作的超时（pnpm 安装可能较慢，但不应无限阻塞）。
pub const PLUGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// 一个 profile 下的插件信息（从 profiles/<name>/package.json 的 dependencies 读）。
#[derive(Debug, Clone, Serialize)]
pub struct DsPlugin {
    pub name: String,
    pub version: String,
    pub is_bundle: bool,
    /// 来源：`"bundle"`（profile 模板自带）| `"user"`（用户 `dsh plugin add` 装的）。
    pub source: String,
    /// 归属：`"official"`（`@deepseek-ai/*`）| `"third_party"`（其它来源）。
    pub origin: String,
    /// `package.json` 里声明的版本范围（与 `version` 不同时给出，供 UI 提示）。
    pub spec: Option<String>,
    /// 包自带描述（`package.json` 的 description，供 UI tooltip）。
    pub description: Option<String>,
    /// 是否声明了 `dsh.bundle`（官方把「组合包」定义为声明该字段的包；普通依赖不是插件）。
    pub declares_bundle: bool,
    /// 是否属于 DSH 安装随附的官方组合包（`OPTIONAL_BUNDLES` / `DEFAULT_PROFILE_BUNDLES` / `ON_DEMAND_BUNDLES`）。
    pub official: bool,
}

/// 一个 profile（dsh 的插件集合）。
#[derive(Debug, Clone, Serialize)]
pub struct DsProfile {
    pub name: String,
    pub path: String,
    pub plugins: Vec<DsPlugin>,
}

/// 插件列表 + 当前 DSH_HOME 信息（让用户明确插件装在哪）。
#[derive(Debug, Clone, Serialize)]
pub struct DsPluginList {
    /// 实际使用的 DSH_HOME 路径
    pub dsh_home: String,
    /// 来源："isolated"（隔离）| "official"（复用官方 ~/.dsh）| "shared"（管理器共享）
    pub home_kind: String,
    pub profiles: Vec<DsProfile>,
}

/// profile 目录：`<DSH_HOME>/profiles`。
pub fn profiles_dir(dsh_home: &Path) -> PathBuf {
    dsh_home.join("profiles")
}

/// 列所有 profile（`profiles/` 下的子目录，排除 node_modules）。
///
/// `official` 为 DSH 安装随附的官方组合包名（用于与官方插件页语义对齐）。
pub fn list_profiles(dsh_home: &Path, official: &[String]) -> Vec<DsProfile> {
    let dir = profiles_dir(dsh_home);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name == "node_modules" {
            continue;
        }
        out.push(read_profile(&name, &p, official));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 读单个 profile（package.json 的 dependencies + dsh.profile.bundles）。
///
/// 版本优先取**实际安装版本**（`node_modules/<pkg>/package.json`），
/// 取不到时退回 `package.json` 里声明的 semver 范围（此时 `spec` 记录原范围）。
fn read_profile(name: &str, path: &Path, official: &[String]) -> DsProfile {
    let mut plugins: Vec<DsPlugin> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let pkg_path = path.join("package.json");
    let parent_nm = path
        .parent()
        .map(|p| p.join("node_modules"))
        .unwrap_or_default();
    let profile_nm = path.join("node_modules");
    let is_official = |pkg: &str| official.iter().any(|o| o == pkg);
    if let Ok(text) = std::fs::read_to_string(&pkg_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            // 1) dsh.profile.bundles：profile 内置的 bundle 层
            let bundles: Vec<String> = v
                .get("dsh")
                .and_then(|d| d.get("profile"))
                .and_then(|p| p.get("bundles"))
                .and_then(|b| b.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            // 2) dependencies：用户 add 的插件（bundles 之外）
            if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
                for (k, ver) in deps {
                    let declared = ver.as_str().map(|s| s.to_string());
                    let is_bundle = bundles.iter().any(|b| b == k);
                    let meta = pkg_meta(&profile_nm, &parent_nm, k);
                    let installed = meta.as_ref().map(|m| m.version.clone());
                    let (version, spec) = match (&installed, &declared) {
                        (Some(actual), Some(d)) if actual != d => (actual.clone(), Some(d.clone())),
                        (Some(actual), _) => (actual.clone(), None),
                        (None, Some(d)) => (d.clone(), None),
                        (None, None) => (String::new(), None),
                    };
                    seen.insert(k.clone());
                    plugins.push(DsPlugin {
                        name: k.clone(),
                        version,
                        is_bundle,
                        source: if is_bundle { "bundle" } else { "user" }.to_string(),
                        origin: plugin_origin(k).to_string(),
                        spec,
                        description: meta.as_ref().and_then(|m| m.description.clone()),
                        declares_bundle: meta.as_ref().map(|m| m.declares_bundle).unwrap_or(false),
                        official: is_official(k),
                    });
                }
            }

            // 3) bundles 里未出现在 dependencies 的（dsh 默认 profile 的 dependencies 常为空）
            //    版本从 <profile>/node_modules/<pkg> 或 <profiles>/node_modules/<pkg> 读。
            for b in &bundles {
                if seen.contains(b) {
                    continue;
                }
                let meta = pkg_meta(&profile_nm, &parent_nm, b);
                seen.insert(b.clone());
                plugins.push(DsPlugin {
                    name: b.clone(),
                    version: meta.as_ref().map(|m| m.version.clone()).unwrap_or_default(),
                    is_bundle: true,
                    source: "bundle".to_string(),
                    origin: plugin_origin(b).to_string(),
                    spec: None,
                    description: meta.as_ref().and_then(|m| m.description.clone()),
                    declares_bundle: meta.as_ref().map(|m| m.declares_bundle).unwrap_or(false),
                    official: is_official(b),
                });
            }
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    DsProfile {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        plugins,
    }
}

/// 已安装包的元信息（版本 / 描述 / 是否声明 `dsh.bundle`）。
struct PkgMeta {
    version: String,
    description: Option<String>,
    declares_bundle: bool,
}

fn pkg_meta(profile_nm: &Path, parent_nm: &Path, pkg: &str) -> Option<PkgMeta> {
    read_pkg_meta(&profile_nm.join(pkg)).or_else(|| read_pkg_meta(&parent_nm.join(pkg)))
}

fn read_pkg_meta(pkg_dir: &Path) -> Option<PkgMeta> {
    let text = std::fs::read_to_string(pkg_dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(PkgMeta {
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        declares_bundle: v.get("dsh").and_then(|d| d.get("bundle")).is_some(),
    })
}

/// DSH 安装随附的官方组合包名（OPTIONAL + DEFAULT + ON_DEMAND），用于对齐官方「随 DSH 提供」语义。
///
/// 实现：在版本目录里定位 `@deepseek-ai/dsh-app-boot`，用 node 读取其导出的三个数组；
/// 结果按版本目录缓存（一次进程生命周期）。node 不可用/读取失败时返回空（退化为只按 profile bundles 判断）。
pub fn official_bundles(version_dir: &Path) -> Vec<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    let key = version_dir.to_string_lossy().to_string();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock() {
        if let Some(v) = map.get(&key) {
            return v.clone();
        }
    }
    let out = query_official_bundles(version_dir).unwrap_or_default();
    if let Ok(mut map) = cache.lock() {
        map.insert(key, out.clone());
    }
    out
}

fn official_bundle_lib(version_dir: &Path) -> Option<PathBuf> {
    let nm = version_dir.join("node_modules");
    let direct = nm
        .join("@deepseek-ai")
        .join("dsh-app-boot")
        .join("lib")
        .join("index.js");
    if direct.exists() {
        return Some(direct);
    }
    let pnpm = nm.join(".pnpm");
    for e in std::fs::read_dir(pnpm).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with("@deepseek-ai+dsh-app-boot@") {
            let p = e
                .path()
                .join("node_modules")
                .join("@deepseek-ai")
                .join("dsh-app-boot")
                .join("lib")
                .join("index.js");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

fn query_official_bundles(version_dir: &Path) -> Option<Vec<String>> {
    let lib = official_bundle_lib(version_dir)?;
    // 官方随附的 bundle 分布在四处：
    //   OPTIONAL_BUNDLES（可选实验特性）、DEFAULT_PROFILE_BUNDLES（无模板时的默认）、
    //   ON_DEMAND_BUNDLES（按需子代理）、PROFILE_TEMPLATES[*].bundles（各 profile 模板，
    //   如 web 模板带入 dsh-web-app）。漏掉最后一项会把 dsh-web-app 误判为用户安装。
    const JS: &str = "const {pathToFileURL} = await import('node:url');\nconst m = await import(pathToFileURL(process.argv[1]).href);\nconst tpl = Object.values(m.PROFILE_TEMPLATES||{}).flatMap(t => t.bundles||[]);\nconsole.log(JSON.stringify([...new Set([...(m.OPTIONAL_BUNDLES||[]),...(m.DEFAULT_PROFILE_BUNDLES||[]),...(m.ON_DEMAND_BUNDLES||[]),...tpl])]));";
    let mut cmd = std::process::Command::new("node");
    cmd.arg("--input-type=module").arg("-e").arg(JS).arg(&lib);
    // GUI 进程无控制台，不设 CREATE_NO_WINDOW 会闪一下黑框
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).ok()
}

/// 读 profile 的版本豁免（官方 `compatibility.json`：`{ "pkg@版本": ["DSH 版本", ...] }`）。
pub fn read_exemptions(dsh_home: &Path, profile: &str) -> serde_json::Value {
    let file = profiles_dir(dsh_home)
        .join(profile)
        .join("compatibility.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// 官方「待决定的构建脚本」占位值（pnpm-workspace.yaml 的 allowBuilds 映射里）。
pub const BUILD_UNDECIDED: &str = "set this to true or false";

/// 读 profile 的 `pnpm-workspace.yaml` 中 allowBuilds 里「待决定」的包名。
pub fn read_pending_builds(profile_dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(profile_dir.join("pnpm-workspace.yaml")) else {
        return Vec::new();
    };
    parse_pending_builds(&text)
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn parse_pending_builds(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_allow = false;
    for line in text.lines() {
        if line.starts_with("allowBuilds:") {
            in_allow = true;
            continue;
        }
        if !in_allow {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_allow = false;
            continue;
        }
        if let Some((k, v)) = line.trim().split_once(':') {
            if unquote(v) == BUILD_UNDECIDED {
                out.push(unquote(k));
            }
        }
    }
    out
}

fn yaml_key(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\\\""))
}

/// 把待决定的构建脚本写成 `allowBuilds: { name: true }`（保留其余内容；官方格式）。
pub fn approve_builds(profile_dir: &Path, names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }
    let path = profile_dir.join("pnpm-workspace.yaml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 pnpm-workspace.yaml 失败：{e}"))?;
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut handled = vec![false; names.len()];
    let start = lines.iter().position(|l| l.starts_with("allowBuilds:"));
    match start {
        Some(si) => {
            let mut end = si + 1;
            while end < lines.len() {
                let line = &lines[end];
                if line.trim().is_empty() {
                    end += 1;
                    continue;
                }
                if !line.starts_with(' ') && !line.starts_with('\t') {
                    break;
                }
                if let Some((k, _)) = line.trim().split_once(':') {
                    let key = unquote(k);
                    if let Some(idx) = names.iter().position(|n| n == &key) {
                        let indent = line.len() - line.trim_start().len();
                        lines[end] = format!("{}{}: true", " ".repeat(indent), yaml_key(&key));
                        handled[idx] = true;
                    }
                }
                end += 1;
            }
            let mut insert_at = end;
            while insert_at > si + 1 && lines[insert_at - 1].trim().is_empty() {
                insert_at -= 1;
            }
            let mut extra = Vec::new();
            for (i, n) in names.iter().enumerate() {
                if !handled[i] {
                    extra.push(format!("  {}: true", yaml_key(n)));
                }
            }
            lines.splice(insert_at..insert_at, extra);
        }
        None => {
            if !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push("allowBuilds:".to_string());
            for n in names {
                lines.push(format!("  {}: true", yaml_key(n)));
            }
        }
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    // 原子写：先写临时文件再替换，避免半截文件（官方同类操作也用原子写）。
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, out).map_err(|e| format!("写入临时文件失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换 pnpm-workspace.yaml 失败：{e}"))?;
    Ok(())
}

/// 读版本内置的 DSH 运行时版本（`<版本>/node_modules/@deepseek-ai/dsh/package.json`）。
pub fn runtime_version(version_dir: &Path) -> Option<String> {
    let dir = version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh");
    read_pkg_meta(&dir).map(|m| m.version)
}

/// 归属判定（启发式）：npm scope 为 `@deepseek-ai/` 视为官方，其余视为第三方。
///
/// 仅按包名判断、不发网络请求；返回 `"official"` / `"third_party"`。
pub fn plugin_origin(name: &str) -> &'static str {
    if name.starts_with("@deepseek-ai/") {
        "official"
    } else {
        "third_party"
    }
}

/// 校验用户输入的 npm 包规格：`name` / `@scope/name`，可带 `@版本或标签`。
///
/// 拒绝空、空白、控制字符、以 `-` 开头（防止把 pnpm 选项当包名透传）与非法字符。
/// 允许的字符集与 npm 包名/常见 semver 范围一致（如 `pkg@^1.2.3`、`@scope/pkg@latest`）。
pub fn validate_package_spec(spec: &str) -> Result<(), String> {
    let s = spec.trim();
    if s.is_empty() {
        return Err("包名不能为空".into());
    }
    if s.len() > 214 {
        return Err("包名过长（最多 214 字符）".into());
    }
    if s.starts_with('-') {
        return Err("包名不能以 - 开头（禁止透传 pnpm 选项）".into());
    }
    if s.chars().any(|c| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, ';' | '&' | '|' | '`' | '$' | '"' | '\'' | '\\')
    }) {
        return Err("包名不能包含空白、控制字符或 shell 特殊字符".into());
    }

    // 拆出可选的 @版本/标签：scope 包名以 @ 开头，故用 rfind 且要求位置 > 0。
    let (body, version) = match s.rfind('@') {
        Some(i) if i > 0 => (&s[..i], Some(&s[i + 1..])),
        _ => (s, None),
    };
    if let Some(v) = version {
        if v.is_empty() {
            return Err("版本/标签不能为空（如 pkg@1.2.3）".into());
        }
        if !v.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '-' | '_' | '~' | '^' | '*' | '+' | '=' | '<' | '>')
        }) {
            return Err(format!("版本/标签含非法字符：{v}"));
        }
    }

    let (scope, name) = if let Some(rest) = body.strip_prefix('@') {
        match rest.split_once('/') {
            Some((sc, nm)) => (Some(sc), nm),
            None => return Err("scope 包名需形如 @scope/name".into()),
        }
    } else {
        (None, body)
    };
    let valid_part = |x: &str| {
        !x.is_empty()
            && x.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    if let Some(sc) = scope {
        if !valid_part(sc) {
            return Err("scope 含非法字符".into());
        }
    }
    if !valid_part(name) {
        return Err("包名含非法字符".into());
    }
    Ok(())
}

/// dsh CLI 路径（`<版本目录>/node_modules/.bin/dsh[.cmd]`）。
pub fn dsh_bin(version_dir: &Path) -> Option<PathBuf> {
    let bin = version_dir
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

/// dsh CLI 入口：优先用 `node <dsh>/lib/bin.js` 直接执行（避免 `cmd /C` 的参数转义，
/// 从而安全支持 GitHub 地址 / 本地目录等安装 spec）。
#[derive(Debug, Clone)]
pub struct DshCli {
    pub bin: PathBuf,
    pub js: Option<PathBuf>,
}

pub fn dsh_cli(version_dir: &Path) -> Option<DshCli> {
    let bin = dsh_bin(version_dir)?;
    let js = version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("lib")
        .join("bin.js");
    Some(DshCli {
        bin,
        js: js.exists().then_some(js),
    })
}

/// 安装 spec 校验：npm 包名 / GitHub 地址 / 本地目录路径（与官方安装对话框一致的三类输入）。
pub fn validate_install_spec(spec: &str) -> Result<(), String> {
    let s = spec.trim();
    if s.is_empty() {
        return Err("请输入插件包名、GitHub 地址或本地目录".into());
    }
    if s.len() > 2048 {
        return Err("插件标识过长".into());
    }
    if s.starts_with('-') {
        return Err("不能以 - 开头（禁止透传 pnpm 选项）".into());
    }
    if s.chars().any(|c| c.is_control()) {
        return Err("插件标识含控制字符".into());
    }
    if validate_package_spec(s).is_ok() {
        return Ok(());
    }
    let lower = s.to_ascii_lowercase();
    let is_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git+")
        || lower.starts_with("github:")
        || lower.starts_with("file:")
        || lower.starts_with("link:");
    let bytes = s.as_bytes();
    let is_path = lower.starts_with("./")
        || lower.starts_with("../")
        || lower.starts_with(".\\")
        || lower.starts_with("..\\")
        || lower.starts_with('/')
        || (bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_url && !is_path {
        return Err("请输入 npm 包名、GitHub 地址或本地目录".into());
    }
    if s.chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '`' | '$' | '"' | '\''))
    {
        return Err("路径/URL 含 shell 特殊字符".into());
    }
    Ok(())
}

/// 安装源校验（对齐官方规则：http(s)、无用户名/密码、无 `#` 片段）。
pub fn validate_registry_url(url: &str) -> Result<(), String> {
    let s = url.trim();
    let lower = s.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("安装源必须是 http(s) 地址".into());
    }
    if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("安装源不能含空白/控制字符".into());
    }
    if s.contains('#') {
        return Err("安装源不能包含 # 片段".into());
    }
    let after = s.split_once("://").map(|(_, r)| r).unwrap_or("");
    if let Some(at) = after.find('@') {
        let slash = after.find('/').unwrap_or(after.len());
        if at < slash {
            return Err("安装源不能包含用户名/密码".into());
        }
    }
    Ok(())
}

/// 一条「有新版本」信息（pnpm outdated --json 的解析结果）。
#[derive(Debug, Clone, Serialize)]
pub struct DsUpdate {
    pub name: String,
    pub current: String,
    pub latest: String,
}

pub fn parse_outdated_json(text: &str) -> Vec<DsUpdate> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim()) else {
        return Vec::new();
    };
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, info) in obj {
        let current = info
            .get("current")
            .and_then(|x| x.as_str())
            .unwrap_or_default();
        let latest = info
            .get("latest")
            .and_then(|x| x.as_str())
            .unwrap_or_default();
        if !current.is_empty() && !latest.is_empty() && current != latest {
            out.push(DsUpdate {
                name: name.clone(),
                current: current.to_string(),
                latest: latest.to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 校验 profile 名（dsh 会用它作目录名）：字母/数字/`.`/`-`/`_`，且不能以 `-` 开头。
pub fn validate_profile_name(name: &str) -> Result<(), String> {
    let s = name.trim();
    if s.is_empty() {
        return Err("profile 名不能为空".into());
    }
    if s.len() > 64 {
        return Err("profile 名过长（最多 64 字符）".into());
    }
    if s.starts_with('-') {
        return Err("profile 名不能以 - 开头".into());
    }
    if s == "." || s == ".." {
        return Err("profile 名不能是 . 或 ..".into());
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err("profile 名只能包含字母、数字、. - _".into());
    }
    Ok(())
}

/// 剥离 ANSI 转义序列（`ESC [ … 终止字节`），让日志/toast 不出现彩色控制码。
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next(); // '['
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 一次插件命令的执行结果。
#[derive(Debug)]
pub struct RunResult {
    pub ok: bool,
    pub output: String,
    pub timed_out: bool,
}

/// 读子进程输出并按行转发（解码 UTF-8/GBK + 剥 ANSI）。
fn pump_lines<R: std::io::Read + Send + 'static>(
    reader: R,
    tx: std::sync::mpsc::Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut br = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match br.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let decoded = crate::console_decode::decode_console(&buf);
                    let line = strip_ansi(decoded.trim_end_matches(['\r', '\n']));
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// 调 dsh CLI 执行插件操作（`dsh plugin --profile <p> <args...>`）。
///
/// - 输出按行流式回调（供前端进度展示），同时累积到返回值 `output`；
/// - 超过 `timeout` 杀掉子进程并返回 `timed_out = true`；
/// - 输出统一解码（D16）并剥离 ANSI。
pub fn run_dsh_plugin<F: FnMut(&str)>(
    dsh_bin: &Path,
    dsh_js: Option<&Path>,
    dsh_home: &Path,
    profile: &str,
    args: &[&str],
    timeout: Duration,
    mut on_line: F,
) -> RunResult {
    let mut cmd = if let Some(js) = dsh_js {
        // 直接 node 执行：避免 cmd /C 二次转义，支持 URL / 本地路径 spec
        let mut c = std::process::Command::new("node");
        c.arg(js);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(dsh_bin);
        c
    } else {
        std::process::Command::new(dsh_bin)
    };
    cmd.arg("plugin")
        .arg("--profile")
        .arg(profile)
        .args(args)
        .env("DSH_HOME", dsh_home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return RunResult {
                ok: false,
                output: format!("执行 dsh 失败：{e}"),
                timed_out: false,
            }
        }
    };

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(pump_lines(out, tx.clone()));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(pump_lines(err, tx.clone()));
    }
    drop(tx);

    let deadline = Instant::now() + timeout;
    let mut lines: Vec<String> = Vec::new();
    let mut ok = false;
    let mut timed_out = false;
    loop {
        while let Ok(line) = rx.try_recv() {
            on_line(&line);
            lines.push(line);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                ok = status.success();
                break;
            }
            Ok(None) => {}
            Err(e) => {
                lines.push(format!("等待 dsh 失败：{e}"));
                break;
            }
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for h in readers {
        let _ = h.join();
    }
    while let Ok(line) = rx.try_recv() {
        on_line(&line);
        lines.push(line);
    }
    RunResult {
        ok,
        output: lines.join("\n"),
        timed_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_dir_joins() {
        assert_eq!(
            profiles_dir(Path::new("H")),
            PathBuf::from("H").join("profiles")
        );
    }

    #[test]
    fn read_profile_from_bundles() {
        let tmp = std::env::temp_dir().join(format!("dsplug-bundles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("package.json"),
            r#"{"name":"p","dependencies":{},"dsh":{"profile":{"bundles":["@scope/a"]}}}"#,
        )
        .unwrap();
        // 模拟 <profiles>/node_modules/@scope/a/package.json
        let nm = tmp
            .parent()
            .unwrap()
            .join(format!("nm-{}", std::process::id()));
        let pkg = nm.join("@scope").join("a");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), r#"{"version":"1.2.3"}"#).unwrap();
        // 把 profile 放到 <nm 父>/profiles/web 结构下
        let profdir = nm
            .parent()
            .unwrap()
            .join(format!("prof-{}", std::process::id()));
        std::fs::create_dir_all(profdir.join("profiles")).unwrap();
        let web = profdir.join("profiles").join("web");
        std::fs::rename(&tmp, &web).unwrap();
        std::fs::create_dir_all(profdir.join("profiles").join("node_modules").join("@scope"))
            .unwrap();
        let _ = std::fs::rename(
            &pkg,
            profdir
                .join("profiles")
                .join("node_modules")
                .join("@scope")
                .join("a"),
        );

        let p = read_profile("web", &web, &[]);
        let a = p
            .plugins
            .iter()
            .find(|x| x.name == "@scope/a")
            .expect("bundle 应被列出");
        assert!(a.is_bundle);
        assert_eq!(a.version, "1.2.3");
        assert_eq!(a.source, "bundle");
        assert_eq!(a.origin, "third_party");
        let _ = std::fs::remove_dir_all(&profdir);
        let _ = std::fs::remove_dir_all(&nm);
    }

    #[test]
    fn plugin_origin_matches_scope() {
        assert_eq!(plugin_origin("@deepseek-ai/dsh-base"), "official");
        assert_eq!(plugin_origin("@scope/other"), "third_party");
        assert_eq!(plugin_origin("my-plugin"), "third_party");
    }

    #[test]
    fn validate_package_spec_accepts_common_forms() {
        for ok in [
            "lodash",
            "@scope/pkg",
            "@deepseek-ai/dsh-base",
            "pkg@1.2.3",
            "@scope/pkg@^1.0.0",
            "@scope/pkg@latest",
            "my-plugin@~2.1",
        ] {
            assert!(validate_package_spec(ok).is_ok(), "应接受：{ok}");
        }
    }

    #[test]
    fn validate_package_spec_rejects_bad_forms() {
        for bad in [
            "",
            "   ",
            "-D",
            "--registry=http://evil.example",
            "pkg; rm -rf /",
            "pkg && calc",
            "pkg|cat",
            "a b",
            "@scope",
            "pkg@",
            "pkg@1.2.3/x",
            "@scope/pkg@1.2.3 beta",
        ] {
            assert!(validate_package_spec(bad).is_err(), "应拒绝：{bad}");
        }
    }

    #[test]
    fn strip_ansi_removes_csi() {
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m"), "ok");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\u{1b}[1;31merr\u{1b}[m end"), "err end");
    }

    #[test]
    fn validate_profile_name_rules() {
        for ok in ["web", "tui", "rescue", "my-profile", "a.b_c"] {
            assert!(validate_profile_name(ok).is_ok(), "应接受：{ok}");
        }
        for bad in ["", "  ", "-x", "a b", "a/b", "a\\b", "a:b", "@x", ".", ".."] {
            assert!(validate_profile_name(bad).is_err(), "应拒绝：{bad}");
        }
    }

    #[test]
    fn read_profile_prefers_installed_version() {
        let base = std::env::temp_dir().join(format!("dsplug-ver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let profile = base.join("profiles").join("web");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(
            profile.join("package.json"),
            r#"{"dependencies":{"pkg":"^1.0.0"}}"#,
        )
        .unwrap();
        let pkg = base.join("profiles").join("node_modules").join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("package.json"), r#"{"version":"1.2.3"}"#).unwrap();

        let p = read_profile("web", &profile, &[]);
        let item = p
            .plugins
            .iter()
            .find(|x| x.name == "pkg")
            .expect("应列出 pkg");
        assert_eq!(item.version, "1.2.3");
        assert_eq!(item.spec.as_deref(), Some("^1.0.0"));
        assert_eq!(item.source, "user");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn read_missing_profile_is_empty() {
        let tmp = std::env::temp_dir().join(format!("dsplug-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let p = read_profile("x", &tmp, &[]);
        assert_eq!(p.name, "x");
        assert!(p.plugins.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── 执行层（run_dsh_plugin）测试：B1-09 ──
    fn unique_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dsplug-run-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(windows)]
    fn write_script(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("fake-dsh.cmd");
        std::fs::write(&p, format!("@echo off\r\n{}\r\n", body)).unwrap();
        p
    }

    #[cfg(unix)]
    fn write_script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-dsh.sh");
        std::fs::write(&p, format!("#!/bin/sh\n{}\n", body)).unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
        p
    }

    #[test]
    fn run_dsh_plugin_captures_output_and_status() {
        let dir = unique_dir("ok");
        let bin = write_script(&dir, "echo hello");
        let res = run_dsh_plugin(
            &bin,
            None,
            &dir,
            "web",
            &["list"],
            Duration::from_secs(10),
            |_| {},
        );
        assert!(!res.timed_out, "不应超时：{}", res.output);
        assert!(res.ok, "应成功：{}", res.output);
        assert!(res.output.contains("hello"), "应捕获输出：{}", res.output);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_dsh_plugin_reports_nonzero_exit() {
        let dir = unique_dir("fail");
        #[cfg(windows)]
        let bin = write_script(&dir, "exit /b 3");
        #[cfg(unix)]
        let bin = write_script(&dir, "exit 3");
        let res = run_dsh_plugin(
            &bin,
            None,
            &dir,
            "web",
            &["list"],
            Duration::from_secs(10),
            |_| {},
        );
        assert!(!res.ok, "非零退出应判失败");
        assert!(!res.timed_out, "不应超时");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_dsh_plugin_times_out() {
        let dir = unique_dir("hang");
        #[cfg(windows)]
        let bin = write_script(&dir, "ping -n 4 127.0.0.1 >nul");
        #[cfg(unix)]
        let bin = write_script(&dir, "sleep 3");
        let res = run_dsh_plugin(
            &bin,
            None,
            &dir,
            "web",
            &["list"],
            Duration::from_millis(300),
            |_| {},
        );
        assert!(res.timed_out, "应判定超时：{}", res.output);
        assert!(!res.ok, "超时不应算成功");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── B1-11/B1-12：官方信息查询与 allowBuilds 写入 ──
    #[test]
    fn parse_pending_builds_finds_sentinel() {
        let text = "packages:\n  - .\n\nallowBuilds:\n  esbuild: \"set this to true or false\"\n  sharp: true\n  \"@scope/native\": set this to true or false\n";
        let pending = parse_pending_builds(text);
        assert_eq!(
            pending,
            vec!["esbuild".to_string(), "@scope/native".to_string()]
        );
    }

    #[test]
    fn approve_builds_updates_mapping() {
        let dir = unique_dir("approve-update");
        let file = dir.join("pnpm-workspace.yaml");
        std::fs::write(
            &file,
            "packages:\n  - .\nallowBuilds:\n  esbuild: \"set this to true or false\"\n  sharp: true\n",
        )
        .unwrap();
        approve_builds(&dir, &["esbuild".to_string(), "@scope/native".to_string()]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("\"esbuild\": true"),
            "应把待决定项改为 true：{text}"
        );
        assert!(
            text.contains("\"@scope/native\": true"),
            "应追加新键：{text}"
        );
        assert!(text.contains("sharp: true"), "应保留已有键：{text}");
        assert!(read_pending_builds(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn approve_builds_creates_section() {
        let dir = unique_dir("approve-create");
        let file = dir.join("pnpm-workspace.yaml");
        std::fs::write(&file, "packages:\n  - .\n").unwrap();
        approve_builds(&dir, &["esbuild".to_string()]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("allowBuilds:"), "应创建 allowBuilds：{text}");
        assert!(text.contains("\"esbuild\": true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_exemptions_reads_compatibility_json() {
        let base = unique_dir("exempt");
        let home = base.join("home");
        let pdir = home.join("profiles").join("web");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("compatibility.json"),
            r#"{"pkg@1.2.3":["0.2.1-rc.2"]}"#,
        )
        .unwrap();
        let v = read_exemptions(&home, "web");
        assert_eq!(v["pkg@1.2.3"][0], "0.2.1-rc.2");
        assert_eq!(read_exemptions(&home, "nope"), serde_json::json!({}));
        let _ = std::fs::remove_dir_all(&base);
    }

    // ── 官方安装交互对齐：spec / registry / outdated 解析 ──
    #[test]
    fn validate_install_spec_accepts_official_forms() {
        for ok in [
            "dsh-plugin-whale-pet",
            "@scope/pkg",
            "pkg@1.2.3",
            "https://github.com/deepseek-ai/deepseek-harness",
            "git+https://github.com/x/y.git",
            "file:../local-plugin",
            "./local-plugin",
            "E:\\plugins\\local",
            "/home/user/local-plugin",
        ] {
            assert!(validate_install_spec(ok).is_ok(), "应接受：{ok}");
        }
        for bad in ["", "-D", "--registry=x", "a\nb", "a;b", "a&b", "a|b"] {
            assert!(validate_install_spec(bad).is_err(), "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn validate_registry_url_rules() {
        assert!(validate_registry_url("https://registry.npmjs.org").is_ok());
        assert!(validate_registry_url("https://registry.npmmirror.com").is_ok());
        assert!(validate_registry_url("http://localhost:4873").is_ok());
        for bad in [
            "",
            "registry.npmjs.org",
            "ftp://x",
            "https://user:pass@x.com",
            "https://x.com/#frag",
            "https://x.com/a b",
        ] {
            assert!(validate_registry_url(bad).is_err(), "应拒绝：{bad:?}");
        }
    }

    #[test]
    fn parse_outdated_json_lists_newer_versions() {
        let text = r#"{"a":{"current":"1.0.0","latest":"2.0.0"},"b":{"current":"1.0.0","latest":"1.0.0"}}"#;
        let out = parse_outdated_json(text);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "a");
        assert_eq!(out[0].latest, "2.0.0");
        assert!(parse_outdated_json("not json").is_empty());
    }
}
