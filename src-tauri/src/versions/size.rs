//! 占用统计（D5 拆分：原 versions.rs 的 size 相关）。
//!
//! 两段式扫描：
//! 1. `collect_files` 遍历目录收集文件列表 + 总量（带进度/取消）
//! 2. `detail_from_files` 逐文件读 nlink 判定共享（带进度/取消）
//!
//! 说明：P7「思路 G」曾试图把 metadata + open 合并为一次系统调用，实测无收益
//! （release 39.13s vs 38.16s），已回退，详见 docs/开发缺口.md。
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 递归统计目录大小（字节）。目录不存在返回 0。
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// 版本占用的详细信息：逻辑总量 + 与 pnpm store 共享（硬链接）的部分。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeInfo {
    /// 逻辑总大小（所有文件按自身大小累加，硬链接会重复计）
    pub total: u64,
    /// 与其他版本共享（nlink > 1）的文件大小之和
    pub shared_size: u64,
    /// 共享文件的数量
    pub shared_count: u64,
    /// 独占大小 = total - shared_size
    pub exclusive_size: u64,
}

/// 读取文件的硬链接数（nlink）。读不到时保守返回 1（视为不共享）。
#[cfg(windows)]
fn file_nlink(path: &Path) -> u32 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let handle = f.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    if ok != 0 && info.nNumberOfLinks > 0 {
        info.nNumberOfLinks
    } else {
        1
    }
}

#[cfg(unix)]
fn file_nlink(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .map(|m| m.nlink() as u32)
        .unwrap_or(1)
}

#[cfg(not(any(windows, unix)))]
fn file_nlink(_path: &Path) -> u32 {
    1
}

/// 进度回调：参数为（已完成文件数, 总文件数）。每 200 个文件回调一次。
pub type ScanProgress = dyn Fn(u64, u64) + Send + Sync;

/// 取消检查：返回 true 表示已请求取消，应尽快中止。
pub type CancelCheck = dyn Fn() -> bool + Send + Sync;

/// 第一段遍历的产物：文件路径列表 + 总字节数 + 文件数。
pub struct FileList {
    pub files: Vec<std::path::PathBuf>,
    pub total: u64,
}

/// 第一段（合并遍历）：一次 read_dir 递归，收集所有文件路径 + 总量 + 文件数。
/// 带进度回调（每 PROGRESS_EVERY 个文件）与取消检查（每层递归）。
/// 返回 None 表示被取消。
pub fn collect_files(
    path: &Path,
    on_progress: Option<&ScanProgress>,
    cancel: Option<&CancelCheck>,
) -> Option<FileList> {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut done = 0u64;
    let mut cancelled = false;
    collect_rec(
        path,
        &mut files,
        &mut total,
        &mut done,
        on_progress,
        cancel,
        &mut cancelled,
    );
    if cancelled {
        return None;
    }
    Some(FileList { files, total })
}

#[allow(clippy::too_many_arguments)]
fn collect_rec(
    path: &Path,
    files: &mut Vec<std::path::PathBuf>,
    total: &mut u64,
    done: &mut u64,
    on_progress: Option<&ScanProgress>,
    cancel: Option<&CancelCheck>,
    cancelled: &mut bool,
) {
    if *cancelled {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if let Some(c) = cancel {
            if c() {
                *cancelled = true;
                return;
            }
        }
        let p = entry.path();
        if p.is_dir() {
            collect_rec(&p, files, total, done, on_progress, cancel, cancelled);
        } else if let Ok(meta) = entry.metadata() {
            *total += meta.len();
            files.push(p);
            *done += 1;
            if let Some(cb) = on_progress {
                if (*done).is_multiple_of(PROGRESS_EVERY) {
                    cb(*done, 0);
                }
            }
        }
    }
}

/// 第二段：用第一段收集的文件列表逐文件读 nlink，返回完整占用详情。
/// 带进度回调与取消检查。返回 None 表示被取消。
pub fn detail_from_files(
    files: &[std::path::PathBuf],
    total: u64,
    on_progress: Option<&ScanProgress>,
    cancel: Option<&CancelCheck>,
) -> Option<SizeInfo> {
    let mut info = SizeInfo {
        total,
        shared_size: 0,
        shared_count: 0,
        exclusive_size: 0,
    };
    let file_total = files.len() as u64;
    for (i, p) in files.iter().enumerate() {
        if let Some(c) = cancel {
            if c() {
                return None;
            }
        }
        if let Ok(meta) = std::fs::metadata(p) {
            let len = meta.len();
            if file_nlink(p) > 1 {
                info.shared_size += len;
                info.shared_count += 1;
            }
        }
        let done = (i + 1) as u64;
        if let Some(cb) = on_progress {
            if done.is_multiple_of(PROGRESS_EVERY) {
                cb(done, file_total);
            }
        }
    }
    info.exclusive_size = info.total.saturating_sub(info.shared_size);
    Some(info)
}

/// 递归统计目录：总量 + 共享(硬链接)部分。用于"占用大小"展示复用情况。
fn dir_size_detail(path: &Path, acc: &mut SizeInfo) {
    dir_size_detail_with_progress(path, acc, None, &mut 0);
}

/// 递归统计目录（带进度回调）。on_progress 为 None 时不回调。
/// done 为跨文件累加的已完成数（含子目录）；每 PROGRESS_EVERY 个文件回调一次。
fn dir_size_detail_with_progress(
    path: &Path,
    acc: &mut SizeInfo,
    on_progress: Option<&ScanProgress>,
    done: &mut u64,
) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                dir_size_detail_with_progress(&p, acc, on_progress, done);
            } else if let Ok(meta) = entry.metadata() {
                let len = meta.len();
                acc.total += len;
                if file_nlink(&p) > 1 {
                    acc.shared_size += len;
                    acc.shared_count += 1;
                }
                *done += 1;
                if let Some(cb) = on_progress {
                    if (*done).is_multiple_of(PROGRESS_EVERY) {
                        cb(*done, 0);
                    }
                }
            }
        }
    }
}

/// 进度回调间隔（每多少个文件回调一次）。
const PROGRESS_EVERY: u64 = 200;

/// 整个版本目录的占用详情（含共享/独占）。用于"扫描占用"。
pub fn version_size_detail(versions_dir: &Path, version: &str) -> SizeInfo {
    let mut info = SizeInfo {
        total: 0,
        shared_size: 0,
        shared_count: 0,
        exclusive_size: 0,
    };
    dir_size_detail(&versions_dir.join(version), &mut info);
    info.exclusive_size = info.total.saturating_sub(info.shared_size);
    info
}
