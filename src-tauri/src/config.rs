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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_dir: None,
            default_version: None,
            isolated_versions: Vec::new(),
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
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self {
            versions: root.join("versions"),
            home: root.join("home"),
            store: root.join("store"),
            root,
        }
    }

    /// 确保三个子目录存在
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.versions)?;
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(&self.store)?;
        Ok(())
    }
}
