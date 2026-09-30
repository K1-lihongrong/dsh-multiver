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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_dir: None,
            default_version: None,
            isolated_versions: Vec::new(),
            broken_versions: Vec::new(),
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
#[derive(Clone)]
pub struct Dirs {
    pub root: PathBuf,
    /// 普通 dsh 版本（主干的地盘）
    pub versions: PathBuf,
    /// 整合包实例（本分支的地盘）
    ///
    /// 与 versions/ 分开存放：主干（不含整合包支持）只扫描 versions/，
    /// 不会把整合包实例误显示为普通版本、误删或误设为默认。
    pub modpacks: PathBuf,
    pub home: PathBuf,
    pub store: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    /// WebView2 数据目录根（每个实例一个独立子目录，避免共享累积导致 431）
    pub webview: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self {
            versions: root.join("versions"),
            modpacks: root.join("modpacks"),
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
        std::fs::create_dir_all(&self.modpacks)?;
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(&self.store)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(&self.state)?;
        std::fs::create_dir_all(&self.webview)?;
        Ok(())
    }

    /// 实例所在的父目录。
    ///
    /// 默认：整合包（modpack- 前缀）→ modpacks/，其余 → versions/。
    /// **兼容旧位置**：2026-09-29 之前导入的整合包实例留在 versions/ 下，
    /// 若 modpacks/ 下不存在、而 versions/ 下存在同名目录，则用 versions/。
    /// 这样旧实例的启动/卸载/扫描等操作仍能正确定位。
    pub fn parent_of(&self, name: &str) -> PathBuf {
        if name.starts_with("modpack-") {
            if !self.modpacks.join(name).exists() && self.versions.join(name).exists() {
                return self.versions.clone();
            }
            self.modpacks.clone()
        } else {
            self.versions.clone()
        }
    }

    /// 实例目录的绝对路径（自动区分版本 / 整合包）
    pub fn instance_dir(&self, name: &str) -> PathBuf {
        self.parent_of(name).join(name)
    }
}
