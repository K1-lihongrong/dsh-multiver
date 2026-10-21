//! 数据隔离（D5 拆分：原 versions.rs 的 isolated 相关）。
use std::path::Path;

/// 隔离 home 的路径
pub fn isolated_home(versions_dir: &Path, version: &str) -> std::path::PathBuf {
    versions_dir.join(version).join("home")
}

/// 复制共享 home 到隔离 home（不覆盖已存在文件）
pub fn copy_shared_to_isolated(shared_home: &Path, isolated: &Path) -> (bool, String, u64) {
    let mut copied = 0u64;
    if let Err(e) = copy_dir_recursive(shared_home, isolated, &mut copied) {
        return (false, format!("复制失败: {}", e), copied);
    }
    (true, format!("已复制 {} 个文件", copied), copied)
}

fn copy_dir_recursive(src: &Path, dst: &Path, count: &mut u64) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst.join(&name);

        // 目标已存在则跳过（不覆盖）
        if dst_path.exists() {
            continue;
        }

        // 关键：保留 junction / symlink，而不是实体化。
        // 共享 home 的 node_modules 多为 JUNCTION（目录联接），朴素复制会把它
        // 实体化成普通目录，破坏 dsh 的模块代理机制（报 "exists and is not a
        // symlink or dsh-managed module proxy"）。这里在目标处**重建一个指向
        // 相同目标的 junction**，从而"复制共享数据"的同时保持模块可解析。
        if is_reparse_point(&src_path) {
            if let Some(target) = read_link_target(&src_path) {
                if create_junction(&dst_path, &target).is_ok() {
                    *count += 1;
                    continue;
                }
                // 建 junction 失败则退回普通处理（下面按目录/文件走）
            }
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, count)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
            *count += 1;
        }
    }
    Ok(())
}

/// 判断路径是否为 reparse point（junction 或 symlink）。
fn is_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        match std::fs::symlink_metadata(path) {
            Ok(md) => md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }
}

/// 读取 junction / symlink 指向的目标路径。
fn read_link_target(path: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_link(path).ok()
}

/// 在 Windows 上创建目录 junction（mklink /J），无需管理员权限。
/// 非 Windows 上退回 symlink_dir。
fn create_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // 分开传参给 cmd：cmd /C mklink /J <link> <target>。
        // 不要拼成单个字符串再 .arg()，否则 Rust 会二次转义导致路径解析失败。
        let out = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                crate::console_decode::decode_console(&out.stderr),
            ))
        }
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link)
    }
}

/// 清理隔离 home（删除目录内容但保留目录本身）
pub fn clear_isolated(versions_dir: &Path, version: &str) -> (bool, String) {
    let home = isolated_home(versions_dir, version);
    if !home.exists() {
        return (true, "隔离目录不存在，无需清理".to_string());
    }
    match std::fs::remove_dir_all(&home) {
        Ok(_) => {
            let _ = std::fs::create_dir_all(&home);
            (true, format!("已清理 {} 的隔离数据", version))
        }
        Err(e) => {
            let hint = if e.raw_os_error() == Some(32) {
                "\n\n可能该版本的 dsh 进程正在运行，持有数据文件。\n请先关闭对应的 dsh 窗口/终端，再重试。"
            } else {
                ""
            };
            (false, format!("清理失败: {}{}", e, hint))
        }
    }
}
