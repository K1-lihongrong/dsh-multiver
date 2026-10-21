//! 配置操作命令（D6 拆分）：切换数据根 / 复用官方配置目录。
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub(crate) fn set_root(app: tauri::AppHandle, root: Option<String>) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);

    // 切换前：对**旧**数据根做一次残留进程清理（GAP-011）。
    // 否则旧 root 下遗留的登记不会被新 root 的启动清理触及，其中的 dsh 可能永久残留。
    #[cfg(unix)]
    {
        let old_root = cfg.resolve_root(&mdir);
        if old_root != mdir {
            // 仅在旧 root 与管理器目录不同、且确实存在时才尝试（避免误清管理器自身目录）
            let n = crate::procreg::cleanup_stale(&old_root);
            if n > 0 {
                logging::write_line(
                    &old_root.join("logs"),
                    "maintenance.log",
                    &format!("切换数据根前清理旧 root 残留 dsh：{} 个", n),
                );
            }
        }
    }

    cfg.root_dir = root.clone();
    cfg.save(&mdir).map_err(|e| e.to_string())?;
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    dirs.ensure().map_err(|e| e.to_string())?;
    // 根目录变了，重生成转发脚本（含新的 DSH_HOME）
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    Ok(format!("根目录已设为 {}", dirs.root.to_string_lossy()))
}

/// 设置「复用官方 dsh 配置目录（~/.dsh）」开关。
///
/// 开启后，非隔离版本的 DSH_HOME 指向官方默认目录，用户已有的会话/凭据/插件立即可用；
/// 隔离版本不受影响。切换后重生成转发脚本，保证 GUI 与终端行为一致。
#[tauri::command]
pub(crate) fn set_use_official_home(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    cfg.use_official_dsh_home = enabled;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败: {}", e))?;
    // 非隔离版本的 home 来源变了：重生成转发脚本
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    if enabled {
        let path = crate::paths::official_dsh_home()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "~/.dsh".to_string());
        Ok(format!("已复用官方配置目录：{}", path))
    } else {
        Ok("已恢复为管理器共享配置目录".to_string())
    }
}

/// 查询官方 dsh 配置目录路径（供前端展示/判断是否存在）。
#[tauri::command]
pub(crate) fn get_official_home() -> Option<String> {
    crate::paths::official_dsh_home().map(|p| p.to_string_lossy().to_string())
}
