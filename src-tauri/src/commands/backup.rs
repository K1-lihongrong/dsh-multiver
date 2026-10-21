//! 配置导入/导出命令（D6 拆分）。
#![allow(unused_imports)]
use super::*;

/// 导入预览：读备份文件，返回将改动的字段（不应用）。
#[tauri::command]
pub(crate) fn preview_import(
    app: tauri::AppHandle,
    file: String,
) -> Result<crate::backup::ImportPreview, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    crate::backup::preview_import(std::path::Path::new(&file), &cfg, &dirs.versions)
}

/// 应用导入：把备份文件的配置字段写入当前配置。
#[tauri::command]
pub(crate) fn apply_import(app: tauri::AppHandle, file: String) -> Result<Vec<String>, String> {
    let mdir = manager_dir(&app);
    let mut cfg = Config::load(&mdir);
    let applied = crate::backup::apply_import(std::path::Path::new(&file), &mut cfg)?;
    cfg.save(&mdir)
        .map_err(|e| format!("保存配置失败：{}", e))?;
    // 配置变了（默认版本/根目录/隔离可能变）→ 重生成转发脚本
    crate::paths::regenerate_forward_script(&mdir, &cfg);
    Ok(applied)
}

/// 导出配置 + 版本清单到指定文件。返回写入路径。
#[tauri::command]
pub(crate) fn export_backup(app: tauri::AppHandle, file: String) -> Result<String, String> {
    let mdir = manager_dir(&app);
    let cfg = Config::load(&mdir);
    let dirs = Dirs::new(cfg.resolve_root(&mdir));
    let out = std::path::PathBuf::from(file);
    crate::backup::export_to_file(&cfg, &dirs.versions, &out)
}
