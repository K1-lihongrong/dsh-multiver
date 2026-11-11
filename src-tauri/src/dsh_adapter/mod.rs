//! DSH 版本适配层（A2-1）。
//!
//! 不同 DSH 版本在 CLI 入口、profile 目录和能力开关上可能不同。调用方只依赖
//! [`DshAdapter`]；无法识别的版本会落到 [`UnknownAdapter`]，并采用保守能力与
//! 标准 profile 布局，避免误开新版专属能力（例如 `allow-version`）。

mod detect;
mod v0_1;
mod v0_3;

pub use detect::detect;
pub use v0_1::V01Adapter;
pub use v0_3::V03Adapter;

use std::path::{Path, PathBuf};

/// DSH 行为代际。`Unknown` 是安全默认：不启用版本专属能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshGeneration {
    V01,
    V03,
    Unknown,
}

/// 适配层暴露给插件命令路径的能力开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterCapabilities {
    /// 是否支持官方 `dsh plugin allow-version` / `revoke-version`。
    pub allow_version: bool,
    /// 是否走通用 `dsh plugin --profile <p> <args>` 命令路径。
    pub plugin_commands: bool,
}

impl AdapterCapabilities {
    pub const fn new(allow_version: bool, plugin_commands: bool) -> Self {
        Self {
            allow_version,
            plugin_commands,
        }
    }

    pub const fn supports_allow_version(self) -> bool {
        self.allow_version
    }

    pub const fn supports_plugin_commands(self) -> bool {
        self.plugin_commands
    }
}

/// 统一 DSH CLI/profile/capability 行为。
pub trait DshAdapter: Send + Sync {
    fn generation(&self) -> DshGeneration;
    fn supports_allow_version(&self) -> bool;
    fn profile_dir(&self, home: &Path, profile: &str) -> PathBuf;
    /// 版本目录下 dsh CLI 入口：`node_modules/.bin/dsh[.cmd]`。
    fn cli_entry(&self, version_dir: &Path) -> PathBuf;
    fn capabilities(&self) -> AdapterCapabilities;
}

/// v0.1/v0.3 与未知版本共用的标准 profile 布局：`<DSH_HOME>/profiles/<profile>`。
pub(crate) fn standard_profile_dir(home: &Path, profile: &str) -> PathBuf {
    home.join("profiles").join(profile)
}

/// 标准 dsh CLI 入口名：Windows 为 `dsh.cmd`，Unix 为 `dsh`。
pub(crate) fn standard_cli_entry(version_dir: &Path) -> PathBuf {
    version_dir
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" })
}

/// 未知版本的安全适配器：只保留通用插件命令，不启用 allow-version。
#[derive(Debug, Default, Clone, Copy)]
pub struct UnknownAdapter;

impl DshAdapter for UnknownAdapter {
    fn generation(&self) -> DshGeneration {
        DshGeneration::Unknown
    }

    fn supports_allow_version(&self) -> bool {
        self.capabilities().supports_allow_version()
    }

    fn profile_dir(&self, home: &Path, profile: &str) -> PathBuf {
        standard_profile_dir(home, profile)
    }

    fn cli_entry(&self, version_dir: &Path) -> PathBuf {
        standard_cli_entry(version_dir)
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::new(false, true)
    }
}

/// 按版本字符串选择适配器；未知/空/垃圾版本落到安全默认。
pub fn adapter_for(version: &str) -> Box<dyn DshAdapter> {
    match detect(version) {
        DshGeneration::V01 => Box::new(V01Adapter),
        DshGeneration::V03 => Box::new(V03Adapter),
        DshGeneration::Unknown => Box::new(UnknownAdapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_name() -> &'static str {
        if cfg!(windows) {
            "dsh.cmd"
        } else {
            "dsh"
        }
    }

    #[test]
    fn detects_known_generations() {
        assert_eq!(detect("0.0.1-rc.5"), DshGeneration::V01);
        assert_eq!(detect("0.1.7-rc.2"), DshGeneration::V01);
        assert_eq!(detect("0.2.2-rc.1"), DshGeneration::V03);
        assert_eq!(detect("v0.3.0"), DshGeneration::V03);
    }

    #[test]
    fn empty_and_garbage_use_safe_unknown() {
        for version in ["", "   ", "garbage", "latest", "v", "0", "0.x.1", "1.0.0"] {
            assert_eq!(
                detect(version),
                DshGeneration::Unknown,
                "应识别为未知版本：{version:?}"
            );
            let adapter = adapter_for(version);
            assert_eq!(adapter.generation(), DshGeneration::Unknown);
            assert!(!adapter.supports_allow_version());
            assert!(!adapter.capabilities().supports_allow_version());
        }
    }

    #[test]
    fn allow_version_is_gated_by_generation() {
        assert!(!adapter_for("0.1.7-rc.2").supports_allow_version());
        assert!(adapter_for("0.2.2-rc.1").supports_allow_version());
        assert!(!adapter_for("unknown-version").supports_allow_version());
    }

    #[test]
    fn profile_dir_uses_standard_layout() {
        let expected = PathBuf::from("H").join("profiles").join("web");
        for version in ["0.0.1-rc.5", "0.2.2-rc.1", "garbage"] {
            let adapter = adapter_for(version);
            assert_eq!(adapter.profile_dir(Path::new("H"), "web"), expected);
        }
    }

    #[test]
    fn cli_entry_uses_platform_binary_name() {
        let version_dir = Path::new("V");
        let expected = version_dir
            .join("node_modules")
            .join(".bin")
            .join(cli_name());
        for version in ["0.0.1-rc.5", "0.2.2-rc.1", "garbage"] {
            let adapter = adapter_for(version);
            assert_eq!(adapter.cli_entry(version_dir), expected);
        }
    }

    #[test]
    fn capabilities_expose_plugin_commands() {
        for version in ["0.1.7-rc.2", "0.2.2-rc.1", "garbage"] {
            assert!(adapter_for(version)
                .capabilities()
                .supports_plugin_commands());
        }
    }
}
