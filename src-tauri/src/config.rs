use crate::domain::{HomeRef, InstanceId, VersionId};
use crate::ports::{Clock, FileSystem, SystemClock, SystemFs};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 当前配置结构版本。旧配置缺省字段时按 1 处理并自动迁移。
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

fn legacy_schema_version() -> u32 {
    1
}

/// 实例状态。Deleting 是崩溃一致性中间态：启动扫描到就把删除续完。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    #[default]
    Ready,
    Deleting,
}

/// 一个隔离实例（全局实体，不隶属于某个版本）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub name: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub state: InstanceState,
}

/// 管理器配置，持久化到 <根目录>/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 配置结构版本；缺失时按旧版 1 迁移。
    #[serde(default = "legacy_schema_version")]
    pub schema_version: u32,
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
    /// 是否复用官方 dsh 配置目录（~/.dsh / %USERPROFILE%\.dsh）。
    /// 默认 false：非隔离版本使用管理器共享 home（<根>/home），保持既有行为。
    /// true 时：非隔离版本的 DSH_HOME 指向官方默认目录，用户已有的会话/凭据/插件立即可用。
    /// 隔离版本不受此开关影响（永远使用 <根>/versions/<版本>/home）。
    #[serde(default)]
    pub use_official_dsh_home: bool,
    /// 用户是否已忽略「缺少运行时」的启动提示（v0.2.9）。
    /// 一旦忽略，不再自动弹横幅；用户仍可从环境检查面板手动准备运行时。
    #[serde(default)]
    pub runtime_prompt_dismissed: bool,

    /// 全局实例表。
    #[serde(default)]
    pub instances: Vec<Instance>,
    /// 版本 → 已挂载实例 id（顺序即展示顺序）。
    #[serde(default)]
    pub version_instances: BTreeMap<VersionId, Vec<InstanceId>>,
    /// 每版本主按钮目标；缺失或 None 表示共享 home。
    #[serde(default)]
    pub selected_target: BTreeMap<VersionId, Option<InstanceId>>,
    /// 实例 id 分配计数器（历史最大值，永不复用）。
    #[serde(default)]
    pub instance_seq: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            root_dir: None,
            default_version: None,
            isolated_versions: Vec::new(),
            broken_versions: Vec::new(),
            maintenance: MaintenanceConfig::default(),
            use_official_dsh_home: false,
            runtime_prompt_dismissed: false,
            instances: Vec::new(),
            version_instances: BTreeMap::new(),
            selected_target: BTreeMap::new(),
            instance_seq: 0,
        }
    }
}

/// 维护相关配置与状态。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

/// 校验用户输入的数据根路径（GUI 与 CLI 共用）。
pub fn validate_root_path(raw: &str) -> Result<std::path::PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("目录不能为空".into());
    }
    let path = std::path::PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(format!("请提供绝对路径：{}", raw));
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("路径不能包含 ..：{}", raw));
    }
    Ok(path)
}

impl Config {
    /// 配置文件名
    pub const FILE_NAME: &'static str = "config.json";

    /// 读取配置。config_path 为管理器自身的目录（exe 所在目录）
    ///
    /// **纯读取，无副作用。** 文件不存在或解析失败时静默回退到默认配置。
    /// 需要「损坏时留痕」的启动路径应调用 [`Config::load_checked`]。
    pub fn load(manager_dir: &Path) -> Self {
        let path = manager_dir.join(Self::FILE_NAME);
        if let Ok(text) = SystemFs.read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&text) {
                return cfg;
            }
        }
        Config::default()
    }

    /// 启动路径专用：读取配置，**文件损坏时写警告日志 + 重命名保留现场**。
    ///
    /// 与 [`Config::load`] 的区别仅在「解析失败」这一种情况：
    /// - 文件不存在 → 与 load 相同，返回默认配置（首次启动的正常路径，不告警）
    /// - 文件存在但解析失败 → 写 `<manager_dir>/logs/app.log` 警告，把原文件重命名为
    ///   `config.json.broken-<时间戳>` 保留现场，返回默认配置
    ///
    /// 设计依据见 `docs/交互设计约定.md` §2.5。技术债 D18。
    pub fn load_checked(manager_dir: &Path) -> Self {
        let path = manager_dir.join(Self::FILE_NAME);
        let text = match SystemFs.read_to_string(&path) {
            Ok(t) => t,
            // 文件不存在或读不了：首次启动的正常路径，不告警
            Err(_) => return Config::default(),
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(mut cfg) => {
                // 旧配置（缺 schema_version）在此完成一次性迁移。
                // 迁移失败时保留原文件并继续用旧配置运行，不覆盖用户现场。
                if cfg.schema_version < CURRENT_SCHEMA_VERSION {
                    let mut next = cfg.clone();
                    match next.migrate_v0_4_0(manager_dir) {
                        Ok(true) => {
                            if let Err(e) = next.save(manager_dir) {
                                crate::logging::write_line(
                                    &manager_dir.join("logs"),
                                    "app.log",
                                    &format!("配置迁移写盘失败，继续使用旧配置：{e}"),
                                );
                            } else {
                                cfg = next;
                            }
                        }
                        Ok(false) => cfg = next,
                        Err(e) => crate::logging::write_line(
                            &manager_dir.join("logs"),
                            "app.log",
                            &format!("配置迁移失败，原文件保持不变：{e}"),
                        ),
                    }
                }
                // 上次删除实例中断：启动时续完。失败只写日志，不阻塞管理器。
                let root = cfg.resolve_root(manager_dir);
                match cfg.resume_deleting_instances(&root) {
                    Ok(true) => {
                        if let Err(e) = cfg.save(manager_dir) {
                            crate::logging::write_line(
                                &manager_dir.join("logs"),
                                "app.log",
                                &format!("续完实例删除后写盘失败：{e}"),
                            );
                        }
                    }
                    Ok(false) => {}
                    Err(e) => crate::logging::write_line(
                        &manager_dir.join("logs"),
                        "app.log",
                        &format!("续完实例删除失败，下次启动重试：{e}"),
                    ),
                }
                cfg
            }
            Err(e) => {
                // 配置损坏：留痕 + 保留现场 + 回退默认
                let ts = crate::logging::stamp_compact(crate::logging::now_secs());
                let broken = manager_dir.join(format!("{}.broken-{}", Self::FILE_NAME, ts));
                let rename_ok = SystemFs.rename(&path, &broken).is_ok();
                let mut msg = format!("config.json 解析失败，已回退默认配置：{}", e);
                if rename_ok {
                    msg.push_str(&format!("；原文件已保留为 {}", broken.display()));
                } else {
                    msg.push_str("；但原文件重命名失败，未保留现场");
                }
                crate::logging::write_line(&manager_dir.join("logs"), "app.log", &msg);
                Config::default()
            }
        }
    }

    /// 保存配置到管理器目录
    pub fn save(&self, manager_dir: &Path) -> std::io::Result<()> {
        let path = manager_dir.join(Self::FILE_NAME);
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        SystemFs.write(&path, text.as_bytes())
    }

    /// 解析出实际的数据根目录
    ///
    /// 注意：用户可能误输入首尾空格（如 `D:\DSH `），若原样使用会让后续
    /// `root.join("home")` 拼出带空格的路径（如 `D:\DSH \home`），
    /// 在 NTFS 上生成"看似重复"的目录项。故这里统一 `trim()` 去首尾空格。
    pub fn resolve_root(&self, manager_dir: &Path) -> PathBuf {
        match &self.root_dir {
            Some(r) if !r.trim().is_empty() => PathBuf::from(r.trim()),
            _ => manager_dir.to_path_buf(),
        }
    }

    /// 计算某版本 + 可选实例对应的 home 类型。
    ///
    /// 规则：显式实例 > selected_target 的显式值 > 共享/官方默认。
    /// selected_target 缺失或为 null 都回共享 home，不做「回落到第一个挂载实例」
    /// 的隐式推断——主按钮开哪份数据必须由配置明确表达。
    pub fn home_ref_for(&self, version: &VersionId, instance: Option<&InstanceId>) -> HomeRef {
        if let Some(id) = instance {
            return HomeRef::Instance(id.clone());
        }
        if let Some(Some(id)) = self.selected_target.get(version) {
            return HomeRef::Instance(id.clone());
        }
        self.default_home_ref()
    }

    fn default_home_ref(&self) -> HomeRef {
        if self.use_official_dsh_home {
            HomeRef::Official
        } else {
            HomeRef::Shared
        }
    }

    /// 给版本创建一个默认隔离实例，并设为该版本主按钮目标。
    /// 保留 isolated_versions 镜像，供 alpha.1 仍在使用旧字段的 CLI/列表读取。
    pub fn ensure_default_instance(
        &mut self,
        version: &VersionId,
        root: &Path,
    ) -> Result<InstanceId, String> {
        if let Some(ids) = self.version_instances.get(version) {
            if let Some(first) = ids.first() {
                // 已有映射时只补「主按钮为共享」的情况；用户显式选过的实例不能被重置。
                let target = match self.selected_target.get(version) {
                    Some(Some(id)) => id.clone(),
                    _ => {
                        self.selected_target
                            .insert(version.clone(), Some(first.clone()));
                        first.clone()
                    }
                };
                if !self.isolated_versions.iter().any(|v| v == version.as_str()) {
                    self.isolated_versions.push(version.to_string());
                }
                return Ok(target);
            }
        }
        let id = self.alloc_instance_id()?;
        let name = self.unique_instance_name("默认隔离");
        let dirs = Dirs::new(root.to_path_buf());
        let _ = dirs.ensure();
        SystemFs
            .create_dir_all(&dirs.homes.join(id.as_str()))
            .map_err(|e| e.to_string())?;
        self.instances.push(Instance {
            id: id.clone(),
            name,
            created_at: SystemClock.now_secs(),
            state: InstanceState::Ready,
        });
        self.version_instances
            .insert(version.clone(), vec![id.clone()]);
        self.selected_target
            .insert(version.clone(), Some(id.clone()));
        if !self.isolated_versions.iter().any(|v| v == version.as_str()) {
            self.isolated_versions.push(version.to_string());
        }
        Ok(id)
    }

    /// 解除版本与全部实例的挂载关系；实例本体保留，供其他版本继续挂载。
    pub fn detach_version(&mut self, version: &VersionId) {
        self.version_instances.remove(version);
        self.selected_target.remove(version);
        self.isolated_versions.retain(|v| v != version.as_str());
    }

    fn alloc_instance_id(&mut self) -> Result<InstanceId, String> {
        loop {
            self.instance_seq = self
                .instance_seq
                .checked_add(1)
                .ok_or_else(|| "实例序号耗尽".to_string())?;
            let id = InstanceId::from_u64(self.instance_seq)?;
            if !self.instances.iter().any(|i| i.id == id) {
                return Ok(id);
            }
        }
    }

    fn unique_instance_name(&self, base: &str) -> String {
        let used: std::collections::HashSet<String> = self
            .instances
            .iter()
            .map(|i| i.name.to_lowercase())
            .collect();
        if !used.contains(&base.to_lowercase()) {
            return base.to_string();
        }
        for n in 2..=9999 {
            let candidate = format!("{base} {n}");
            if !used.contains(&candidate.to_lowercase()) {
                return candidate;
            }
        }
        format!("{base} {}", SystemClock.now_secs())
    }

    /// 续完上次中断的实例删除：清掉 homes/<id>，移除实例与所有引用。
    /// 失败时保留 state=deleting，下次启动继续；不阻塞主流程。
    pub fn resume_deleting_instances(&mut self, root: &Path) -> Result<bool, String> {
        let deleting: Vec<InstanceId> = self
            .instances
            .iter()
            .filter(|i| i.state == InstanceState::Deleting)
            .map(|i| i.id.clone())
            .collect();
        if deleting.is_empty() {
            return Ok(false);
        }
        let dirs = Dirs::new(root.to_path_buf());
        for id in &deleting {
            let home = dirs.homes.join(id.as_str());
            if !home.exists() {
                continue;
            }
            let _ = SystemFs.create_dir_all(&dirs.trash);
            let stamp = crate::logging::now_secs();
            let mut dest = dirs
                .trash
                .join(format!("instance-{}-{}", id.as_str(), stamp));
            let mut n = 1u32;
            while dest.exists() {
                dest = dirs
                    .trash
                    .join(format!("instance-{}-{}-{}", id.as_str(), stamp, n));
                n += 1;
            }
            // trash 中转：同一卷内改名是瞬时的，目录内容不搬；失败才退化为直接删。
            if SystemFs.rename(&home, &dest).is_err() {
                SystemFs
                    .remove_dir_all(&home)
                    .map_err(|e| format!("删除实例 home 失败（{}）：{e}", home.display()))?;
            }
        }
        self.instances.retain(|i| !deleting.contains(&i.id));
        for ids in self.version_instances.values_mut() {
            ids.retain(|id| !deleting.contains(id));
        }
        self.version_instances.retain(|_, ids| !ids.is_empty());
        self.selected_target.retain(|_, target| {
            target
                .as_ref()
                .map(|id| !deleting.contains(id))
                .unwrap_or(true)
        });
        Ok(true)
    }

    /// v0.4.0 配置迁移：isolated_versions → 实例表 + homes/iN。
    ///
    /// 幂等：schema_version 已是当前版本时直接返回 false。
    pub fn migrate_v0_4_0(&mut self, manager_dir: &Path) -> Result<bool, String> {
        if self.schema_version >= CURRENT_SCHEMA_VERSION {
            return Ok(false);
        }
        let root = self.resolve_root(manager_dir);
        let dirs = Dirs::new(root.clone());
        dirs.ensure()
            .map_err(|e| format!("创建数据目录失败：{e}"))?;

        // 先对齐已有实例的最大序号，避免手工改坏配置后产生重复 id。
        if let Some(max_id) = self.instances.iter().map(|i| i.id.value()).max() {
            if self.instance_seq < max_id {
                self.instance_seq = max_id;
            }
        }

        let legacy_versions = self.isolated_versions.clone();
        let mut migrated = 0usize;
        for raw in legacy_versions {
            let version = VersionId::new(&raw)?;
            if self
                .version_instances
                .get(&version)
                .map(|ids| !ids.is_empty())
                .unwrap_or(false)
            {
                continue;
            }
            let id = self.alloc_instance_id()?;
            let name = self.unique_instance_name("默认隔离");
            let src = dirs.versions.join(version.as_str()).join("home");
            let dst = dirs.homes.join(id.as_str());
            let dst_existed = dst.exists();
            if dst_existed && !dst.is_dir() {
                return Err(format!("迁移目标不是目录，拒绝使用：{}", dst.display()));
            }
            if dst_existed {
                // 典型场景：上次改名成功但 config.json 没保存。重新分配到同一个 id，
                // 目标目录里就是用户数据；复用并留痕，不阻塞升级，也不覆盖源目录。
                let extra = if src.exists() {
                    format!("；源目录仍保留在 {}，请人工确认", src.display())
                } else {
                    String::new()
                };
                crate::logging::write_line(
                    &root.join("logs"),
                    "app.log",
                    &format!(
                        "迁移 {}：复用已存在的实例目录 {}{}",
                        version.as_str(),
                        dst.display(),
                        extra
                    ),
                );
            } else if src.exists() {
                if SystemFs.rename(&src, &dst).is_err() {
                    let (ok, msg, _) =
                        crate::versions::isolated::copy_shared_to_isolated(&src, &dst);
                    if !ok {
                        return Err(format!("迁移 home 失败：{msg}"));
                    }
                    let _ = SystemFs.remove_dir_all(&src);
                }
            } else {
                SystemFs.create_dir_all(&dst).map_err(|e| e.to_string())?;
            }
            self.instances.push(Instance {
                id: id.clone(),
                name,
                created_at: crate::logging::now_secs(),
                state: InstanceState::Ready,
            });
            self.version_instances
                .insert(version.clone(), vec![id.clone()]);
            self.selected_target.insert(version, Some(id));
            migrated += 1;
        }
        self.schema_version = CURRENT_SCHEMA_VERSION;
        crate::logging::write_line(
            &root.join("logs"),
            "app.log",
            &format!("配置已迁移到 v{CURRENT_SCHEMA_VERSION}：生成 {migrated} 个实例"),
        );
        Ok(true)
    }

    /// 根据一次安装结果维护 `broken_versions`：
    /// - 成功：移除该版本标记
    /// - 失败且分类为「私有包/不完整」：加入标记
    ///
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

    /// 卸载某版本时清理配置中对它的引用：默认版本、隔离镜像、挂载与主按钮目标。
    ///
    /// 实例本体保留为孤儿：它可能仍被其他版本挂载，也可能只剩这一份数据；
    /// 是否删除由用户在全局面板决定，卸载版本不代替用户做数据删除。
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
        if let Ok(vid) = VersionId::new(version) {
            if self.version_instances.remove(&vid).is_some() {
                changed = true;
            }
            if self.selected_target.remove(&vid).is_some() {
                changed = true;
            }
        }
        changed
    }
}

/// 由根目录派生出的各个子目录
#[derive(Debug, Clone)]
pub struct Dirs {
    pub root: PathBuf,
    pub versions: PathBuf,
    pub home: PathBuf,
    /// 实例 home 根：<root>/homes/<id>
    pub homes: PathBuf,
    pub store: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    /// WebView2 数据目录根（每个版本一个独立子目录，避免共享累积导致 431）
    pub webview: PathBuf,
    /// 「回收站」目录：卸载时先把版本目录 rename 到这里（瞬时），再由后台慢慢删。
    /// 避免 Windows 上直接删 26000 个文件导致的长时间无响应等待。
    pub trash: PathBuf,
    /// 管理器资源目录（如桌面快捷方式用的 dsh 图标）；仅 Windows 使用。
    pub assets: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self {
            versions: root.join("versions"),
            home: root.join("home"),
            homes: root.join("homes"),
            store: root.join("store"),
            cache: root.join("cache"),
            state: root.join("state"),
            webview: root.join("webview"),
            trash: root.join("trash"),
            assets: root.join("assets"),
            root,
        }
    }

    /// 确保各子目录存在
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.versions)?;
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(&self.homes)?;
        std::fs::create_dir_all(&self.store)?;
        std::fs::create_dir_all(&self.cache)?;
        std::fs::create_dir_all(&self.state)?;
        std::fs::create_dir_all(&self.webview)?;
        std::fs::create_dir_all(&self.trash)?;
        Ok(())
    }

    /// 按名称解析子目录（供「打开目录」用）。未知名称回退到 root。
    pub fn resolve(&self, which: &str) -> PathBuf {
        match which {
            "versions" => self.versions.clone(),
            "home" => self.home.clone(),
            "homes" => self.homes.clone(),
            "store" => self.store.clone(),
            "cache" => self.cache.clone(),
            "state" => self.state.clone(),
            "logs" => self.root.join("logs"),
            "webview" => self.webview.clone(),
            "trash" => self.trash.clone(),
            "assets" => self.assets.clone(),
            "root" => self.root.clone(),
            _ => self.root.clone(),
        }
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
        let mut cfg = Config {
            root_dir: Some("   ".to_string()),
            ..Default::default()
        };
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
        assert_eq!(
            cfg.resolve_root(&PathBuf::from("C:\\m")),
            PathBuf::from("D:\\data")
        );
    }

    #[test]
    fn root_with_surrounding_spaces_is_trimmed() {
        // 用户误输入首尾空格时，应去掉，避免拼出带空格的子目录路径（如 "D:\DSH \home"）
        let cfg = Config {
            root_dir: Some("  D:\\DSH  ".to_string()),
            ..Config::default()
        };
        assert_eq!(
            cfg.resolve_root(&PathBuf::from("C:\\m")),
            PathBuf::from("D:\\DSH")
        );
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
            use_official_dsh_home: true,
            runtime_prompt_dismissed: false,
            ..Config::default()
        };
        cfg.save(&dir).unwrap();
        let back = Config::load(&dir);
        assert_eq!(back.default_version.as_deref(), Some("0.1.7"));
        assert_eq!(back.isolated_versions, vec!["0.1.7".to_string()]);
        assert_eq!(back.broken_versions, vec!["0.0.1".to_string()]);
        assert!(!back.maintenance.auto_enabled);
        assert_eq!(back.maintenance.last_prune_at, Some(123));
        assert_eq!(back.maintenance.last_cleanup_count, 3);
        assert!(back.use_official_dsh_home);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_root_path_rules() {
        assert!(validate_root_path("  ").is_err());
        assert!(validate_root_path("relative/path").is_err());
        #[cfg(windows)]
        assert!(validate_root_path("C:\\a\\..\\b").is_err());
        #[cfg(not(windows))]
        assert!(validate_root_path("/tmp/../etc").is_err());
        #[cfg(windows)]
        assert!(validate_root_path("C:\\data").is_ok());
        #[cfg(not(windows))]
        assert!(validate_root_path("/tmp/data").is_ok());
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
    fn load_checked_missing_file_returns_default_without_side_effects() {
        // 首次启动：文件不存在 → 返回默认配置，不写日志、不重命名
        let dir = tmp("checked-missing");
        let cfg = Config::load_checked(&dir);
        assert!(cfg.root_dir.is_none());
        // 不应产生 logs/ 或 broken 文件
        assert!(!dir.join("logs").exists());
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_checked_valid_file_returns_config_without_side_effects() {
        let dir = tmp("checked-valid");
        let c = Config {
            default_version: Some("1.2.3".to_string()),
            ..Config::default()
        };
        c.save(&dir).unwrap();
        let cfg = Config::load_checked(&dir);
        assert_eq!(cfg.default_version.as_deref(), Some("1.2.3"));
        // 原文件仍在，无 broken 文件、无 logs
        assert!(dir.join(Config::FILE_NAME).exists());
        assert!(!dir.join("logs").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_checked_corrupt_file_renames_and_logs() {
        // D18：损坏时重命名保留现场 + 写 app.log 警告
        let dir = tmp("checked-corrupt");
        let cfg_path = dir.join(Config::FILE_NAME);
        std::fs::write(&cfg_path, "{ not valid json").unwrap();

        let cfg = Config::load_checked(&dir);
        assert!(cfg.default_version.is_none());

        // 原文件已被重命名（不存在了）
        assert!(!cfg_path.exists());
        // 产生了 config.json.broken-<ts> 文件
        let broken: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.broken-")
            })
            .collect();
        assert_eq!(broken.len(), 1, "应有一个 broken 文件");

        // app.log 里有警告，且含「解析失败」
        let log = std::fs::read_to_string(dir.join("logs").join("app.log")).unwrap();
        assert!(log.contains("解析失败"));
        assert!(log.contains("已回退默认配置"));

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
        let mut c = Config {
            broken_versions: vec!["0.1.0".to_string(), "0.2.0".to_string()],
            ..Default::default()
        };
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
        let mut c = Config {
            broken_versions: vec!["0.1.0".to_string()],
            ..Default::default()
        };
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

    // ── Dirs::resolve：目录名 → 路径 ──

    #[test]
    fn dirs_resolve_known_names() {
        let d = Dirs::new(PathBuf::from("R"));
        assert_eq!(d.resolve("versions"), PathBuf::from("R").join("versions"));
        assert_eq!(d.resolve("home"), PathBuf::from("R").join("home"));
        assert_eq!(d.resolve("store"), PathBuf::from("R").join("store"));
        assert_eq!(d.resolve("cache"), PathBuf::from("R").join("cache"));
        assert_eq!(d.resolve("state"), PathBuf::from("R").join("state"));
        assert_eq!(d.resolve("logs"), PathBuf::from("R").join("logs"));
        assert_eq!(d.resolve("root"), PathBuf::from("R"));
    }

    #[test]
    fn dirs_resolve_unknown_falls_back_to_root() {
        let d = Dirs::new(PathBuf::from("R"));
        assert_eq!(d.resolve("nope"), PathBuf::from("R"));
    }

    fn write_old_config(manager: &Path, root: &Path, isolated: &[&str]) {
        let old = serde_json::json!({
            "root_dir": root.to_string_lossy(),
            "default_version": null,
            "isolated_versions": isolated,
            "broken_versions": [],
            "maintenance": { "auto_enabled": false },
            "use_official_dsh_home": false,
            "runtime_prompt_dismissed": false
        });
        std::fs::write(
            manager.join(Config::FILE_NAME),
            serde_json::to_string_pretty(&old).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn migrates_single_isolated_version_to_instance_home() {
        let manager = tmp("migrate-one");
        let root = manager.join("root");
        let old_home = root.join("versions").join("0.1.0").join("home");
        std::fs::create_dir_all(&old_home).unwrap();
        std::fs::write(old_home.join("marker.txt"), b"legacy").unwrap();
        write_old_config(&manager, &root, &["0.1.0"]);

        let cfg = Config::load_checked(&manager);
        assert_eq!(cfg.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(cfg.instances.len(), 1);
        assert_eq!(cfg.instances[0].id.to_string(), "i1");
        assert_eq!(
            cfg.version_instances
                .get(&VersionId::new("0.1.0").unwrap())
                .unwrap(),
            &vec![InstanceId::new("i1").unwrap()]
        );
        assert_eq!(
            cfg.selected_target
                .get(&VersionId::new("0.1.0").unwrap())
                .unwrap(),
            &Some(InstanceId::new("i1").unwrap())
        );
        let new_home = root.join("homes").join("i1");
        assert_eq!(
            std::fs::read(new_home.join("marker.txt")).unwrap(),
            b"legacy"
        );
        assert!(!old_home.exists(), "旧 home 应已搬走");
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn migrates_multiple_versions_with_unique_names_and_ids() {
        let manager = tmp("migrate-many");
        let root = manager.join("root");
        for v in ["0.1.0", "0.2.0"] {
            let old_home = root.join("versions").join(v).join("home");
            std::fs::create_dir_all(&old_home).unwrap();
            std::fs::write(old_home.join("v.txt"), v).unwrap();
        }
        write_old_config(&manager, &root, &["0.1.0", "0.2.0"]);

        let cfg = Config::load_checked(&manager);
        assert_eq!(cfg.instances.len(), 2);
        assert_eq!(cfg.instance_seq, 2);
        assert_ne!(cfg.instances[0].name, cfg.instances[1].name);
        for (version, id) in [("0.1.0", "i1"), ("0.2.0", "i2")] {
            let vid = VersionId::new(version).unwrap();
            assert_eq!(
                cfg.version_instances.get(&vid).unwrap(),
                &vec![InstanceId::new(id).unwrap()]
            );
            assert_eq!(
                std::fs::read(root.join("homes").join(id).join("v.txt")).unwrap(),
                version.as_bytes()
            );
        }
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn migration_is_idempotent_and_persisted() {
        let manager = tmp("migrate-idem");
        let root = manager.join("root");
        std::fs::create_dir_all(root.join("versions").join("0.1.0").join("home")).unwrap();
        write_old_config(&manager, &root, &["0.1.0"]);

        let first = Config::load_checked(&manager);
        let second = Config::load_checked(&manager);
        assert_eq!(first.instances.len(), 1);
        assert_eq!(second.instances.len(), 1);
        assert_eq!(second.instance_seq, 1);
        let reloaded = Config::load(&manager);
        assert_eq!(reloaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(reloaded.instances.len(), 1);
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn migration_without_isolated_versions_is_noop() {
        let manager = tmp("migrate-none");
        let root = manager.join("root");
        write_old_config(&manager, &root, &[]);
        let cfg = Config::load_checked(&manager);
        assert!(cfg.instances.is_empty());
        assert_eq!(cfg.schema_version, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn ensure_default_instance_keeps_explicit_target() {
        let manager = tmp("ensure-target");
        let root = manager.join("root");
        let vid = VersionId::new("0.1.0").unwrap();
        let mut cfg = Config::default();
        let first = cfg.ensure_default_instance(&vid, &root).unwrap();
        let second = InstanceId::new("i2").unwrap();
        cfg.instances.push(Instance {
            id: second.clone(),
            name: "对比用".into(),
            created_at: 0,
            state: InstanceState::Ready,
        });
        cfg.version_instances
            .get_mut(&vid)
            .unwrap()
            .push(second.clone());
        cfg.selected_target
            .insert(vid.clone(), Some(second.clone()));

        let target = cfg.ensure_default_instance(&vid, &root).unwrap();
        assert_eq!(target, second, "已有显式选择时应返回它，而不是首个实例");
        assert_eq!(cfg.selected_target.get(&vid), Some(&Some(second)));
        assert_ne!(target, first);
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn migration_reuses_existing_instance_home() {
        let manager = tmp("migrate-reuse");
        let root = manager.join("root");
        let dst = root.join("homes").join("i1");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("marker.txt"), b"kept").unwrap();
        write_old_config(&manager, &root, &["0.1.0"]);

        let cfg = Config::load_checked(&manager);
        assert_eq!(cfg.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(cfg.instances.len(), 1);
        assert_eq!(cfg.instances[0].id.to_string(), "i1");
        assert_eq!(
            std::fs::read(dst.join("marker.txt")).unwrap(),
            b"kept",
            "复用的实例目录内容不能被清掉"
        );
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn migration_keeps_source_when_destination_exists() {
        let manager = tmp("migrate-reuse-both");
        let root = manager.join("root");
        let src = root.join("versions").join("0.1.0").join("home");
        let dst = root.join("homes").join("i1");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("src.txt"), b"src").unwrap();
        std::fs::write(dst.join("dst.txt"), b"dst").unwrap();
        write_old_config(&manager, &root, &["0.1.0"]);

        let cfg = Config::load_checked(&manager);
        assert_eq!(cfg.instances.len(), 1);
        assert_eq!(
            std::fs::read(dst.join("dst.txt")).unwrap(),
            b"dst",
            "目标目录应被复用"
        );
        assert!(
            src.exists(),
            "源目录与目标同时存在时不删源，留给用户人工确认"
        );
        let _ = std::fs::remove_dir_all(&manager);
    }

    #[test]
    fn uninstall_keeps_orphan_instance() {
        let mut cfg = Config::default();
        let shared_id = InstanceId::new("i1").unwrap();
        cfg.instances.push(Instance {
            id: shared_id.clone(),
            name: "共享实例".into(),
            created_at: 0,
            state: InstanceState::Ready,
        });
        for v in ["0.1.0", "0.2.0"] {
            cfg.version_instances
                .insert(VersionId::new(v).unwrap(), vec![shared_id.clone()]);
        }
        assert!(cfg.remove_version_refs("0.1.0"));
        assert_eq!(cfg.instances.len(), 1, "仍被 0.2.0 挂载，必须保留");
        assert!(cfg.remove_version_refs("0.2.0"));
        assert_eq!(cfg.instances.len(), 1, "孤儿实例保留，由用户决定是否删除");
        assert_eq!(cfg.instances[0].id, shared_id);
        assert!(cfg.version_instances.is_empty());
        assert!(cfg.selected_target.is_empty());
    }

    #[test]
    fn resume_deleting_instance_cleans_home_and_refs() {
        let manager = tmp("resume-delete");
        let root = manager.join("root");
        let id = InstanceId::new("i7").unwrap();
        let home = Dirs::new(root.clone()).homes.join("i7");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("x"), b"x").unwrap();
        let mut cfg = Config {
            root_dir: Some(root.to_string_lossy().to_string()),
            ..Config::default()
        };
        cfg.instances.push(Instance {
            id: id.clone(),
            name: "待删除".into(),
            created_at: 0,
            state: InstanceState::Deleting,
        });
        cfg.version_instances
            .insert(VersionId::new("0.1.0").unwrap(), vec![id.clone()]);
        cfg.selected_target
            .insert(VersionId::new("0.1.0").unwrap(), Some(id.clone()));
        cfg.instance_seq = 7;
        cfg.save(&manager).unwrap();

        let loaded = Config::load_checked(&manager);
        assert!(loaded.instances.is_empty());
        assert!(loaded.version_instances.is_empty());
        assert!(loaded.selected_target.is_empty());
        assert!(!home.exists(), "原 home 必须已移走");
        assert_eq!(loaded.instance_seq, 7, "序号仍不复用");

        // 删除走 trash 中转：目录应被改名到 trash/instance-i7-*，内容保留。
        let trash = Dirs::new(root.clone()).trash;
        let entries: Vec<String> = std::fs::read_dir(&trash)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        let moved = entries
            .iter()
            .find(|n| n.starts_with("instance-i7-"))
            .unwrap_or_else(|| panic!("trash 中应存在实例目录，实际 {entries:?}"));
        assert_eq!(
            std::fs::read(trash.join(moved).join("x")).unwrap(),
            b"x",
            "改名不应丢内容"
        );
        let _ = std::fs::remove_dir_all(&manager);
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
