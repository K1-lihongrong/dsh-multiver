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
use std::path::{Path, PathBuf};

/// 一个 profile 下的插件信息（从 profiles/<name>/package.json 的 dependencies 读）。
#[derive(Debug, Clone, Serialize)]
pub struct DsPlugin {
    pub name: String,
    pub version: String,
    pub is_bundle: bool,
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
pub fn list_profiles(dsh_home: &Path) -> Vec<DsProfile> {
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
        out.push(read_profile(&name, &p));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 读单个 profile（package.json 的 dependencies + dsh.profile.bundles）。
fn read_profile(name: &str, path: &Path) -> DsProfile {
    let mut plugins: Vec<DsPlugin> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let pkg_path = path.join("package.json");
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
                    let version = ver.as_str().unwrap_or("").to_string();
                    let is_bundle = bundles.iter().any(|b| b == k);
                    seen.insert(k.clone());
                    plugins.push(DsPlugin {
                        name: k.clone(),
                        version,
                        is_bundle,
                    });
                }
            }

            // 3) bundles 里未出现在 dependencies 的（dsh 默认 profile 的 dependencies 常为空）
            //    版本从 <profile>/node_modules/<pkg>/package.json 或 <profiles>/node_modules/<pkg> 读。
            let parent_nm = path
                .parent()
                .map(|p| p.join("node_modules"))
                .unwrap_or_default();
            for b in &bundles {
                if seen.contains(b) {
                    continue;
                }
                let ver = read_pkg_version(&path.join("node_modules").join(b))
                    .or_else(|| read_pkg_version(&parent_nm.join(b)))
                    .unwrap_or_default();
                seen.insert(b.clone());
                plugins.push(DsPlugin {
                    name: b.clone(),
                    version: ver,
                    is_bundle: true,
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

/// 读 <pkg>/package.json 的 version 字段。
fn read_pkg_version(pkg_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(pkg_dir.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
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

/// 调 dsh CLI 执行插件操作（`dsh plugin --profile <p> <args...>`）。
///
/// 返回 (成功, 输出文本)。dsh 侧输出全量带回，供界面展示。
pub fn run_dsh_plugin(
    dsh_bin: &Path,
    dsh_home: &Path,
    profile: &str,
    args: &[&str],
) -> (bool, String) {
    let mut cmd = if cfg!(windows) {
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
        .stdin(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&err);
            }
            (out.status.success(), text)
        }
        Err(e) => (false, format!("执行 dsh 失败：{}", e)),
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

        let p = read_profile("web", &web);
        let a = p
            .plugins
            .iter()
            .find(|x| x.name == "@scope/a")
            .expect("bundle 应被列出");
        assert!(a.is_bundle);
        assert_eq!(a.version, "1.2.3");
        let _ = std::fs::remove_dir_all(&profdir);
        let _ = std::fs::remove_dir_all(&nm);
    }

    #[test]
    fn read_missing_profile_is_empty() {
        let tmp = std::env::temp_dir().join(format!("dsplug-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let p = read_profile("x", &tmp);
        assert_eq!(p.name, "x");
        assert!(p.plugins.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
