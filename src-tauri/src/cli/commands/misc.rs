//! 其余命令：isolate / shortcut / copy-shared / clear-isolated / export / import / run / setup-runtime / update。

use crate::cli::load_ctx;
use crate::cli::output::{info_fail, json_str, out_result};
use crate::runtime::RuntimeProgress;
use crate::versions;

/// 便携运行时下载的进度回调：TTY 下单行刷新「阶段 + 已下载 / 总量 (百分比)」。
///
/// `json` 模式下不输出（stdout 只能有 JSON）。Node 与 pnpm 两段下载共用此回调
/// （原先两处逐字重复）。
fn runtime_progress_cb(json: bool) -> impl Fn(RuntimeProgress) {
    move |p| {
        if json {
            return;
        }
        match p.total {
            Some(t) if t > 0 => eprint!(
                "\r{}  {:.1} / {:.1} MB ({:.0}%)",
                p.stage,
                p.done as f64 / 1e6,
                t as f64 / 1e6,
                p.done as f64 * 100.0 / t as f64
            ),
            _ => eprint!("\r{}", p.stage),
        }
    }
}

/// `export [<文件>]`：导出配置 + 版本清单（JSON）。默认写到数据根目录。
pub fn cmd_export(file: Option<&str>, json: bool) -> i32 {
    let ctx = load_ctx();
    let out = match file {
        Some(f) => std::path::PathBuf::from(f),
        None => ctx.dirs.root.join(format!(
            "dsh-multiver-backup-{}.json",
            crate::logging::now_secs()
        )),
    };
    match crate::backup::export_to_file(&ctx.cfg, &ctx.dirs.versions, &out) {
        Ok(path) => {
            if json {
                println!(
                    "{}",
                    json_str(&serde_json::json!({ "ok": true, "path": path }))
                );
            } else {
                println!("已导出：{}", path);
            }
            0
        }
        Err(e) => {
            out_result(json, false, &e);
            1
        }
    }
}

/// `shortcut <版本>`：为某版本创建桌面快捷方式。
pub fn cmd_shortcut(version: &str, dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    let exe = crate::manager_dir_plain().join(if cfg!(windows) {
        "dsh-multiver.exe"
    } else {
        "dsh-multiver"
    });
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "shortcut", "version": version,
                }))
            );
        } else {
            println!("[dry-run] 将为版本 {} 创建桌面快捷方式", version);
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    match crate::launcher::create_desktop_shortcut(&exe, version) {
        Ok(path) => {
            out_result(json, true, &format!("已创建桌面快捷方式：{}", path));
            0
        }
        Err(e) => {
            out_result(json, false, &e);
            1
        }
    }
}

/// `copy-shared <版本>`：把共享 home 数据复制到该隔离版本的独立 home。
/// 该版本必须已开启隔离。
pub fn cmd_copy_shared(version: &str, dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    if !ctx.cfg.isolated_versions.iter().any(|v| v == version) {
        return info_fail(
            json,
            &format!("{} 未开启隔离（先用 isolate {} 开启）", version, version),
        );
    }
    let shared = ctx.dirs.home.clone();
    let isolated = versions::isolated_home(&ctx.dirs.versions, version);
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "copy-shared", "version": version,
                    "from": shared.to_string_lossy(), "to": isolated.to_string_lossy(),
                }))
            );
        } else {
            println!(
                "[dry-run] 将把 {} 复制到 {}",
                shared.to_string_lossy(),
                isolated.to_string_lossy()
            );
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    let (ok, msg, _) = versions::copy_shared_to_isolated(&shared, &isolated);
    out_result(json, ok, &msg);
    if ok {
        0
    } else {
        1
    }
}

/// `clear-isolated <版本>`：清空该隔离版本的独立 home（保留版本本身）。
pub fn cmd_clear_isolated(version: &str, dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    if dry_run {
        let home = versions::isolated_home(&ctx.dirs.versions, version);
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "clear-isolated", "version": version,
                    "home": home.to_string_lossy(),
                }))
            );
        } else {
            println!("[dry-run] 将清空隔离数据：{}", home.to_string_lossy());
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    let (ok, msg) = versions::clear_isolated(&ctx.dirs.versions, version);
    out_result(json, ok, &msg);
    if ok {
        0
    } else {
        1
    }
}

/// `isolate <版本> [--off]`：开关数据隔离（默认开启；--off 关闭）。
/// 与 GUI 的 set_isolated 行为一致：改配置 + 预创建隔离目录 + 重生成转发脚本。
pub fn cmd_isolate(version: &str, on: bool, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    if dry_run {
        let installed = versions::exists(&ctx.dirs.versions, version);
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true, "action": "isolate", "version": version,
                    "isolated": on, "installed": installed,
                }))
            );
        } else {
            println!(
                "[dry-run] 将{}版本 {} 的数据隔离",
                if on { "开启" } else { "关闭" },
                version
            );
            if !installed {
                println!("[dry-run] 警告：该版本未安装（实际执行会失败）");
            }
            println!("[dry-run] 会重生成终端转发脚本 dsh（若该版本是默认版本）");
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }
    if !versions::exists(&ctx.dirs.versions, version) {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    ctx.cfg.isolated_versions.retain(|v| v != version);
    if on {
        ctx.cfg.isolated_versions.push(version.to_string());
        let _ = std::fs::create_dir_all(ctx.dirs.versions.join(version).join("home"));
    }
    if let Err(e) = ctx.cfg.save(&ctx.mdir) {
        return info_fail(json, &format!("保存配置失败：{}", e));
    }
    crate::paths::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
    let msg = if on {
        format!("{} 已开启数据隔离", version)
    } else {
        format!("{} 已关闭数据隔离", version)
    };
    out_result(json, true, &msg);
    0
}

/// `import <文件>`：导入备份文件（export 生成的）里的配置。
/// `--dry-run` 只预览将改动的字段，不应用。
pub fn cmd_import(file: &str, dry_run: bool, json: bool) -> i32 {
    let mut ctx = load_ctx();
    let path = std::path::Path::new(file);

    // 预览
    let preview = match crate::backup::preview_import(path, &ctx.cfg, &ctx.dirs.versions) {
        Ok(p) => p,
        Err(e) => {
            out_result(json, false, &e);
            return 1;
        }
    };

    if dry_run {
        if json {
            println!("{}", json_str(&preview));
        } else {
            println!("[dry-run] 将应用以下改动：");
            if preview.changes.is_empty() {
                println!("  （无需改动）");
            } else {
                for c in &preview.changes {
                    println!("  · {}", c);
                }
            }
            if !preview.missing_versions.is_empty() {
                println!("[dry-run] 注意：以下版本本机未安装（仅恢复配置，需另行安装）：");
                for v in &preview.missing_versions {
                    println!("  · {}", v);
                }
            }
            println!("[dry-run] 未执行任何操作");
        }
        return 0;
    }

    // 应用
    match crate::backup::apply_import(path, &mut ctx.cfg) {
        Ok(applied) => {
            if let Err(e) = ctx.cfg.save(&ctx.mdir) {
                out_result(json, false, &format!("保存配置失败：{}", e));
                return 1;
            }
            crate::paths::regenerate_forward_script(&ctx.mdir, &ctx.cfg);
            let msg = if applied.is_empty() {
                "配置无变化".to_string()
            } else {
                format!("已应用：{}", applied.join("、"))
            };
            out_result(json, true, &msg);
            0
        }
        Err(e) => {
            out_result(json, false, &e);
            1
        }
    }
}

/// `run <版本>`：启动 dsh web（新控制台窗口 + 自动打开系统浏览器）。
/// 与 GUI 的「浏览器打开」行为一致：关掉控制台窗口即停止服务。
pub fn cmd_run(version: &str, json: bool) -> i32 {
    let ctx = load_ctx();
    let vdir = ctx.dirs.versions.join(version);
    if !vdir.join("node_modules").exists() {
        return info_fail(json, &format!("版本 {} 未安装", version));
    }
    let home = crate::paths::resolve_home(&ctx.cfg, &ctx.dirs, version);

    // 端口：固定 3080；被占用则随机（CLI 无交互，直接随机）
    let port: u16 = if crate::launcher::port_in_use(3080) {
        0
    } else {
        3080
    };
    let (ok, msg) = crate::launcher::spawn_web_console(
        &vdir,
        &home,
        &ctx.dirs.store,
        &ctx.dirs.cache,
        &ctx.dirs.state,
        port,
    );
    if ok {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "ok": true, "version": version, "port": port, "home": home.to_string_lossy(),
                }))
            );
        } else {
            println!("{}", msg);
            println!("数据目录：{}", home.to_string_lossy());
            println!("（关闭弹出的控制台窗口即可停止服务）");
        }
        0
    } else {
        out_result(json, false, &msg);
        1
    }
}

/// `setup-runtime`：检测并下载缺失的便携运行时。
///
/// - `--dry-run`：只报告检测结果（缺什么），不下载
/// - 正常：下载缺失的便携 Node 到 `<root>/tools/node/`
pub fn cmd_setup_runtime(dry_run: bool, json: bool) -> i32 {
    let ctx = load_ctx();
    let root = &ctx.dirs.root;
    let sys_node = crate::runtime::system_node_ok();
    let bun_node = crate::runtime::bundled_node_ok(root);
    let sys_pnpm = crate::runtime::system_pnpm_ok();
    let bun_pnpm = crate::runtime::bundled_pnpm_ok(root);

    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "dry_run": true,
                    "system_node_ok": sys_node,
                    "bundled_node_ok": bun_node,
                    "system_pnpm_ok": sys_pnpm,
                    "bundled_pnpm_ok": bun_pnpm,
                    "need_node": !sys_node && !bun_node,
                    "need_pnpm": !sys_pnpm && !bun_pnpm,
                }))
            );
        } else {
            println!("[预览] 运行时检测：");
            println!("  系统 Node：{}", if sys_node { "可用" } else { "不可用" });
            println!("  便携 Node：{}", if bun_node { "已就绪" } else { "缺失" });
            println!("  系统 pnpm：{}", if sys_pnpm { "可用" } else { "不可用" });
            println!("  便携 pnpm：{}", if bun_pnpm { "已就绪" } else { "缺失" });
            if !sys_node && !bun_node {
                println!("  → 将下载便携 Node（约 30MB）");
            } else {
                println!("  → 无需下载 Node");
            }
            if !sys_pnpm && !bun_pnpm {
                println!("  → 将下载便携 pnpm（约 22MB）");
            } else {
                println!("  → 无需下载 pnpm");
            }
        }
        return 0;
    }

    // Node：系统有就跳过；否则下便携版
    let mut node_ok = sys_node || bun_node;
    let mut node_msg = if sys_node {
        "系统 Node 可用".to_string()
    } else if bun_node {
        "便携 Node 已就绪".to_string()
    } else {
        String::new()
    };
    if !sys_node && !bun_node {
        if !json {
            eprintln!("正在下载便携 Node（约 30MB）…");
        }
        let on_prog = runtime_progress_cb(json);
        match crate::runtime::install_bundled_node(root, &on_prog) {
            Ok(dir) => {
                node_ok = true;
                node_msg = format!("已安装便携 Node：{}", dir.to_string_lossy());
            }
            Err(e) => {
                node_msg = format!("下载便携 Node 失败：{}", e);
            }
        }
    }

    // pnpm：系统有就跳过；否则下便携独立版
    let mut pnpm_ok = sys_pnpm || bun_pnpm;
    let mut pnpm_msg = if sys_pnpm {
        "系统 pnpm 可用".to_string()
    } else if bun_pnpm {
        "便携 pnpm 已就绪".to_string()
    } else {
        String::new()
    };
    if !sys_pnpm && !bun_pnpm {
        if !json {
            eprintln!("正在下载便携 pnpm（约 22MB）…");
        }
        let on_prog = runtime_progress_cb(json);
        match crate::runtime::install_bundled_pnpm(root, &on_prog) {
            Ok(dir) => {
                pnpm_ok = true;
                pnpm_msg = format!("已安装便携 pnpm：{}", dir.to_string_lossy());
            }
            Err(e) => {
                pnpm_msg = format!("下载便携 pnpm 失败：{}", e);
            }
        }
    }

    if !json {
        eprintln!();
    }
    let ok = node_ok && pnpm_ok;
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({
                "ok": ok,
                "node": node_msg,
                "pnpm": pnpm_msg,
            }))
        );
    } else {
        println!("{}", node_msg);
        println!("{}", pnpm_msg);
    }
    if ok {
        0
    } else {
        1
    }
}

/// `update`：检查并执行自动更新。
///
/// - `--dry-run`：只检查是否有新版，不下载
/// - 正常：下载 → 校验 → 生成替换批处理 → 退出（由批处理替换并重启）
pub fn cmd_update(dry_run: bool, json: bool) -> i32 {
    let info = crate::updater::check();
    if !info.has_update {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "has_update": false, "current": info.current, "latest": info.latest,
                }))
            );
        } else {
            println!("已是最新版本（{}）", info.current);
        }
        return 0;
    }
    let latest = info.latest.clone().unwrap_or_default();
    if dry_run {
        if json {
            println!(
                "{}",
                json_str(&serde_json::json!({
                    "has_update": true, "current": info.current, "latest": latest,
                    "download_url": info.download_url, "sha256": info.sha256, "dry_run": true,
                }))
            );
        } else {
            println!("发现新版本 {}（当前 {}）", latest, info.current);
            println!("运行不带 --dry-run 的 update 即可下载并更新");
        }
        return 0;
    }
    let Some(url) = info.download_url.clone() else {
        out_result(json, false, "未找到下载地址（Release 资产缺失）");
        return 1;
    };
    if !json {
        eprintln!("正在下载 v{} …", latest);
    }
    let new_exe = match crate::updater::download_and_verify(&url, info.sha256.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            out_result(json, false, &format!("下载或校验失败：{}", e));
            return 1;
        }
    };
    let target = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            out_result(json, false, &format!("无法定位当前程序：{}", e));
            return 1;
        }
    };
    match crate::updater::spawn_replacer(&new_exe, &target) {
        Ok(_) => {
            out_result(
                json,
                true,
                &format!("更新就绪（v{}），管理器将退出以完成替换", latest),
            );
            0
        }
        Err(e) => {
            out_result(json, false, &format!("启动替换程序失败：{}", e));
            1
        }
    }
}
