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
    /// 来源：`"bundle"`（profile 模板自带）| `"user"`（用户 `dsh plugin add` 装的）。
    pub source: String,
    /// 归属：`"official"`（`@deepseek-ai/*`）| `"third_party"`（其它来源）。
    pub origin: String,
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
                        source: if is_bundle { "bundle" } else { "user" }.to_string(),
                        origin: plugin_origin(k).to_string(),
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
                    source: "bundle".to_string(),
                    origin: plugin_origin(b).to_string(),
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
            let mut text = crate::console_decode::decode_console(&out.stdout);
            let err = crate::console_decode::decode_console(&out.stderr);
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
    fn read_missing_profile_is_empty() {
        let tmp = std::env::temp_dir().join(format!("dsplug-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let p = read_profile("x", &tmp);
        assert_eq!(p.name, "x");
        assert!(p.plugins.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
