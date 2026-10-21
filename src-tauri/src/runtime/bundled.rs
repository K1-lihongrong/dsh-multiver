//! 便携运行时（Node / pnpm）的检测、下载与解压（D12 拆分：原 runtime.rs 的便携运行时部分）。
#[cfg(windows)]
use super::download_to;
use super::RuntimeProgress;
use std::path::{Path, PathBuf};
/// 便携运行时依赖的 Node 版本（与 envcheck 的下限一致）。
#[cfg_attr(not(windows), allow(dead_code))]
pub const NODE_VERSION: &str = "22.19.0";

/// 便携 Node 目录：`<root>/tools/node`
pub fn node_dir(root: &Path) -> PathBuf {
    root.join("tools").join("node")
}

/// 便携 pnpm 目录：`<root>/tools/pnpm`
pub fn pnpm_dir(root: &Path) -> PathBuf {
    root.join("tools").join("pnpm")
}

/// Windows 下便携 Node 的可执行文件名；其余平台为 `node`。
#[cfg(windows)]
pub fn node_exe_name() -> &'static str {
    "node.exe"
}
#[cfg(not(windows))]
pub fn node_exe_name() -> &'static str {
    "node"
}

/// 便携 Node 是否已就绪（存在 node 可执行文件）。
pub fn bundled_node_ok(root: &Path) -> bool {
    node_dir(root).join(node_exe_name()).is_file()
}

/// 便携 pnpm 是否已就绪。
pub fn bundled_pnpm_ok(root: &Path) -> bool {
    let d = pnpm_dir(root);
    #[cfg(windows)]
    {
        d.join("pnpm.cmd").is_file() || d.join("pnpm.exe").is_file()
    }
    #[cfg(not(windows))]
    {
        d.join("pnpm").is_file()
    }
}

/// 运行时状态：当前环境缺什么。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RuntimeStatus {
    /// 系统 Node 是否满足 dsh 要求
    pub system_node_ok: bool,
    /// 便携 Node 是否已就绪
    pub bundled_node_ok: bool,
    /// 系统 pnpm 是否可用
    pub system_pnpm_ok: bool,
    /// 便携 pnpm 是否已就绪
    pub bundled_pnpm_ok: bool,
    /// 是否支持便携运行时自动下载（目前仅 Windows）
    pub portable_supported: bool,
}

/// 检测运行时应走系统还是便携：返回该注入 dsh 子进程 PATH 的额外目录列表。
///
/// 规则（与规划一致）：
/// 1. 系统 Node 满足 → 用系统的（不注入便携 node；便携存在仍保留作兜底）
/// 2. 系统 Node 不满足 → 注入便携 Node 目录（若有）
///
/// pnpm 同理。
pub fn runtime_path_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if !system_node_ok() && bundled_node_ok(root) {
        dirs.push(node_dir(root));
    }
    if !system_pnpm_ok() && bundled_pnpm_ok(root) {
        dirs.push(pnpm_dir(root));
    }
    dirs
}

/// 计算最终要注入子进程的环境变量列表（形如 [("PATH", "旧值;新目录")]）。
/// 无注入需求时返回 None。
pub fn path_env_additions(root: &Path) -> Option<String> {
    let dirs = runtime_path_dirs(root);
    if dirs.is_empty() {
        return None;
    }
    let cur = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = dirs
        .iter()
        .map(|d| d.to_string_lossy().to_string())
        .collect();
    if !cur.is_empty() {
        parts.push(cur);
    }
    Some(parts.join(if cfg!(windows) { ";" } else { ":" }))
}

// ───────────────────────── 系统运行时探测 ─────────────────────────

/// 系统 Node 是否满足 dsh 要求（复用 envcheck 的纯系统判断）。
pub fn system_node_ok() -> bool {
    crate::envcheck::system_node_ok()
}

/// 系统 pnpm 是否可用。
pub fn system_pnpm_ok() -> bool {
    crate::envcheck::system_pnpm_ok()
}

// ───────────────────────── 下载 ─────────────────────────

/// 下载 Node 官方 zip 到临时文件（返回临时文件路径）。
///
/// 源顺序：官方 → 淘宝镜像（npmmirror）。任一成功即返回。
/// 这是「失败换源」在运行时下载里的复用。
#[cfg(windows)]
pub fn download_node_zip(
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let file = format!("node-v{}-win-x64.zip", NODE_VERSION);
    let urls = [
        format!("https://nodejs.org/dist/v{}/{}", NODE_VERSION, file),
        format!(
            "https://npmmirror.com/mirrors/node/v{}/{}",
            NODE_VERSION, file
        ),
    ];
    let tmp = std::env::temp_dir().join(format!(
        "dsh-multiver-node-{}.zip",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut last_err = String::from("未尝试任何下载源");
    for url in &urls {
        match download_to(url, &tmp, "正在下载 Node", on_progress) {
            Ok(_) => return Ok(tmp),
            Err(e) => last_err = format!("{} → {}", url, e),
        }
    }
    Err(format!("下载 Node 失败：{}", last_err))
}

/// 解压 Node zip 到 `<root>/tools/node/`，把 zip 内的顶层目录（`node-vX-win-x64/`）内容
/// 平铺到 node 目录。
#[cfg(windows)]
pub fn extract_node_zip(zip_path: &Path, root: &Path) -> Result<(), String> {
    let target = node_dir(root);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // 跳过顶层目录名，只保留其下的相对路径
        let rel = match name.split_once('/') {
            Some((_, rest)) => rest,
            None => continue,
        };
        if rel.is_empty() {
            continue;
        }
        let out = target.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 完整流程：下载 + 解压便携 Node。返回 node 目录。
#[cfg(windows)]
pub fn install_bundled_node(
    root: &Path,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let zip = download_node_zip(on_progress)?;
    on_progress(RuntimeProgress {
        stage: "正在解压 Node".to_string(),
        done: 0,
        total: None,
    });
    let result = extract_node_zip(&zip, root);
    let _ = std::fs::remove_file(&zip);
    result?;
    Ok(node_dir(root))
}

// ───────────────────────── 便携 pnpm ─────────────────────────

/// 便携 pnpm 版本（@pnpm/exe 的稳定版）。
#[cfg_attr(not(windows), allow(dead_code))]
pub const PNPM_VERSION: &str = "12.11.2";

/// 下载 pnpm 独立版（@pnpm/exe.win32-x64 的 tgz）到临时文件。
///
/// 源顺序：官方 registry → npmmirror。任一成功即返回。
#[cfg(windows)]
pub fn download_pnpm_tgz(
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let rel = format!("@pnpm/exe.win32-x64/-/exe.win32-x64-{}.tgz", PNPM_VERSION);
    let urls = [
        format!("https://registry.npmjs.org/{}", rel),
        format!("https://registry.npmmirror.com/{}", rel),
    ];
    let tmp = std::env::temp_dir().join(format!(
        "dsh-multiver-pnpm-{}.tgz",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut last_err = String::from("未尝试任何下载源");
    for url in &urls {
        match download_to(url, &tmp, "正在下载 pnpm", on_progress) {
            Ok(_) => return Ok(tmp),
            Err(e) => last_err = format!("{} → {}", url, e),
        }
    }
    Err(format!("下载 pnpm 失败：{}", last_err))
}

/// 解压 pnpm tgz 到 `<root>/tools/pnpm/`，取出 `package/pnpm.exe`（及其余文件）。
#[cfg(windows)]
pub fn extract_pnpm_tgz(tgz_path: &Path, root: &Path) -> Result<(), String> {
    let target = pnpm_dir(root);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(tgz_path).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        // 取 package/ 下的相对路径
        let rel = match path.components().next() {
            Some(std::path::Component::Normal(first)) if first == "package" => {
                path.strip_prefix("package").unwrap_or(&path)
            }
            _ => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = target.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry.unpack(&out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 完整流程：下载 + 解压便携 pnpm。返回 pnpm 目录。
#[cfg(windows)]
pub fn install_bundled_pnpm(
    root: &Path,
    on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    let tgz = download_pnpm_tgz(on_progress)?;
    on_progress(RuntimeProgress {
        stage: "正在解压 pnpm".to_string(),
        done: 0,
        total: None,
    });
    let result = extract_pnpm_tgz(&tgz, root);
    let _ = std::fs::remove_file(&tgz);
    result?;
    Ok(pnpm_dir(root))
}

// ───────────────────────── 非 Windows：不支持便携运行时 ─────────────────────────

#[cfg(not(windows))]
pub fn install_bundled_node(
    _root: &Path,
    _on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    Err("便携运行时仅支持 Windows；请用系统包管理器安装 Node.js".to_string())
}

#[cfg(not(windows))]
pub fn install_bundled_pnpm(
    _root: &Path,
    _on_progress: &(dyn Fn(RuntimeProgress) + Send + Sync),
) -> Result<PathBuf, String> {
    Err("便携运行时仅支持 Windows；请用系统包管理器安装 pnpm".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dsh-runtime-{}-{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn paths_under_root() {
        let root = PathBuf::from("C:\\data");
        assert!(
            node_dir(&root).ends_with("tools\\node") || node_dir(&root).ends_with("tools/node")
        );
        assert!(
            pnpm_dir(&root).ends_with("tools\\pnpm") || pnpm_dir(&root).ends_with("tools/pnpm")
        );
    }

    #[test]
    fn bundled_node_ok_detects_exe() {
        let root = tmp_root("node-ok");
        assert!(!bundled_node_ok(&root));
        let dir = node_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(node_exe_name()), b"x").unwrap();
        assert!(bundled_node_ok(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_env_none_when_system_ok() {
        // 若本机系统 node/pnpm 都满足，则无需注入（测试环境相关，仅在满足时断言）
        if system_node_ok() && system_pnpm_ok() {
            assert!(path_env_additions(&PathBuf::from("C:\\nonexistent")).is_none());
        }
    }
}
