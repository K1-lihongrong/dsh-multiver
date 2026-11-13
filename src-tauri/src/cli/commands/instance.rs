//! `instance` 子命令：create / list / rename / remove。

use crate::cli::load_ctx;
use crate::cli::output::{info_fail, json_str};
use crate::domain::InstanceId;
use crate::instances;
use crate::versions;

fn fmt_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.2} MB", f / MB)
    } else if f >= KB {
        format!("{:.2} KB", f / KB)
    } else {
        format!("{n} B")
    }
}

fn validate_dry_run_attaches(
    versions_dir: &std::path::Path,
    attaches: &[String],
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for raw in attaches {
        let version = crate::domain::VersionId::new(raw)?;
        if !seen.insert(version.clone()) {
            return Err(format!("重复的 --attach：{version}"));
        }
        if !versions::exists(versions_dir, version.as_str()) {
            return Err(format!("版本 {version} 未安装，不能挂载实例"));
        }
    }
    Ok(())
}

pub fn cmd_create(name: &str, attaches: &[String], dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    let clean_name = match ctx.cfg.validate_instance_name(name, None) {
        Ok(v) => v,
        Err(e) => return info_fail(json, &e),
    };
    if let Err(e) = validate_dry_run_attaches(&ctx.dirs.versions, attaches) {
        return info_fail(json, &e);
    }
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "instance-create",
                    "name": clean_name, "attaches": attaches,
                }))
            );
        } else {
            println!("[dry-run] 将创建实例「{}」", clean_name);
            if attaches.is_empty() {
                println!("[dry-run] 不挂载任何版本（孤儿实例）");
            } else {
                println!("[dry-run] 挂载版本：{}", attaches.join(", "));
            }
            println!("[dry-run] 不创建目录（首次运行时创建）");
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }

    let id = match instances::create(
        &mut ctx.cfg,
        &ctx.mdir,
        &ctx.dirs.versions,
        &clean_name,
        attaches,
    ) {
        Ok(id) => id,
        Err(e) => return info_fail(json, &e),
    };
    let home = ctx.dirs.homes.join(id.as_str());
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "ok": true, "id": id.to_string(), "name": clean_name,
                "attached_versions": attaches,
                "home": home.to_string_lossy(),
            }))
        );
    } else {
        println!("已创建实例 {}（{}）", id, clean_name);
        println!("home：{}", home.to_string_lossy());
        if attaches.is_empty() {
            println!("（尚未挂载任何版本）");
        } else {
            println!("已挂载：{}", attaches.join(", "));
        }
    }
    0
}

pub fn cmd_list(json: bool, sizes: bool) -> i32 {
    let ctx = load_ctx();
    let mut view = instances::views(&ctx.cfg, &ctx.dirs, &std::collections::BTreeMap::new());
    if sizes {
        for inst in &mut view.instances {
            let path = std::path::Path::new(&inst.home);
            inst.size = Some(versions::dir_size(path));
        }
    }

    if json {
        println!("{}", json_str(&view));
        return 0;
    }
    if view.instances.is_empty() {
        println!("暂无实例。用 instance create --name <名称> --attach <版本> 创建。");
        return 0;
    }
    println!(
        "{:<6} {:<20} {:<8} {:<28} 占用",
        "ID", "名称", "状态", "挂载版本"
    );
    for inst in &view.instances {
        let mounted = if inst.mounted_versions.is_empty() {
            "（未挂载）".to_string()
        } else {
            inst.mounted_versions.join(",")
        };
        println!(
            "{:<6} {:<20} {:<8} {:<28} {}",
            inst.id,
            inst.name,
            inst.state,
            mounted,
            inst.size.map(fmt_bytes).unwrap_or_else(|| "-".into())
        );
    }
    0
}

pub fn cmd_rename(id: &str, name: &str, json: bool) -> i32 {
    let mut ctx = load_ctx();
    let id = match InstanceId::new(id) {
        Ok(v) => v,
        Err(e) => return info_fail(json, &e),
    };
    if let Err(e) = instances::rename(&mut ctx.cfg, &ctx.mdir, &id, name) {
        return info_fail(json, &e);
    }
    let clean = ctx
        .cfg
        .instances
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.name.clone())
        .unwrap_or_default();
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({ "ok": true, "id": id.to_string(), "name": clean }))
        );
    } else {
        println!("实例 {} 已改名为「{}」", id, clean);
    }
    0
}

pub fn cmd_remove(id: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    let id = match InstanceId::new(id) {
        Ok(v) => v,
        Err(e) => return info_fail(json, &e),
    };
    if !ctx.cfg.instance_exists(&id) {
        return info_fail(json, &format!("实例 {} 不存在", id));
    }
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "instance-remove",
                    "id": id.to_string(),
                }))
            );
        } else {
            println!("[dry-run] 将删除实例 {}（home 移入 trash）", id);
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    match instances::remove(&mut ctx.cfg, &ctx.mdir, &ctx.dirs.root, &id) {
        Ok(trash) => {
            if json {
                println!(
                    "{}",
                    json_str(&serde_json::json!({
                        "ok": true, "id": id.to_string(),
                        "trash": trash.as_ref().map(|p| p.to_string_lossy().to_string()),
                    }))
                );
            } else {
                println!("实例 {} 已删除", id);
                if let Some(p) = trash {
                    println!("目录已移入回收站：{}", p.to_string_lossy());
                }
            }
            0
        }
        Err(e) => info_fail(json, &e),
    }
}
