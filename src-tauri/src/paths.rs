//! 路径解析与终端转发脚本（D6 拆分：原 commands.rs 的路径类纯函数）。
//!
//! 这些函数被命令层、CLI 层共同使用，与 Tauri 无关，故独立成模块：
//! - `path_bin_dir` / `forward_script_dir`：转发脚本写到哪个目录
//! - `regenerate_forward_script`：根据默认版本生成或删除转发脚本
//! - `official_dsh_home`：官方 `~/.dsh` 目录
//! - `resolve_home`：某版本实际使用的 DSH_HOME（隔离 / 官方 / 共享）

use crate::actions;
use crate::config::{Config, Dirs};
use std::path::{Path, PathBuf};

/// 根据当前配置，重生成（或删除）终端 dsh 转发脚本。
///
/// - 有默认版本：写入转发脚本（Windows 为 dsh.cmd，Unix 为 dsh；含 DSH_HOME）
/// - 无默认版本：删除转发脚本
///
/// 供 set_default / set_root / set_isolated / uninstall_version 统一调用。
pub(crate) fn regenerate_forward_script(mdir: &Path, cfg: &Config) {
    let target = path_bin_dir(mdir);
    match &cfg.default_version {
        Some(v) => {
            let root = cfg.resolve_root(mdir);
            let dirs = Dirs::new(root.clone());
            // DSH_HOME 由 resolve_home 统一决定（隔离 / 复用官方 / 管理器共享），
            // 与 GUI 启动路径使用同一逻辑，保证"GUI 能用的配置，终端也能用"。
            let home = resolve_home(cfg, &dirs, v);
            let script =
                actions::build_forward_script(v, &root.to_string_lossy(), &home.to_string_lossy());
            let _ = actions::write_forward_script(&target, &script);
        }
        None => {
            // GAP-014：只删"本管理器生成的"转发脚本，避免误删官方 npm i -g 装的 dsh。
            #[cfg(windows)]
            let p = target.join("dsh.cmd");
            #[cfg(unix)]
            let p = target.join("dsh");
            if let Ok(content) = std::fs::read_to_string(&p) {
                if actions::is_forward_script(&content) {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
}

/// 决定转发脚本写到哪个目录。
///
/// - Windows：优先 `%APPDATA%\npm`（npm 全局 bin），否则管理器目录
/// - Unix：优先 `~/.local/bin`（XDG，多数发行版已在 PATH），其次 `~/.local/share/pnpm`、
///   `~/.npm-global/bin`；都不存在则创建 `~/.local/bin`。最后回退管理器目录。
///
/// 转发脚本所在目录（供 CLI 检测冲突复用）。
pub(crate) fn forward_script_dir(manager: &Path) -> PathBuf {
    path_bin_dir(manager)
}

#[cfg(windows)]
fn path_bin_dir(manager: &Path) -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let npm_bin = PathBuf::from(appdata).join("npm");
        if npm_bin.exists() {
            return npm_bin;
        }
    }
    manager.to_path_buf()
}

#[cfg(unix)]
fn path_bin_dir(manager: &Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        let candidates = [
            format!("{}/.local/bin", home),
            format!("{}/.local/share/pnpm", home),
            format!("{}/.npm-global/bin", home),
        ];
        for c in &candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return p;
            }
        }
        // 都不存在：创建 XDG 标准的 ~/.local/bin
        let fallback = PathBuf::from(&candidates[0]);
        if std::fs::create_dir_all(&fallback).is_ok() {
            return fallback;
        }
    }
    manager.to_path_buf()
}

#[cfg(not(any(windows, unix)))]
fn path_bin_dir(manager: &Path) -> PathBuf {
    manager.to_path_buf()
}

/// 官方 dsh 默认配置目录（~/.dsh 或 %USERPROFILE% 下的 .dsh）。
///
/// 返回 None 表示无法确定用户主目录（极少见）；调用方应回退到共享 home。
pub(crate) fn official_dsh_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .filter(|p| !p.trim().is_empty())
            .map(|p| PathBuf::from(p).join(".dsh"))
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .ok()
            .filter(|p| !p.trim().is_empty())
            .map(|p| PathBuf::from(p).join(".dsh"))
    }
}

/// 计算某版本实际使用的 DSH_HOME。
///
/// 优先级（高到低）：
/// 1. 隔离版本 → <根>/versions/<版本>/home（永远优先，不受全局开关影响）
/// 2. 复用官方 → ~/.dsh（use_official_dsh_home = true 时）
/// 3. 管理器共享 → <根>/home（默认）
pub(crate) fn resolve_home(cfg: &Config, dirs: &Dirs, version: &str) -> PathBuf {
    if cfg.isolated_versions.iter().any(|v| v == version) {
        let isolated_home = dirs.versions.join(version).join("home");
        let _ = std::fs::create_dir_all(&isolated_home);
        return isolated_home;
    }
    if cfg.use_official_dsh_home {
        if let Some(oh) = official_dsh_home() {
            let _ = std::fs::create_dir_all(&oh);
            return oh;
        }
    }
    dirs.home.clone()
}
