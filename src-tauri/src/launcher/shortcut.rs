//! 桌面快捷方式与 Windows 桌面集成（D14 拆分：原 launcher.rs 的快捷方式）。
//!
//! - create_desktop_shortcut：Windows .lnk / Unix .desktop
//! - build_shortcut_script / build_set_aumid_ps：PowerShell 脚本生成
//! - register_aumid_icon / materialize_dsh_icon：任务栏图标
//! - desktop_dir / desktop_from_registry：桌面目录定位

use std::path::{Path, PathBuf};
use std::process::Command;
/// dsh 官方图标（.ico），编译期内嵌进 exe。
///
/// 来源：deepseek-ai/deepseek-harness 的 apps/desktop/resources/icon-windows.png
/// （由 `gen-dsh-shortcut-icon.mjs` 转成 ico）。按 BRAND_GUIDELINES，快捷方式如实指向"启动 dsh"，
/// 用 dsh 官方图标标识 dsh 本身是合理的。
#[cfg(windows)]
const DSH_SHORTCUT_ICO: &[u8] = include_bytes!("../../icons/dsh-shortcut.ico");

/// 在注册表为某 AUMID 注册任务栏图标（IconUri 指向给定 .ico）。
/// 设了 AUMID 后 Windows 从 `HKCU\Software\Classes\AppUserModelId\<aumid>` 的 IconUri 取图标，
/// 不注册会显示占位图。
#[cfg(windows)]
pub(crate) fn register_aumid_icon(aumid: &str, ico_path: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let key = format!("HKCU\\Software\\Classes\\AppUserModelId\\{}", aumid);
    let _ = std::process::Command::new("reg")
        .args([
            "add",
            &key,
            "/v",
            "IconUri",
            "/t",
            "REG_SZ",
            "/d",
            &ico_path.to_string_lossy(),
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// 把内嵌的 dsh 图标释放到 `<manager_dir>/assets/dsh-shortcut.ico`，返回其路径。
/// 已存在且大小一致则跳过（避免每次重写）。
#[cfg(windows)]
pub(crate) fn materialize_dsh_icon(exe_path: &Path) -> Option<PathBuf> {
    let dir = exe_path.parent()?.join("assets");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("dsh-shortcut.ico");
    // 大小一致就认为已是最新
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() == DSH_SHORTCUT_ICO.len() as u64 {
            return Some(path);
        }
    }
    std::fs::write(&path, DSH_SHORTCUT_ICO).ok()?;
    Some(path)
}

/// 为某版本在桌面创建快捷方式。
///
/// - Windows：`DSH <版本>.lnk`，用 PowerShell COM 生成（零依赖）；图标用 dsh 官方图标
/// - Unix：`DSH <版本>.desktop`，写 Desktop Entry 文件并置可执行
#[cfg(windows)]
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir().ok_or_else(|| "无法定位桌面目录".to_string())?;
    let lnk_name = format!("DSH {}.lnk", version);
    let lnk_path = desktop.join(&lnk_name);

    // 快捷方式图标用 dsh 官方图标（而非管理器图标），语义为"启动 dsh"
    let icon = materialize_dsh_icon(exe_path);
    let script = build_shortcut_script(&lnk_path, exe_path, version, icon.as_deref());

    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("调用 PowerShell 失败: {}", e))?
    };
    #[cfg(not(windows))]
    let out = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&script)
        .output()
        .map_err(|e| format!("调用 PowerShell 失败: {}", e))?;

    if out.status.success() {
        Ok(lnk_path.to_string_lossy().to_string())
    } else {
        let e = crate::console_decode::decode_console(&out.stderr);
        Err(format!("创建快捷方式失败: {}", e.trim()))
    }
}

/// Unix：生成 `.desktop` 桌面入口。
///
/// 无桌面环境时返回清晰错误（见 desktop_dir）。已在 WSL 上验证（GAP-005）。
#[cfg(unix)]
pub fn create_desktop_shortcut(exe_path: &Path, version: &str) -> Result<String, String> {
    let desktop = desktop_dir()
        .ok_or_else(|| "未找到桌面目录（当前环境可能没有桌面，如 WSL/服务器/容器）".to_string())?;
    let file = desktop.join(format!("DSH {}.desktop", version));
    let work = exe_path.parent().unwrap_or(Path::new("."));
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=DSH {ver}\n\
         Comment=运行 DeepSeek Harness {ver}\n\
         Exec=\"{exe}\" --launch-version {ver}\n\
         Path={work}\n\
         Terminal=false\n\
         Categories=Development;\n",
        ver = version,
        exe = exe_path.to_string_lossy(),
        work = work.to_string_lossy()
    );
    std::fs::write(&file, content).map_err(|e| e.to_string())?;
    // 置为可执行（部分桌面环境要求）
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
    }
    // GNOME 需要"信任"标记才会允许直接双击；失败不阻塞（KDE/XFCE 无需）
    let _ = Command::new("gio")
        .args(["set", &file.to_string_lossy(), "metadata::trusted", "true"])
        .output();
    Ok(file.to_string_lossy().to_string())
}

#[cfg(windows)]
fn build_shortcut_script(
    lnk: &Path,
    exe_path: &Path,
    version: &str,
    icon: Option<&Path>,
) -> String {
    let work = exe_path.parent().unwrap_or(Path::new("."));
    // 快捷方式的 AUMID 用「dsh 窗口」的（而非管理器的）：
    //   - 快捷方式的目标虽同为管理器 exe，但语义是"启动某个 dsh 版本"
    //   - 若用管理器的 AUMID，任务栏会把"管理器 exe"与这些快捷方式视为同一应用，
    //     并取快捷方式的图标（鲸鱼）当作管理器的任务栏图标（管理器应为方块）
    //   - 用 dsh 的 AUMID 后：快捷方式归 dsh 组（鲸鱼），管理器归自己的组（方块），各归各位
    let aumid = crate::window::DSH_WINDOW_AUMID;
    // IconLocation = "<ico 路径>,0"（第 0 个图标）；无 ico 则省略，退回 exe 内嵌图标
    let icon_line = match icon {
        Some(p) => format!(
            "$lnk.IconLocation = '{}';",
            escape_ps(&format!("{},0", p.to_string_lossy()))
        ),
        None => String::new(),
    };
    // 注意：WScript.Shell 的快捷方式对象**没有** AppUserModelID 属性
    //（给它赋值会静默失败），必须改用 COM 的 IPropertyStore 写 PKEY_AppUserModel_ID。
    let lnk_ps = escape_ps(&lnk.to_string_lossy());
    let aumid_ps = escape_ps(aumid);
    format!(
        "$ws = New-Object -ComObject WScript.Shell; $lnk = $ws.CreateShortcut('{lnk}'); $lnk.TargetPath = '{exe}'; $lnk.Arguments = '--launch-version {ver}'; $lnk.WorkingDirectory = '{work}'; {icon}$lnk.Save(); {set_aumid}",
        lnk = lnk_ps,
        exe = escape_ps(&exe_path.to_string_lossy()),
        ver = escape_ps(version),
        work = escape_ps(&work.to_string_lossy()),
        icon = icon_line,
        set_aumid = build_set_aumid_ps(&lnk_ps, &aumid_ps),
    )
}

/// 生成一段 PowerShell：用 COM 的 IPropertyStore 给 .lnk 设 PKEY_AppUserModel_ID。
///
/// 为什么不能直接用 WScript.Shell：其快捷方式对象根本没有 AppUserModelID 属性，
/// 赋值会被静默忽略（快捷方式始终无 AUMID）。正确做法是经 IShellLink -> IPropertyStore。
#[cfg(windows)]
fn build_set_aumid_ps(lnk_ps: &str, aumid_ps: &str) -> String {
    // C# 内联：SHGetPropertyStoreFromParsingName 取 IPropertyStore，SetValue + Commit。
    // PKEY_AppUserModel_ID = {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, 5
    let cs = r#"
using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
public static class LnkAumid {
    [ComImport, Guid("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IPropertyStore {
        void GetCount(out uint c);
        void GetAt(uint i, out PROPERTYKEY k);
        void GetValue(ref PROPERTYKEY k, out PROPVARIANT v);
        void SetValue(ref PROPERTYKEY k, ref PROPVARIANT v);
        void Commit();
    }
    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    struct PROPERTYKEY { public Guid fmtid; public uint pid; }
    [StructLayout(LayoutKind.Explicit)]
    struct PROPVARIANT { [FieldOffset(0)] public ushort vt; [FieldOffset(8)] public IntPtr p; }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr pbc, uint flags, ref Guid riid, [MarshalAs(UnmanagedType.Interface)] out IPropertyStore store);
    [DllImport("ole32.dll")] static extern int PropVariantClear(ref PROPVARIANT pvar);
    public static void Set(string lnk, string aumid) {
        var iid = new Guid("886d8eeb-8cf2-4446-8d02-cdba1dbdcf99");
        IPropertyStore store;
        SHGetPropertyStoreFromParsingName(lnk, IntPtr.Zero, 2, ref iid, out store); // GPS_READWRITE = 2
        var key = new PROPERTYKEY { fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), pid = 5 };
        var pv = new PROPVARIANT();
        pv.vt = 31; // VT_LPWSTR
        pv.p = Marshal.StringToCoTaskMemUni(aumid);
        store.SetValue(ref key, ref pv);
        store.Commit();
        Marshal.FreeCoTaskMem(pv.p);
        Marshal.ReleaseComObject(store);
    }
}
"#;
    // 用 here-string 内联 C# 并调用；Add-Type 已存在同名类型时跳过
    format!(
        "if (-not ('LnkAumid' -as [type])) {{ Add-Type -TypeDefinition @'
{cs}
'@ }}; [LnkAumid]::Set('{lnk}', '{aumid}')",
        cs = cs,
        lnk = lnk_ps,
        aumid = aumid_ps,
    )
}

/// PowerShell 单引号字符串内转义（把 ' 变成 ''）
#[cfg(windows)]
fn escape_ps(s: &str) -> String {
    s.replace('\'', "''")
}

/// 桌面目录。
///
/// - Windows：优先读注册表 Shell Folders\Desktop（OneDrive/域策略/重定向都能覆盖），
///   读不到再回退常见路径猜测
/// - Unix：优先 `xdg-user-dir DESKTOP`，其次 `~/Desktop`、`~/桌面`
#[cfg(windows)]
fn desktop_dir() -> Option<PathBuf> {
    if let Some(dir) = desktop_from_registry() {
        if dir.exists() {
            return Some(dir);
        }
    }
    let user = std::env::var("USERPROFILE").ok()?;
    let plain = PathBuf::from(&user).join("Desktop");
    if plain.exists() {
        return Some(plain);
    }
    let onedrive = PathBuf::from(&user).join("OneDrive").join("Desktop");
    if onedrive.exists() {
        return Some(onedrive);
    }
    Some(plain)
}

/// 从注册表读取真实桌面路径：HKCU\...\Explorer\Shell Folders 的 Desktop 值。
/// 用 reg query 读取（零依赖）；失败返回 None。
#[cfg(windows)]
fn desktop_from_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let out = Command::new("reg")
        .args([
            "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders",
            "/v",
            "Desktop",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = crate::console_decode::decode_console(&out.stdout);
    // 输出形如：    Desktop    REG_SZ    D:\Users\ALING\Desktop
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Desktop") {
            let value = rest
                .trim_start()
                .strip_prefix("REG_SZ")
                .or_else(|| rest.trim_start().strip_prefix("REG_EXPAND_SZ"))?
                .trim();
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

/// Unix 桌面目录：优先 `xdg-user-dir DESKTOP`，其次 `~/Desktop`、`~/桌面`。
/// 注意：本分支在 Windows 上不参与编译，尚未经真机验证（见 GAP-005）。
#[cfg(unix)]
fn desktop_dir() -> Option<PathBuf> {
    // 1) XDG 标准：xdg-user-dir 会读 ~/.config/user-dirs.dirs
    if let Ok(out) = Command::new("xdg-user-dir").arg("DESKTOP").output() {
        if out.status.success() {
            let p = crate::console_decode::decode_console(&out.stdout)
                .trim()
                .to_string();
            if !p.is_empty() && Path::new(&p).exists() {
                return Some(PathBuf::from(p));
            }
        }
    }
    // 2) 回退常见路径（存在则直接用）
    let home = std::env::var("HOME").ok()?;
    for name in ["Desktop", "桌面"] {
        let p = PathBuf::from(&home).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    // 3) 无桌面环境（如 WSLg/服务器/容器）：返回 None，由调用方给出清晰错误，
    //    而不是返回一个不存在的路径导致 "No such file or directory (os error 2)"。
    None
}
