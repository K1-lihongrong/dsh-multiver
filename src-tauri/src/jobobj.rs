//! 跨平台子进程组管理。
//!
//! 目标：管理器退出（正常 / 强杀 / 崩溃）时，其启动的 dsh（及其 node 子进程树）
//! **自动被清理**，不留孤儿进程。
//!
//! 平台实现：
//! - **Windows**：Job Object + `KILL_ON_JOB_CLOSE`。spawn **后** `attach` 把子进程加入 Job
//!   并持有句柄；句柄关闭（管理器退出）→ OS 结束组内进程。
//! - **Unix**：进程组。spawn **前** `configure_command` 让子进程成为新进程组组长
//!   （pgid = pid）；`kill_tree` 用 `killpg` 杀整组。
//!
//! 因时机不同，抽象接口分两阶段：`configure_command`（spawn 前）+ `attach`（spawn 后）。
//!
//! ## ⚠️ 为什么 Unix 侧【不】用 `PR_SET_PDEATHSIG`（GAP-008 的教训）
//!
//! `PDEATHSIG` 绑定的是**发起 `spawn()` 的那个线程**，不是进程。本项目的 spawn 发生在
//! `tauri::async_runtime::spawn_blocking` 的线程池线程上——命令返回后该线程被回收，
//! 内核随即按 `PDEATHSIG=SIGKILL` 把 dsh 子进程杀掉，表现为内嵌窗口"重新连接中"。
//!
//! 要让 `PDEATHSIG` 语义正确，必须把 `spawn()` 放在一个**生命周期与进程一致**的常驻线程上
//! （见 docs/开发缺口.md 的 GAP-010）。当前未实现，故**不使用** `PDEATHSIG`：
//! - 正常关窗 / 正常退出：`kill_tree`（killpg）负责清理
//! - 管理器被 `kill -9`：靠**下次启动时的残留清理**兜底（见 GAP-010）

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

/// spawn 前配置命令：让子进程成为**新进程组组长**（pgid = 子进程 pid），
/// 便于 `kill_tree` 用 `killpg` 杀整组。
///
/// 注意：这里**不设** `PR_SET_PDEATHSIG`——它绑的是 spawn 线程而非进程，
/// 会在池线程回收时误杀子进程（GAP-008）。详见模块头部注释。
#[cfg(unix)]
pub fn configure_command(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
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

// ─────────────────────── 测试（GAP-008 回归） ───────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    fn alive_after(child: &mut Child, ms: u64) -> bool {
        std::thread::sleep(Duration::from_millis(ms));
        matches!(child.try_wait(), Ok(None))
    }

    // 对照实验（GAP-008）：
    // - 实验组：configure_command（无 PDEATHSIG）→ 线程退出后子进程应存活
    // - 对照组：手写 PDEATHSIG → 线程退出后子进程应被 SIGKILL
    //
    // 证明「去掉 PDEATHSIG」修复了内嵌窗口"重新连接中"（spawn 池线程回收误杀 dsh）。
    #[cfg(unix)]
    #[test]
    fn gap008_pdeathsig_kills_child_on_thread_exit() {
        let mut ok_child = {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut cmd = Command::new("sleep");
                cmd.arg("5").stdout(Stdio::null()).stderr(Stdio::null());
                configure_command(&mut cmd);
                let child = cmd.spawn().expect("spawn sleep");
                let _ = tx.send(child);
            });
            rx.recv_timeout(Duration::from_secs(3)).expect("no child")
        };
        let ok_alive = alive_after(&mut ok_child, 800);
        let _ = ok_child.kill();
        let _ = ok_child.wait();

        let mut dead_child = {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut cmd = Command::new("sleep");
                cmd.arg("5").stdout(Stdio::null()).stderr(Stdio::null());
                unsafe {
                    use std::os::unix::process::CommandExt;
                    cmd.pre_exec(|| {
                        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                        Ok(())
                    });
                }
                let child = cmd.spawn().expect("spawn sleep");
                let _ = tx.send(child);
            });
            rx.recv_timeout(Duration::from_secs(3)).expect("no child")
        };
        let dead_alive = alive_after(&mut dead_child, 800);
        let _ = dead_child.kill();
        let _ = dead_child.wait();

        assert!(ok_alive, "experimental child should be alive (no PDEATHSIG)");
        assert!(!dead_alive, "control child should be killed by PDEATHSIG");
    }
}
