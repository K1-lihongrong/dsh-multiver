//! catalog/inspect：官方组合包查询、运行时版本、outdated/view 输出解析。

use serde::Serialize;
use std::path::{Path, PathBuf};

use super::profile::read_pkg_meta;

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

/// 读版本内置的 DSH 运行时版本（`<版本>/node_modules/@deepseek-ai/dsh/package.json`）。
pub fn runtime_version(version_dir: &Path) -> Option<String> {
    let dir = version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh");
    read_pkg_meta(&dir).map(|m| m.version)
}

/// 一条「有新版本」信息（pnpm outdated --json 的解析结果）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
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
        if !current.is_empty() && !latest.is_empty() && version_is_newer(current, latest) {
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

/// 从 dsh/pnpm 混合输出中提取 `outdated --json` 的 JSON 行。
/// dsh 会先打印 profile 兼容性警告，因此不能直接对整个 output 做 JSON 解析。
pub fn parse_outdated_output(output: &str) -> Option<Vec<DsUpdate>> {
    for line in output.lines() {
        let line = line.trim();
        if (line.starts_with('{') || line.starts_with('['))
            && serde_json::from_str::<serde_json::Value>(line).is_ok()
        {
            return Some(parse_outdated_json(line));
        }
    }
    None
}

/// 简单 semver 比较：只用于判断注册表 latest 是否比已安装版本新。
/// 解析失败时返回 false，避免把 prerelease / git / 本地版本误报成可更新。
fn parse_semver(v: &str) -> Option<(u64, u64, u64, Vec<String>)> {
    let s = v.trim().trim_start_matches('v');
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = s.split_once('-').unwrap_or((s, ""));
    let mut nums = core.split('.');
    let major = nums.next()?.parse::<u64>().ok()?;
    let minor = nums.next()?.parse::<u64>().ok()?;
    let patch = nums.next()?.parse::<u64>().ok()?;
    if nums.next().is_some() {
        return None;
    }
    let pre = if pre.is_empty() {
        Vec::new()
    } else {
        pre.split('.').map(|x| x.to_string()).collect()
    };
    Some((major, minor, patch, pre))
}

pub fn version_is_newer(current: &str, latest: &str) -> bool {
    let Some((cmaj, cmin, cpat, cpre)) = parse_semver(current) else {
        return false;
    };
    let Some((lmaj, lmin, lpat, lpre)) = parse_semver(latest) else {
        return false;
    };
    if (lmaj, lmin, lpat) != (cmaj, cmin, cpat) {
        return (lmaj, lmin, lpat) > (cmaj, cmin, cpat);
    }
    if cpre.is_empty() != lpre.is_empty() {
        return lpre.is_empty();
    }
    for (c, l) in cpre.iter().zip(lpre.iter()) {
        let cn = c.parse::<u64>().ok();
        let ln = l.parse::<u64>().ok();
        match (cn, ln) {
            (Some(a), Some(b)) if a != b => return b > a,
            (Some(_), None) => return false,
            (None, Some(_)) => return true,
            _ => {
                if c != l {
                    return l > c;
                }
            }
        }
    }
    lpre.len() > cpre.len()
}

/// 解析 `pnpm view <pkg> version --json` 的输出。
///
/// 正常输出是 JSON 字符串（`"0.5.2"`），也可能被 dsh 的告警行包住；
/// 因此先按 JSON 字符串解析，失败时取最后一条像版本号的行。
pub fn parse_view_version(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if let Ok(serde_json::Value::String(v)) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    for line in trimmed.lines().rev() {
        let s = line.trim().trim_matches('"').trim();
        if s.is_empty() || s.contains(' ') || s.contains(':') {
            continue;
        }
        let first = s.chars().next().unwrap_or_default();
        if first.is_ascii_digit()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        {
            return Some(s.to_string());
        }
    }
    None
}
