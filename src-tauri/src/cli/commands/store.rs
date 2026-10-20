//! 磁盘维护与诊断：maintenance / clean / doctor。

use crate::cli::load_ctx;
use crate::cli::output::json_str;
use crate::{maintenance, versions};

/// `maintenance [--cleanup|--prune]`
pub fn cmd_maintenance(kind: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "maintenance", "kind": kind,
                }))
            );
            return 0;
        }
        if kind == "cleanup" || kind == "all" {
            println!(
                "[dry-run] 将清理孤立 webview 缓存（{}）",
                ctx.dirs.webview.to_string_lossy()
            );
        }
        if kind == "prune" || kind == "all" {
            println!("[dry-run] 将回收依赖仓库（pnpm store prune）");
        }
        println!("[dry-run] 未执行任何操作");
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        eprintln!("创建数据目录失败：{}", e);
        return 1;
    }

    let mut cleaned: Option<usize> = None;
    let mut pruned = false;
    if kind == "cleanup" || kind == "all" {
        let removed = maintenance::cleanup_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
        ctx.cfg.maintenance.last_cleanup_at = Some(maintenance::now_secs());
        ctx.cfg.maintenance.last_cleanup_count = removed.len() as u64;
        cleaned = Some(removed.len());
        if !json {
            println!("已清理孤立缓存 {} 项", removed.len());
        }
    }

    if kind == "prune" || kind == "all" {
        match maintenance::run_store_prune(&ctx.dirs.store, &ctx.dirs.cache, &ctx.dirs.state) {
            Ok(()) => {
                ctx.cfg.maintenance.last_prune_at = Some(maintenance::now_secs());
                pruned = true;
                if !json {
                    println!("已回收依赖仓库");
                }
            }
            Err(e) => {
                let _ = ctx.cfg.save(&ctx.mdir);
                if json {
                    println!(
                        "{}",
                        json_str(
                            &serde_json::json!({ "ok": false, "error": format!("回收依赖仓库失败：{}", e) })
                        )
                    );
                } else {
                    eprintln!("回收依赖仓库失败：{}", e);
                }
                return 1;
            }
        }
    }

    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        if json {
            println!(
                "{}",
                json_str(
                    &serde_json::json!({ "ok": false, "error": format!("保存配置失败：{}", e) })
                )
            );
        } else {
            eprintln!("保存配置失败：{}", e);
        }
        return 1;
    }
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "ok": true, "kind": kind, "cleaned": cleaned, "pruned": pruned,
            }))
        );
    }
    0
}

/// `doctor`：一键诊断。聚合检查项，每项 ✓/✗ + 说明；有致命项失败 → 退出码 1。
pub fn cmd_doctor(json: bool) -> i32 {
    let ctx = load_ctx();
    let mut items: Vec<serde_json::Value> = Vec::new();
    let mut fatal = false;

    // 1) 数据根目录存在 / 可写
    let root = &ctx.dirs.root;
    let root_exists = root.exists();
    let root_writable = std::fs::metadata(root)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false);
    let root_ok = root_exists && root_writable;
    if !root_ok {
        fatal = true;
    }
    items.push(serde_json::json!({
        "name": "数据根目录", "ok": root_ok, "critical": true,
        "detail": if !root_exists { format!("不存在：{}", root.to_string_lossy()) }
                  else if !root_writable { format!("不可写：{}", root.to_string_lossy()) }
                  else { root.to_string_lossy().to_string() },
    }));

    // 2) 各子目录存在
    for (label, path) in [
        ("versions", &ctx.dirs.versions),
        ("store", &ctx.dirs.store),
        ("home", &ctx.dirs.home),
    ] {
        let ok = path.exists();
        items.push(serde_json::json!({
            "name": format!("目录 {}", label), "ok": ok, "critical": false,
            "detail": path.to_string_lossy(),
        }));
    }

    // 3) broken 版本
    let broken = &ctx.cfg.broken_versions;
    items.push(serde_json::json!({
        "name": "失败版本标记", "ok": broken.is_empty(), "critical": false,
        "detail": if broken.is_empty() { "无".to_string() }
                  else { format!("{} 个：{}", broken.len(), broken.join(", ")) },
    }));

    // 4) 已安装版本数
    let installed = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    items.push(serde_json::json!({
        "name": "已安装版本", "ok": !installed.is_empty(), "critical": false,
        "detail": if installed.is_empty() { "无".to_string() }
                  else { format!("{} 个", installed.len()) },
    }));

    // 5) 孤立 webview 目录（版本已卸载但 webview 残留）——只报告，不自动删
    let orphans = crate::maintenance::list_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
    items.push(serde_json::json!({
        "name": "孤立 webview 缓存", "ok": orphans.is_empty(), "critical": false,
        "detail": if orphans.is_empty() { "无".to_string() }
                  else { format!("{} 个：{}（可跑 clean 清理）", orphans.len(), orphans.join(", ")) },
    }));

    // 6) 环境检查（Node / pnpm / 磁盘 / npm 源等）—— CLI 为显式命令，跑完整检查
    let env_items = crate::envcheck::run_all(&ctx.dirs.root, true);
    for it in &env_items {
        if it.critical && !it.ok {
            fatal = true;
        }
        items.push(serde_json::json!({
            "name": it.name, "ok": it.ok, "critical": it.critical, "detail": it.detail,
        }));
    }

    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({ "ok": !fatal, "items": items }))
        );
    } else {
        for it in &items {
            let ok = it["ok"].as_bool().unwrap_or(false);
            let name = it["name"].as_str().unwrap_or("");
            let detail = it["detail"].as_str().unwrap_or("");
            let crit = if it["critical"].as_bool().unwrap_or(false) {
                ""
            } else {
                "（非致命）"
            };
            println!(
                "[{}] {}{}  {}",
                if ok { "✓" } else { "✗" },
                name,
                crit,
                detail
            );
        }
    }
    if fatal {
        1
    } else {
        0
    }
}

/// `clean`：清理孤立 webview 缓存 + 回收站（maintenance --cleanup 的超集）。
pub fn cmd_clean(dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "clean",
                }))
            );
        } else {
            println!(
                "[dry-run] 将清理孤立 webview 缓存（{}）",
                ctx.dirs.webview.to_string_lossy()
            );
            println!(
                "[dry-run] 将清空卸载回收站（{}）",
                ctx.dirs.trash.to_string_lossy()
            );
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        return crate::cli::output::info_fail(json, &format!("创建数据目录失败：{}", e));
    }
    let removed = maintenance::cleanup_orphan_webviews(&ctx.dirs.versions, &ctx.dirs.webview);
    let trashed = maintenance::cleanup_trash(&ctx.dirs.trash);
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "ok": true, "webviews": removed.len(), "trash": trashed,
            }))
        );
    } else {
        println!("已清理孤立缓存 {} 项", removed.len());
        println!("已清空回收站 {} 项", trashed);
    }
    0
}
