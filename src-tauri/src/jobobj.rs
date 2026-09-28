//! Windows Job Object 封装：把 dsh 子进程加入一个 Job，并设置
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`。这样当管理器进程退出（无论正常还是
//! 被强杀/崩溃）时，OS 会**自动结束 Job 内的所有进程**，彻底根除孤儿进程。
//!
//! 关键：Job 句柄必须**持有到管理器退出**。一旦句柄被 drop（或进程退出导致
//! 句柄关闭），Job 关闭 → 组内进程被 OS 杀掉。因此这里用 `Arc<JobHandle>`
//! 让进程 map 持有它，直到对应的 dsh 进程被移除/窗口关闭。
//!
//! 非 Windows 平台提供空实现（本项目主要面向 Windows）。

use std::process::Child;

/// 一个 Job 的持有句柄。drop 时（或进程退出时）会关闭 Job 句柄，
/// 若设置了 KILL_ON_JOB_CLOSE，则组内进程被 OS 结束。
#[cfg(windows)]
pub struct JobHandle {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// Job 句柄可跨线程传递（仅用于持有 + 关闭，不做并发读写）。
#[cfg(windows)]
unsafe impl Send for JobHandle {}
#[cfg(windows)]
unsafe impl Sync for JobHandle {}

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// 为一个已 spawn 的子进程创建 Job、加入，并返回持有句柄。
///
/// 失败时返回 None（不影响进程本身继续运行，只是失去"退出即清理"的保障）。
#[cfg(windows)]
pub fn assign_to_new_job(child: &Child) -> Option<JobHandle> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        // 1. 创建匿名 Job
        let job: HANDLE = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return None;
        }

        // 2. 设置 KILL_ON_JOB_CLOSE
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            CloseHandle(job);
            return None;
        }

        // 3. 把子进程加入 Job
        let proc_handle = child.as_raw_handle() as HANDLE;
        let ok = AssignProcessToJobObject(job, proc_handle);
        if ok == 0 {
            CloseHandle(job);
            return None;
        }

        Some(JobHandle { handle: job })
    }
}

/// 非 Windows：空实现。
#[cfg(not(windows))]
pub struct JobHandle;

#[cfg(not(windows))]
pub fn assign_to_new_job(_child: &Child) -> Option<JobHandle> {
    None
}
