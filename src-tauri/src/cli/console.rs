//! 控制台接入 + TTY 判断。

/// 把进程接到父控制台，使 release 版（`windows_subsystem = "windows"`）也能在
/// cmd / PowerShell 里直接看到 CLI 输出。
///
/// 规则：
/// - stdout 已重定向到管道 / 文件 → 保持原样（别抢走用户的 `> out.txt`）
/// - 否则尝试 `AttachConsole(父进程)`，成功则把 stdout/stderr 指到 `CONOUT$`
/// - 顺带把控制台输出代码页设为 UTF-8（65001），避免中文乱码
#[cfg(windows)]
pub fn setup_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileType, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_TYPE_DISK, FILE_TYPE_PIPE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetConsoleOutputCP, SetStdHandle, ATTACH_PARENT_PROCESS,
        STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };

    unsafe {
        // stdout 已被重定向到管道 / 文件 → 保持原样
        let existing = GetStdHandle(STD_OUTPUT_HANDLE);
        if !existing.is_null() && existing != INVALID_HANDLE_VALUE {
            let t = GetFileType(existing);
            if t == FILE_TYPE_PIPE || t == FILE_TYPE_DISK {
                return;
            }
        }
        // 附加到父进程控制台（在 cmd 里直接运行时生效）
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        // 打开 CONOUT$，把它设为进程的 stdout / stderr
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let h = CreateFileW(
            name.as_ptr(),
            FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, h);
            SetStdHandle(STD_ERROR_HANDLE, h);
            // 控制台默认代码页可能不是 UTF-8，中文会乱码
            SetConsoleOutputCP(65001);
        }
    }
}

#[cfg(not(windows))]
pub fn setup_console() {}

/// stderr 是否是真控制台（决定要不要画 `\r` 进度行）。
/// 重定向到文件/管道时返回 false → 静默，保持输出可管道。
#[cfg(windows)]
pub fn progress_is_tty() -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE};
    unsafe {
        let h = GetStdHandle(STD_ERROR_HANDLE);
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: u32 = 0;
        GetConsoleMode(h, &mut mode) != 0
    }
}

#[cfg(unix)]
pub fn progress_is_tty() -> bool {
    // isatty 判断 stderr 是否连着终端；重定向/管道时返回 false → 静默，保持可管道
    unsafe { libc::isatty(libc::STDERR_FILENO) == 1 }
}

#[cfg(not(any(windows, unix)))]
pub fn progress_is_tty() -> bool {
    false
}
