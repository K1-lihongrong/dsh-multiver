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
