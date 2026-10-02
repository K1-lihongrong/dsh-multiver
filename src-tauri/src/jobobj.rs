//! 跨平台子进程组管理。
//!
//! 目标：管理器退出（正常 / 强杀 / 崩溃）时，其启动的 dsh（及其 node 子进程树）
//! **自动被清理**，不留孤儿进程。
//!
//! 平台实现：
//! - **Windows**：Job Object + `KILL_ON_JOB_CLOSE`。spawn **后** `attach` 把子进程加入 Job
//!   并持有句柄；句柄关闭（管理器退出）→ OS 结束组内进程。
//! - **Unix**：进程组 + `PDEATHSIG`（Linux）。spawn **前** `configure_command` 让子进程成为
//!   新进程组组长（pgid = pid）并设置父死信号；`kill_tree` 用 `killpg` 杀整组。
//!
//! 因时机不同，抽象接口分两阶段：`configure_command`（spawn 前）+ `attach`（spawn 后）。

use std::process::{Child, Command};

// ─────────────────────────── Windows ───────────────────────────

/// 一个 Job 的持有句柄。drop 时（或进程退出时）关闭 Job 句柄，
/// 若设置了 KILL_ON_JOB_CLOSE，则组内进程被 OS 结束。
#[cfg(windows)]
pub struct ProcessGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// 仅用于持有 + 关闭，不做并发读写，可跨线程传递。
#[cfg(windows)]
unsafe impl Send for ProcessGuard {}
#[cfg(windows)]
unsafe impl Sync for ProcessGuard {}

#[cfg(windows)]
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// spawn 前配置命令。Windows 无需配置（Job 在 spawn 后 attach）。
#[cfg(windows)]
pub fn configure_command(_cmd: &mut Command) {}

/// spawn 后把子进程加入新 Job。
/// 失败返回 None（不影响进程本身继续运行，只是失去"退出即清理"的保障）。
#[cfg(windows)]
pub fn attach(child: &Child) -> Option<ProcessGuard> {
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

        Some(ProcessGuard { handle: job })
    }
}

// ─────────────────────────── Unix ───────────────────────────

/// Unix 下进程组已在 spawn 前设定，此句柄为空占位。
#[cfg(unix)]
pub struct ProcessGuard;

/// spawn 前配置命令：
/// - 让子进程成为**新进程组组长**（pgid = 子进程 pid），便于 `killpg` 杀整组
/// - Linux 上设置 `PR_SET_PDEATHSIG=SIGKILL`：父（线程）死亡时子进程被内核杀，
///   兜底"管理器被强杀、来不及跑清理代码"的情况
#[cfg(unix)]
pub fn configure_command(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);

    #[cfg(target_os = "linux")]
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
}

/// spawn 后：进程组已由 `configure_command` 设定，无需额外操作。
#[cfg(unix)]
pub fn attach(_child: &Child) -> Option<ProcessGuard> {
    Some(ProcessGuard)
}

// ─────────────────────── 其他平台（占位） ───────────────────────

#[cfg(not(any(windows, unix)))]
pub struct ProcessGuard;

#[cfg(not(any(windows, unix)))]
pub fn configure_command(_cmd: &mut Command) {}

#[cfg(not(any(windows, unix)))]
pub fn attach(_child: &Child) -> Option<ProcessGuard> {
    None
}
