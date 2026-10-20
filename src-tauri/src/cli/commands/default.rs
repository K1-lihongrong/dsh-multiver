//! set-default 命令。

use crate::cli::load_ctx;
use crate::cli::output::out_result;
use crate::cli::parse::has_force;
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
        let target = crate::commands::forward_script_dir(&ctx.mdir);
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
    crate::commands::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    out_result(
        json,
        true,
        &format!("默认版本已设为 {}，dsh 命令已就绪", version),
    );
    0
}
