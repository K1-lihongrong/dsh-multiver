//! 版本相关命令：list / which / info / versions / install / uninstall。

use crate::cli::console::progress_is_tty;
use crate::cli::output::{fmt_bytes, info_fail, json_str, out_result, pct_of, which_fail};
use crate::cli::{load_ctx, Ctx};
use crate::versions;

/// `list`：默认输出一行一个版本号（默认版本加 `* ` 前缀）；
/// `--json` 输出对象数组（含 is_default / isolated / installed_at 等）。
pub fn cmd_list(json: bool) -> i32 {
    let ctx = load_ctx();
    let list = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    if json {
        println!("{}", json_str(&list));
    } else {
        for v in &list {
            if v.is_default {
                println!("* {}", v.version);
            } else {
                println!("{}", v.version);
            }
        }
    }
    0
}

/// `which`：输出默认版本 dsh 入口的绝对路径。
/// 无默认版本 / 入口不存在 → 失败（退出码 1）。
pub fn cmd_which(json: bool) -> i32 {
    let ctx = load_ctx();
    let Some(def) = ctx.cfg.default_version.as_deref() else {
        return which_fail(json, "未设置默认版本（先用 set-default <版本>）");
    };
    let bin = ctx
        .dirs
        .versions
        .join(def)
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if !bin.exists() {
        return which_fail(
            json,
            &format!("默认版本 {} 的入口不存在：{}", def, bin.to_string_lossy()),
        );
    }
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "version": def,
                "path": bin.to_string_lossy(),
            }))
        );
    } else {
        println!("{}", bin.to_string_lossy());
    }
    0
}

/// `install <版本> [--registry <url>]`
pub fn cmd_install(version: &str, registry: Option<&str>, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true,
                    "action": "install",
                    "version": version,
                    "registry": registry,
                }))
            );
        } else {
            println!(
                "[dry-run] 将安装版本 {} 到 {}",
                version,
                ctx.dirs.versions.join(version).to_string_lossy()
            );
            if let Some(r) = registry {
                println!("[dry-run] 使用 npm 源：{}", r);
            }
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if let Err(e) = ctx.dirs.ensure() {
        out_result(json, false, &format!("创建数据目录失败：{}", e));
        return 1;
    }

    let tty = progress_is_tty();
    let last = std::cell::Cell::new(u32::MAX);
    let on_progress = |ev: &versions::ProgressEvent| {
        if !tty {
            return;
        }
        let pct = pct_of(ev);
        if pct != last.get() {
            last.set(pct);
            eprint!("\r正在安装 {}  {:>3}%  {}", version, pct, ev.stage);
            let _ = std::io::Write::flush(&mut std::io::stderr());
        }
    };

    let (ok, msg) = versions::install(
        &ctx.dirs.versions,
        &ctx.dirs.store,
        &ctx.dirs.cache,
        &ctx.dirs.state,
        version,
        &on_progress,
        registry,
    );
    if tty {
        eprintln!(); // 收尾进度行
    }

    // 维护 broken_versions（与 GUI 的 install_version 保持一致）
    if ok {
        if ctx.cfg.broken_versions.iter().any(|v| v == version) {
            ctx.cfg.broken_versions.retain(|v| v != version);
            let _ = ctx.cfg.save(&ctx.mdir);
        }
        out_result(json, true, &msg);
        0
    } else {
        let kind = versions::classify_error(&msg);
        let is_broken_kind = kind == "private-package" || kind == "incomplete-version";
        if is_broken_kind && !ctx.cfg.broken_versions.iter().any(|v| v == version) {
            ctx.cfg.broken_versions.push(version.to_string());
            let _ = ctx.cfg.save(&ctx.mdir);
        }
        out_result(json, false, &msg);
        1
    }
}

/// `uninstall <版本>`（与 GUI 的 uninstall_version 行为一致）
pub fn cmd_uninstall(version: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "uninstall", "version": version,
                    "installed": versions::exists(&ctx.dirs.versions, version),
                }))
            );
            return 0;
        }
        let installed = versions::exists(&ctx.dirs.versions, version);
        println!("[dry-run] 将卸载版本 {}", version);
        if installed {
            println!("[dry-run] 该版本已安装");
        } else {
            println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
        }
        if ctx.cfg.default_version.as_deref() == Some(version) {
            println!("[dry-run] 注意：这是当前默认版本，卸载后会清除默认设置");
        }
        if ctx.cfg.isolated_versions.iter().any(|v| v == version) {
            println!("[dry-run] 注意：该版本开启了数据隔离，会一并移除隔离标记");
        }
        println!("[dry-run] 未执行任何操作");
        return 0;
    }

    // 卸载的是默认版本 / 隔离版本 → 同步清配置，并重生成转发脚本
    let mut need_save = false;
    if ctx.cfg.default_version.as_deref() == Some(version) {
        ctx.cfg.default_version = None;
        need_save = true;
    }
    if ctx.cfg.isolated_versions.iter().any(|v| v == version) {
        ctx.cfg.isolated_versions.retain(|v| v != version);
        need_save = true;
    }
    if need_save {
        let _ = ctx.cfg.save(&ctx.mdir);
    }
    crate::paths::regenerate_forward_script(&ctx.mdir, &ctx.cfg);

    // 快速卸载：rename 到回收站（瞬时），后台再删；CLI 为一次性进程，故同步删回收站
    let (ok, msg) = versions::uninstall_fast(&ctx.dirs.versions, &ctx.dirs.trash, version);
    if ok {
        // 顺带把该版本的 WebView2 数据目录也移入回收站
        let wv = ctx.dirs.webview.join(version);
        if wv.exists() {
            let _ = std::fs::create_dir_all(&ctx.dirs.trash);
            let dest = ctx
                .dirs
                .trash
                .join(format!("webview-{}-{}", version, std::process::id()));
            if std::fs::rename(&wv, &dest).is_err() {
                let _ = std::fs::remove_dir_all(&wv);
            }
        }
        // 不在这里同步清空回收站：CLI 也要保持"秒退"。
        // trash 残留由下次启动的维护任务（cleanup_trash）或 maintenance 清理。
        out_result(json, true, &msg);
        0
    } else {
        out_result(json, false, &msg);
        1
    }
}

/// `info <版本>`：查看单个版本详情（人读多行 / JSON 对象）。
pub fn cmd_info(version: &str, json: bool) -> i32 {
    let ctx = load_ctx();
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    let list = versions::list(
        &ctx.dirs.versions,
        ctx.cfg.default_version.as_deref(),
        &ctx.cfg.isolated_versions,
    );
    let Some(info) = list.iter().find(|v| v.version == version) else {
        return info_fail(json, &format!("版本 {} 未安装", version));
    };
    let size = cached_version_size(&ctx, version);
    // 非隔离时 DSH_HOME 可能是共享 home 或官方 ~/.dsh，界面/脚本都需要看到实际值
    let home = crate::paths::resolve_home(&ctx.cfg, &ctx.dirs, version);

    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "version": info.version,
                "path": info.path,
                "is_default": info.is_default,
                "isolated": info.isolated,
                "shared_home": info.shared_home,
                "installed_at": info.installed_at,
                "home": home.to_string_lossy(),
                "size": size,
            }))
        );
    } else {
        println!("版本:     {}", info.version);
        println!("路径:     {}", info.path);
        println!("默认:     {}", if info.is_default { "是" } else { "否" });
        if info.isolated {
            println!("隔离:     是（独立 home）");
        } else if ctx.cfg.use_official_dsh_home {
            println!("隔离:     否（官方配置目录）");
        } else {
            println!("隔离:     否（共享 home）");
        }
        println!("配置目录: {}", home.to_string_lossy());
        if !info.installed_at.is_empty() {
            println!("安装于:   {}", info.installed_at);
        }
        println!(
            "占用:     {}（复用 {} / 独占 {}）",
            fmt_bytes(size.total),
            fmt_bytes(size.shared_size),
            fmt_bytes(size.exclusive_size)
        );
    }
    0
}

/// 占用扫描走缓存（与 GUI 同口径，版本级）。命中直接返回，未命中现扫并写缓存。
fn cached_version_size(ctx: &Ctx, version: &str) -> versions::SizeInfo {
    let store_mtime = crate::sizecache::dir_mtime(&ctx.dirs.store);
    let dir_mtime = crate::sizecache::dir_mtime(&ctx.dirs.versions.join(version));
    let mut cache = crate::sizecache::load(&ctx.dirs.root);
    if crate::sizecache::is_valid(&cache, version, dir_mtime, store_mtime) {
        if let Some(info) = crate::sizecache::get_info(&cache, version) {
            return info;
        }
    }
    let info = versions::version_size_detail(&ctx.dirs.versions, version);
    crate::sizecache::put(&mut cache, version, dir_mtime, store_mtime, info.clone());
    let _ = crate::sizecache::save(&ctx.dirs.root, &cache);
    info
}

/// `versions`：列出远端可用版本（从 npm 查询，最新在前）。
pub fn cmd_versions(json: bool) -> i32 {
    let ctx = load_ctx();
    let (ok, list, err) =
        crate::actions::list_remote(&ctx.dirs.store, &ctx.dirs.cache, &ctx.dirs.state);
    if !ok {
        return info_fail(json, &format!("获取可用版本失败：{}", err));
    }
    if json {
        println!("{}", json_str(&list));
    } else {
        for v in &list {
            println!("{}", v);
        }
    }
    0
}
