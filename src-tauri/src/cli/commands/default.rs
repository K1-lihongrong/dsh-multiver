//! set-default / set-root 命令。

use crate::cli::load_ctx;
use crate::cli::output::out_result;
use crate::cli::parse::has_force;
use crate::config::Dirs;
use crate::versions;

/// `set-default <版本>`
pub fn cmd_set_default(version: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!(
                "{}",
                crate::cli::output::json_str(&serde_json::json!({
                    "dry_run": true, "action": "set-default", "version": version,
                    "installed": versions::exists(&ctx.dirs.versions, version),
                }))
            );
            return 0;
        }
        let installed = versions::exists(&ctx.dirs.versions, version);
        println!("[dry-run] 将把默认版本设为 {}", version);
        if installed {
            println!("[dry-run] 该版本已安装");
        } else {
            println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
        }
        println!("[dry-run] 会重生成终端转发脚本 dsh");
        println!("[dry-run] 未执行任何操作");
        // dry-run 下不因未安装而失败（只是预览）
        return 0;
    }
    if !versions::exists(&ctx.dirs.versions, version) {
        out_result(json, false, &format!("版本 {} 未安装", version));
        return 1;
    }
    // GAP-014：目标位置已有"非本管理器"的 dsh 命令 → 拒绝（除非显式 --force）
    if !has_force() {
        let target = crate::paths::forward_script_dir(&ctx.mdir);
        if let Some(p) = crate::actions::existing_foreign_dsh(&target) {
            out_result(json, false, &format!(
                "目标位置已存在非本管理器的 dsh 命令：{}\n继续将覆盖它（可能是 npm i -g 装的官方 dsh）。如确认，加 --force 重试。",
                p.to_string_lossy()
            ));
            return 1;
        }
    }
    ctx.cfg.default_version = Some(version.to_string());
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        out_result(json, false, &format!("保存配置失败：{}", e));
        return 1;
    }
    crate::paths::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    out_result(
        json,
        true,
        &format!("默认版本已设为 {}，dsh 命令已就绪", version),
    );
    0
}

/// `set-root <目录>`
///
/// 与 GUI 的 `commands::set_root` 等价：切换数据根、创建子目录、重生成终端转发脚本。
/// Unix 上额外对**旧**数据根做一次残留进程清理（GAP-011）——否则旧 root 下遗留的登记
/// 不会被新 root 的启动清理触及，其中的 dsh 可能永久残留。
/// Windows 不需要：子进程由 Job Object 保障（OS 级），见 procreg.rs 模块注释。
pub fn cmd_set_root(dir: &str, dry_run: bool, json: bool) -> i32 {
    let raw = dir.trim();
    if raw.is_empty() {
        out_result(json, false, "目录不能为空");
        return 1;
    }
    let path = std::path::Path::new(raw);
    if !path.is_absolute() {
        out_result(json, false, &format!("请提供绝对路径：{}", raw));
        return 1;
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        out_result(json, false, &format!("路径不能包含 ..：{}", raw));
        return 1;
    }

    let mut ctx = load_ctx();
    let old_root = ctx.dirs.root.clone();

    if dry_run {
        let created = !path.exists();
        if json {
            println!(
                "{}",
                crate::cli::output::json_str(&serde_json::json!({
                    "dry_run": true,
                    "action": "set-root",
                    "root": path.to_string_lossy(),
                    "current": old_root.to_string_lossy(),
                    "created": created,
                }))
            );
            return 0;
        }
        println!("[dry-run] 将把数据根设为 {}", path.display());
        println!("[dry-run] 当前数据根 {}", old_root.display());
        if created {
            println!("[dry-run] 该目录不存在，实际执行会创建");
        }
        println!("[dry-run] 会创建 versions/home/store/cache/state/webview/trash");
        println!("[dry-run] 会重生成终端转发脚本 dsh");
        #[cfg(unix)]
        if old_root != path {
            println!("[dry-run] 会清理旧数据根的残留 dsh 进程登记");
        }
        println!("[dry-run] 未执行任何操作");
        return 0;
    }

    // 创建目录并验证可写。先记下根目录是否原本不存在——供 --json 的 created 字段。
    let dir_created = !path.exists();
    let dirs = Dirs::new(path.to_path_buf());
    if let Err(e) = dirs.ensure() {
        out_result(
            json,
            false,
            &format!("无法创建目录 {}：{}", path.display(), e),
        );
        return 1;
    }
    let probe = dirs.root.join(".dsh-multiver-write-test");
    if let Err(e) = std::fs::write(&probe, b"ok") {
        out_result(
            json,
            false,
            &format!("目录不可写 {}：{}", path.display(), e),
        );
        return 1;
    }
    let _ = std::fs::remove_file(&probe);

    // Unix：切换前对旧数据根做一次残留进程清理（GAP-011）
    #[cfg(unix)]
    {
        if old_root != dirs.root {
            let n = crate::procreg::cleanup_stale(&old_root);
            if n > 0 {
                crate::logging::write_line(
                    &old_root.join("logs"),
                    "maintenance.log",
                    &format!("切换数据根前清理旧 root 残留 dsh：{} 个", n),
                );
            }
        }
    }

    let changed = old_root != dirs.root;
    ctx.cfg.root_dir = Some(dirs.root.to_string_lossy().to_string());
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        out_result(json, false, &format!("保存配置失败：{}", e));
        return 1;
    }
    crate::paths::regenerate_forward_script(&ctx.mdir, &ctx.cfg);

    if json {
        println!(
            "{}",
            crate::cli::output::json_str(&serde_json::json!({
                "ok": true,
                "root": dirs.root.to_string_lossy(),
                "previous": old_root.to_string_lossy(),
                "changed": changed,
                "created": dir_created,
            }))
        );
    } else {
        println!("数据根已设为 {}", dirs.root.display());
        if dir_created {
            println!("（该目录为新建）");
        }
        if changed {
            println!("原数据根 {}", old_root.display());
        }
        println!("终端转发脚本 dsh 已重生成");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 路径合法性校验：这些分支在任何平台都应拒绝，且不触碰配置。
    #[test]
    fn set_root_rejects_empty() {
        assert_eq!(cmd_set_root("   ", true, false), 1);
    }

    #[test]
    fn set_root_rejects_relative() {
        assert_eq!(cmd_set_root("relative/path", true, false), 1);
        assert_eq!(cmd_set_root("./here", true, false), 1);
    }

    #[test]
    fn set_root_rejects_parent_dir() {
        assert_eq!(cmd_set_root("/tmp/../etc", true, false), 1);
        #[cfg(windows)]
        assert_eq!(cmd_set_root("C:\\a\\..\\b", true, false), 1);
    }

    /// dry-run 对合法绝对路径应返回 0，且不写配置。
    #[test]
    fn set_root_dry_run_succeeds_without_side_effects() {
        let tmp = std::env::temp_dir().join("dsh-multiver-setroot-dryrun");
        let s = tmp.to_string_lossy().to_string();
        assert_eq!(cmd_set_root(&s, true, true), 0);
    }
}
