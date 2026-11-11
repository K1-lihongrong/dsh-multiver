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

mod catalog;
mod diagnostics;
mod exempt;
mod install;
mod profile;

#[allow(unused_imports)]
pub use catalog::{
    official_bundles, parse_outdated_json, parse_outdated_output, parse_view_version,
    runtime_version, version_is_newer, DsUpdate,
};
#[allow(unused_imports)]
pub use diagnostics::{
    dsh_bin, dsh_cli, dsh_cli_from_bin, run_dsh_plugin, strip_ansi, DshCli, RunResult,
};
#[allow(unused_imports)]
pub use exempt::{approve_builds, read_exemptions, read_pending_builds, BUILD_UNDECIDED};
#[allow(unused_imports)]
pub use install::{
    execute_plugin_update, rollback_add_spec, validate_install_spec, validate_package_spec,
    validate_registry_url, PluginUpdatePlan, PluginUpdateResult, PluginUpdateStatus,
    PluginUpdateStep, PluginUpdateTarget,
};
#[allow(unused_imports)]
pub use profile::{
    list_profiles, plugin_origin, profiles_dir, validate_profile_name, DsPlugin, DsPluginList,
    DsProfile,
};

#[cfg(test)]
pub(crate) use exempt::parse_pending_builds;
#[cfg(test)]
pub(crate) use profile::read_profile;

use std::time::Duration;

/// 单次插件操作的超时（pnpm 安装可能较慢，但不应无限阻塞）。
pub const PLUGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    #[test]
    fn profiles_dir_joins() {
        assert_eq!(
            profiles_dir(Path::new("H")),
            PathBuf::from("H").join("profiles")
        );
    }

    #[test]
    fn missing_installed_dependency_keeps_spec_but_not_fake_version() {
        let base = std::env::temp_dir().join(format!("dsplug-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("package.json"),
            r#"{"name":"p","dependencies":{"missing-plugin":"^1.2.3"}}"#,
        )
        .unwrap();
        let p = read_profile("web", &base, &[]);
        let plugin = p
            .plugins
            .iter()
            .find(|x| x.name == "missing-plugin")
            .unwrap();
        assert_eq!(plugin.version, "");
        assert_eq!(plugin.spec.as_deref(), Some("^1.2.3"));
        let _ = std::fs::remove_dir_all(&base);
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

    fn update_plan() -> PluginUpdatePlan {
        PluginUpdatePlan {
            name: "pkg".into(),
            current: "1.0.0".into(),
            latest: "2.0.0".into(),
            original_spec: "pkg@^1.0.0".into(),
        }
    }

    #[test]
    fn plugin_update_success_runs_remove_then_install() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let mut run = |step, args: &[&str]| {
            calls
                .borrow_mut()
                .push((step, args.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
            Ok(String::new())
        };
        let out = execute_plugin_update(&update_plan(), &mut run, || Ok(()));
        assert_eq!(out.status, PluginUpdateStatus::Updated);
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, PluginUpdateStep::Remove);
        assert_eq!(calls[0].1, vec!["remove", "pkg"]);
        assert_eq!(calls[1].0, PluginUpdateStep::Install);
        assert_eq!(calls[1].1, vec!["add", "pkg@2.0.0"]);
    }

    #[test]
    fn install_failure_rolls_back_original_spec() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let mut run = |step, args: &[&str]| {
            calls
                .borrow_mut()
                .push((step, args.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
            if step == PluginUpdateStep::Install {
                Err("network down".into())
            } else {
                Ok(String::new())
            }
        };
        let out = execute_plugin_update(&update_plan(), &mut run, || Ok(()));
        assert_eq!(out.status, PluginUpdateStatus::RolledBack);
        assert!(out.message.as_deref().unwrap().contains("network down"));
        let calls = calls.borrow();
        assert_eq!(calls[2].0, PluginUpdateStep::Rollback);
        assert_eq!(calls[2].1, vec!["add", "pkg@^1.0.0"]);
    }

    #[test]
    fn rollback_failure_reports_both_errors() {
        let mut run = |step: PluginUpdateStep, _args: &[&str]| match step {
            PluginUpdateStep::Remove | PluginUpdateStep::Install => Ok(String::new()),
            PluginUpdateStep::Rollback => Err("rollback denied".into()),
        };
        let out = execute_plugin_update(&update_plan(), &mut run, || Err("verify failed".into()));
        assert_eq!(out.status, PluginUpdateStatus::RollbackFailed);
        assert_eq!(out.rollback_error.as_deref(), Some("rollback denied"));
        assert!(out.message.as_deref().unwrap().contains("verify failed"));
    }

    #[test]
    fn remove_failure_is_reported_without_rollback() {
        use std::cell::RefCell;
        let calls = RefCell::new(0usize);
        let mut run = |_step, _args: &[&str]| {
            *calls.borrow_mut() += 1;
            Err("remove blocked".into())
        };
        let out = execute_plugin_update(&update_plan(), &mut run, || Ok(()));
        assert_eq!(out.status, PluginUpdateStatus::Failed);
        assert_eq!(*calls.borrow(), 1, "remove 失败不应再尝试回滚");
    }

    #[test]
    fn rollback_spec_preserves_semver_and_non_registry_sources() {
        assert_eq!(
            rollback_add_spec("pkg", Some("^1.2.3"), "1.2.4").as_deref(),
            Some("pkg@^1.2.3")
        );
        assert_eq!(
            rollback_add_spec("pkg", Some("github:user/repo"), "1.2.3").as_deref(),
            Some("github:user/repo")
        );
        assert_eq!(
            rollback_add_spec("pkg", Some("file:../pkg"), "1.2.3").as_deref(),
            Some("file:../pkg")
        );
        assert_eq!(
            rollback_add_spec("pkg", None, "1.2.3").as_deref(),
            Some("pkg@1.2.3")
        );
        assert_eq!(rollback_add_spec("pkg", None, ""), None);
    }

    #[test]
    fn version_is_newer_handles_semver_and_prerelease() {
        assert!(version_is_newer("0.5.1", "0.5.2"));
        assert!(version_is_newer("1.9.58", "1.9.61"));
        assert!(!version_is_newer("0.0.1-rc.5", "0.0.1-rc.1"));
        assert!(version_is_newer("0.0.1-rc.1", "0.0.1-rc.5"));
        assert!(version_is_newer("0.0.1-rc.1", "0.0.1"));
        assert!(!version_is_newer("0.0.1", "0.0.1-rc.1"));
        assert!(!version_is_newer("not-semver", "1.0.0"));
    }

    #[test]
    fn parse_outdated_output_ignores_dsh_warning_lines() {
        let output = "dsh: warning: something\n{}\ndsh: it stays installed\n";
        assert_eq!(parse_outdated_output(output), Some(Vec::new()));
        let output = "dsh: warning\n{\"dsh-ops\":{\"current\":\"0.5.1\",\"latest\":\"0.5.2\"}}\n";
        let parsed = parse_outdated_output(output).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "dsh-ops");
        assert_eq!(parsed[0].latest, "0.5.2");
        assert_eq!(parse_outdated_output("no json here"), None);
    }

    #[test]
    fn parse_view_version_handles_json_and_wrapped_output() {
        assert_eq!(parse_view_version("\"0.5.2\"").as_deref(), Some("0.5.2"));
        assert_eq!(
            parse_view_version("dsh: warning: something\n0.5.2\n").as_deref(),
            Some("0.5.2")
        );
        assert_eq!(parse_view_version(""), None);
        assert_eq!(parse_view_version("not-a-version"), None);
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
