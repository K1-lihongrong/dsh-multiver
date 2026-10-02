use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 管理器配置，持久化到 <根目录>/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 数据根目录。为空时表示使用默认（exe 同目录）
    pub root_dir: Option<String>,
    /// 当前默认版本号（终端 dsh 指向的版本）
    pub default_version: Option<String>,
    /// 开启数据隔离的版本号列表（这些版本使用独立的 DSH_HOME）
    #[serde(default)]
    pub isolated_versions: Vec<String>,
    /// 已知安装失败的版本号（依赖已下架的私有包等）。
    /// 列表里这些版本标灰 + 提示，避免用户反复踩坑。
    /// 安装成功后自动移除；用户可在界面手动清除。
    #[serde(default)]
    pub broken_versions: Vec<String>,
    /// 维护相关配置与状态。
    #[serde(default)]
    pub maintenance: MaintenanceConfig,
}

/// 维护相关配置与状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceConfig {
    /// 是否启用启动时自动维护（默认开）。
    pub auto_enabled: bool,
    /// 上次 store prune 的 unix 时间戳（秒）。
    #[serde(default)]
    pub last_prune_at: Option<u64>,
    /// 上次清理孤立 webview 的 unix 时间戳（秒）。
    #[serde(default)]
    pub last_cleanup_at: Option<u64>,
    /// 上次清理孤立 webview 的条目数。
    #[serde(default)]
    pub last_cleanup_count: u64,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            auto_enabled: true,
            last_prune_at: None,
            last_cleanup_at: None,
            last_cleanup_count: 0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_dir: None,
            default_version: None,
            isolated_versions: Vec::new(),
            broken_versions: Vec::new(),
            maintenance: MaintenanceConfig::default(),
        }
    }
}

impl Config {
    /// 配置文件名
    pub const FILE_NAME: &'static str = "config.json";

    /// 读取配置。config_path 为管理器自身的目录（exe 所在目录）
    pub fn load(manager_dir: &Path) -> Self {
        let path = manager_dir.join(Self::FILE_NAME);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&text) {
                return cfg;
            }
        }
        Config::default()
    }

    /// 保存配置到管理器目录
    pub fn save(&self, manager_dir: &Path) -> std::io::Result<()> {
        let path = manager_dir.join(Self::FILE_NAME);
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }

    /// 解析出实际的数据根目录
    pub fn resolve_root(&self, manager_dir: &Path) -> PathBuf {
        match &self.root_dir {
            Some(r) if !r.trim().is_empty() => PathBuf::from(r),
            _ => manager_dir.to_path_buf(),
        }
    }

    /// 根据一次安装结果维护 `broken_versions`：
    /// - 成功：移除该版本标记
    /// - 失败且分类为「私有包/不完整」：加入标记
    /// 返回配置是否发生变化（调用方据此决定是否保存）。
    pub fn apply_install_result(&mut self, version: &str, ok: bool, err_kind: &str) -> bool {
        if ok {
            if self.broken_versions.iter().any(|v| v == version) {
                self.broken_versions.retain(|v| v != version);
                return true;
            }
            false
        } else if err_kind == "private-package" || err_kind == "incomplete-version" {
            if !self.broken_versions.iter().any(|v| v == version) {
                self.broken_versions.push(version.to_string());
                return true;
            }
            false
        } else {
            false
        }
    }

    /// 卸载某版本时清理配置中对它的引用：默认版本、隔离标记。
    /// 返回配置是否发生变化。
    pub fn remove_version_refs(&mut self, version: &str) -> bool {
        let mut changed = false;
        if self.default_version.as_deref() == Some(version) {
            self.default_version = None;
            changed = true;
        }
        if self.isolated_versions.iter().any(|v| v == version) {
            self.isolated_versions.retain(|v| v != version);
            changed = true;
        }
        changed
    }
}

/// 由根目录派生出的各个子目录
pub struct Dirs {
    pub root: PathBuf,
    pub versions: PathBuf,
    pub home: PathBuf,
    pub store: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    /// WebView2 数据目录根（每个版本一个独立子目录，避免共享累积导致 431）
    pub webview: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self {
            versions: root.join("versions"),
            home: root.join("home"),
            store: root.join("store"),
            cache: root.join("cache"),
            state: root.join("state"),
            webview: root.join("webview"),
            root,
        }
    }

    /// 确保各子目录存在
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.versions)?;
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(&self.store)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(&self.state)?;
        std::fs::create_dir_all(&self.webview)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-cfgtest-{}-{}",
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
    fn default_resolve_root_is_manager_dir() {
        let cfg = Config::default();
        let mdir = PathBuf::from("C:\\some\\manager");
        assert_eq!(cfg.resolve_root(&mdir), mdir);
    }

    #[test]
    fn empty_or_blank_root_falls_back_to_manager_dir() {
        let mdir = PathBuf::from("C:\\manager");
        let mut cfg = Config::default();
        cfg.root_dir = Some("   ".to_string());
        assert_eq!(cfg.resolve_root(&mdir), mdir);
        cfg.root_dir = Some("".to_string());
        assert_eq!(cfg.resolve_root(&mdir), mdir);
    }

    #[test]
    fn explicit_root_is_used() {
        let cfg = Config {
            root_dir: Some("D:\\data".to_string()),
            ..Config::default()
        };
        assert_eq!(cfg.resolve_root(&PathBuf::from("C:\\m")), PathBuf::from("D:\\data"));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tmp("roundtrip");
        let cfg = Config {
            root_dir: Some("D:\\x".to_string()),
            default_version: Some("0.1.7".to_string()),
            isolated_versions: vec!["0.1.7".to_string()],
            broken_versions: vec!["0.0.1".to_string()],
            maintenance: MaintenanceConfig {
                auto_enabled: false,
                last_prune_at: Some(123),
                last_cleanup_at: Some(456),
                last_cleanup_count: 3,
            },
        };
        cfg.save(&dir).unwrap();
        let back = Config::load(&dir);
        assert_eq!(back.default_version.as_deref(), Some("0.1.7"));
        assert_eq!(back.isolated_versions, vec!["0.1.7".to_string()]);
        assert_eq!(back.broken_versions, vec!["0.0.1".to_string()]);
        assert!(!back.maintenance.auto_enabled);
        assert_eq!(back.maintenance.last_prune_at, Some(123));
        assert_eq!(back.maintenance.last_cleanup_count, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tmp("missing");
        let cfg = Config::load(&dir);
        assert!(cfg.root_dir.is_none());
        assert!(cfg.default_version.is_none());
        assert!(cfg.maintenance.auto_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_corrupt_file_returns_default() {
        let dir = tmp("corrupt");
        std::fs::write(dir.join(Config::FILE_NAME), "{ not valid json").unwrap();
        let cfg = Config::load(&dir);
        assert!(cfg.default_version.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dirs_new_derives_expected_subdirs() {
        // 用 join 构造期望值，避免平台路径分隔符差异（Windows \ vs Unix /）
        let d = Dirs::new(PathBuf::from("R"));
        assert_eq!(d.versions, PathBuf::from("R").join("versions"));
        assert_eq!(d.home, PathBuf::from("R").join("home"));
        assert_eq!(d.webview, PathBuf::from("R").join("webview"));
    }


    // ── apply_install_result：broken_versions 维护 ──

    #[test]
    fn install_success_removes_broken_mark() {
        let mut c = Config::default();
        c.broken_versions = vec!["0.1.0".to_string(), "0.2.0".to_string()];
        let changed = c.apply_install_result("0.1.0", true, "");
        assert!(changed);
        assert_eq!(c.broken_versions, vec!["0.2.0".to_string()]);
    }

    #[test]
    fn install_success_no_mark_is_noop() {
        let mut c = Config::default();
        assert!(!c.apply_install_result("0.1.0", true, ""));
        assert!(c.broken_versions.is_empty());
    }

    #[test]
    fn install_failure_private_adds_mark() {
        let mut c = Config::default();
        let changed = c.apply_install_result("0.1.0", false, "private-package");
        assert!(changed);
        assert_eq!(c.broken_versions, vec!["0.1.0".to_string()]);
    }

    #[test]
    fn install_failure_incomplete_adds_mark() {
        let mut c = Config::default();
        assert!(c.apply_install_result("0.1.0", false, "incomplete-version"));
        assert_eq!(c.broken_versions, vec!["0.1.0".to_string()]);
    }

    #[test]
    fn install_failure_network_does_not_mark() {
        let mut c = Config::default();
        assert!(!c.apply_install_result("0.1.0", false, "network"));
        assert!(c.broken_versions.is_empty());
    }

    #[test]
    fn install_failure_already_marked_is_noop() {
        let mut c = Config::default();
        c.broken_versions = vec!["0.1.0".to_string()];
        assert!(!c.apply_install_result("0.1.0", false, "private-package"));
        assert_eq!(c.broken_versions.len(), 1);
    }

    // ── remove_version_refs：卸载时清理引用 ──

    #[test]
    fn remove_refs_clears_default_and_isolated() {
        let mut c = Config {
            default_version: Some("0.1.0".to_string()),
            isolated_versions: vec!["0.1.0".to_string()],
            ..Config::default()
        };
        assert!(c.remove_version_refs("0.1.0"));
        assert!(c.default_version.is_none());
        assert!(c.isolated_versions.is_empty());
    }

    #[test]
    fn remove_refs_partial() {
        // 只是默认版本、不在隔离列表
        let mut c = Config {
            default_version: Some("0.1.0".to_string()),
            ..Config::default()
        };
        assert!(c.remove_version_refs("0.1.0"));
        assert!(c.default_version.is_none());
    }

    #[test]
    fn remove_refs_no_match_is_noop() {
        let mut c = Config {
            default_version: Some("0.2.0".to_string()),
            isolated_versions: vec!["0.2.0".to_string()],
            ..Config::default()
        };
        assert!(!c.remove_version_refs("0.1.0"));
        assert_eq!(c.default_version.as_deref(), Some("0.2.0"));
        assert_eq!(c.isolated_versions, vec!["0.2.0".to_string()]);
    }

    #[test]
    fn dirs_ensure_creates_all() {
        let root = tmp("ensure");
        let d = Dirs::new(root.clone());
        d.ensure().unwrap();
        assert!(d.versions.exists());
        assert!(d.home.exists());
        assert!(d.store.exists());
        assert!(d.cache.exists());
        assert!(d.state.exists());
        assert!(d.webview.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
