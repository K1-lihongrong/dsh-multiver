//! profile 读取：解析 `$DSH_HOME/profiles/<profile>/package.json` 与已安装包元信息。

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 一个 profile 下的插件信息（从 profiles/<name>/package.json 的 dependencies 读）。
#[derive(Debug, Clone, Serialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DsProfile {
    pub name: String,
    pub path: String,
    pub plugins: Vec<DsPlugin>,
}

/// 插件列表 + 当前 DSH_HOME 信息（让用户明确插件装在哪）。
#[derive(Debug, Clone, Serialize, specta::Type)]
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
pub(crate) fn read_profile(name: &str, path: &Path, official: &[String]) -> DsProfile {
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
                        (None, Some(d)) => (String::new(), Some(d.clone())),
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
pub(crate) struct PkgMeta {
    pub(crate) version: String,
    description: Option<String>,
    declares_bundle: bool,
}

fn pkg_meta(profile_nm: &Path, parent_nm: &Path, pkg: &str) -> Option<PkgMeta> {
    read_pkg_meta(&profile_nm.join(pkg)).or_else(|| read_pkg_meta(&parent_nm.join(pkg)))
}

pub(crate) fn read_pkg_meta(pkg_dir: &Path) -> Option<PkgMeta> {
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
