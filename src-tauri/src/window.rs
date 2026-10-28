//! 窗口 label 与平台（Windows 任务栏）辅助。
//!
//! 从 lib.rs 抽出（GAP-003，纯移动，无逻辑改动）。

/// 由版本号生成合法的窗口 label 前缀（不含代次）。
/// Tauri 要求 label 只含字母数字和 `-` `/` `:` `_`，而版本号含 `.`，故替换为 `_`。
pub(crate) fn window_label_prefix(version: &str) -> String {
    let sanitized: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("dsh-{}", sanitized)
}

/// 带代次的**唯一**窗口 label。
/// 用唯一 label 从根本上避免 "a webview with label ... already exists"：
/// close()/destroy() 都是异步投递，不能保证旧窗口立即消失，故不再复用 label。
pub(crate) fn window_label_gen(version: &str, gen: u64) -> String {
    format!("{}-{}", window_label_prefix(version), gen)
}

/// 从命令行参数解析出 --launch-version <版本>（精简启动模式用）
pub(crate) fn parse_launch_version(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--launch-version" {
            return it.next().cloned();
        }
    }
    None
}

/// 只设**进程级** AUMID——必须在任何窗口创建之前调用。
///
/// 若进程无 AUMID，任务栏会退化为"找指向该 exe 的快捷方式并取其图标"，
/// 于是所有指向管理器 exe 的桌面快捷方式（图标是 dsh 鲸鱼）都会污染管理器的任务栏图标。
/// Tauri 在 setup 回调之前就已创建配置里的主窗口，故不能放在 setup 里设，
/// 必须提前到 run() 的最开头（Builder 之前）。
#[cfg(windows)]
pub(crate) fn set_process_aumid() {
    let wide: Vec<u16> = MANAGER_AUMID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }
}

#[cfg(not(windows))]
pub(crate) fn set_process_aumid() {}

/// 注册各 AUMID 的任务栏图标（IconUri）——可在窗口创建后调用。
#[cfg(windows)]
pub(crate) fn register_aumid_icons() {
    if let Ok(exe) = std::env::current_exe() {
        // 管理器：图标 = 当前 exe（其内嵌图标即管理器的方块图标）
        crate::launcher::register_aumid_icon(MANAGER_AUMID, &exe);
        // dsh 内嵌窗口：图标 = dsh 官方图标（鲸鱼）
        if let Some(ico) = crate::launcher::materialize_dsh_icon(&exe) {
            crate::launcher::register_aumid_icon(DSH_WINDOW_AUMID, &ico);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn register_aumid_icons() {}

/// 管理器进程级 AppUserModelID（须与 tauri.conf.json 的 identifier 一致）。
#[cfg(windows)]
pub(crate) const MANAGER_AUMID: &str = "io.github.K1-lihongrong.dsh-multiver";

/// dsh 内嵌窗口专用的 AppUserModelID。
/// 与进程级 AUMID（管理器的）不同，用来让"运行 dsh"的窗口在任务栏：
///   - 显示 dsh 官方图标（而非管理器图标）
///   - 与管理器窗口分开成组
#[cfg(windows)]
pub(crate) const DSH_WINDOW_AUMID: &str = "io.github.K1-lihongrong.dsh-multiver.dsh";

/// 为某个窗口单独设置 AppUserModelID（窗口级，非进程级）。
///
/// Windows 任务栏按 AUMID 取图标与分组；同一进程内的多个窗口默认共用进程级 AUMID。
/// 这里用 SHGetPropertyStoreForWindow 拿窗口的 IPropertyStore，写入
/// PKEY_AppUserModel_ID，从而让 dsh 窗口用独立的 AUMID。
/// 必须在窗口**显示之前**调用（否则任务栏可能已按旧 AUMID 建项）。
#[cfg(windows)]
pub(crate) fn set_window_app_user_model_id(
    window: &tauri::WebviewWindow,
    aumid: &str,
) -> Result<(), String> {
    use windows::core::GUID;
    use windows::Win32::Foundation::{HWND, PROPERTYKEY};
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow,
    };

    // PKEY_AppUserModel_ID = {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, 5
    const PKEY_APPUSERMODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
        pid: 5,
    };

    let raw = window
        .hwnd()
        .map_err(|e| format!("取窗口句柄失败: {}", e))?;
    let hwnd = HWND(raw.0);

    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)
            .map_err(|e| format!("SHGetPropertyStoreForWindow 失败: {}", e))?;
        let pv = PROPVARIANT::from(aumid);
        store
            .SetValue(&PKEY_APPUSERMODEL_ID, &pv)
            .map_err(|e| format!("IPropertyStore::SetValue 失败: {}", e))?;
        // SetValue 只写缓冲区，必须 Commit 才真正落盘（否则不生效）
        store
            .Commit()
            .map_err(|e| format!("IPropertyStore::Commit 失败: {}", e))?;
    }
    Ok(())
}


/// M2m：确保主窗口存在并聚焦。
///
/// 场景：第二实例被单实例插件转交给已运行实例。
/// - 主窗口还在 → unminimize + show + set_focus
/// - 主窗口已被 launch-version 流程 close（lib.rs 里 launch 成功后 close main）→ 按 tauri.conf.json 重建
///
/// Windows 前台锁可能拒绝 set_focus（用户正在别的应用里操作）；此时退化 request_user_attention。
/// 返回值说明用了哪条路径，便于日志与验收记录。
pub(crate) fn ensure_main_window(app: &tauri::AppHandle) -> Result<&'static str, String> {
    use tauri::Manager;

    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        match main.set_focus() {
            Ok(()) => return Ok("focused"),
            Err(e) => {
                // 前台锁拒绝：退化到任务栏闪烁提示
                let _ = main.request_user_attention(Some(tauri::UserAttentionType::Informational));
                // M2m（B3）：release 无控制台，eprintln 会丢，同时写文件日志。
                let mdir = crate::manager_dir(app);
                let root = crate::Config::load(&mdir).resolve_root(&mdir);
                crate::logging::log_app_error(
                    &root,
                    "ensure-main-window",
                    &format!("聚焦失败（已退化任务栏提示）: {}", e),
                );
                eprintln!("[dsh-multiver] 单实例聚焦失败（已退化任务栏提示）: {}", e);
                return Ok("attention");
            }
        }
    }

    // 主窗口不存在：按配置重建。
    //
    // M2m（B1）：重建前必须清除启动态。原因：若本进程最初是以 `--launch-version V`
    // 启动的，`LAUNCH_MODE` 仍是 V；新窗口前端 `onMounted` 调 `get_launch_mode()`
    // 会读到 V 而进入「启动态」，但 `launch-mode-start` / `launch-progress` 事件
    // 只在首次启动流程发出，重建窗口收不到 → 永远卡在「正在启动」。
    *crate::lock_launch_mode() = None;

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or_else(|| "tauri.conf.json 中找不到 label=main 的窗口配置".to_string())?;

    let win = tauri::WebviewWindowBuilder::from_config(app, &config)
        .map_err(|e| format!("重建主窗口失败（from_config）: {}", e))?
        .build()
        .map_err(|e| format!("重建主窗口失败（build）: {}", e))?;

    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    Ok("rebuilt")
}
