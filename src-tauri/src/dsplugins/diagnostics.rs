//! 诊断/执行层：dsh CLI 定位、ANSI 清理与插件命令流式执行。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// dsh CLI 路径（`<版本目录>/node_modules/.bin/dsh[.cmd]`）。
pub fn dsh_bin(version_dir: &Path) -> Option<PathBuf> {
    let bin = version_dir
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "dsh.cmd" } else { "dsh" });
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

/// dsh CLI 入口：优先用 `node <dsh>/lib/bin.js` 直接执行（避免 `cmd /C` 的参数转义，
/// 从而安全支持 GitHub 地址 / 本地目录等安装 spec）。
#[derive(Debug, Clone)]
pub struct DshCli {
    pub bin: PathBuf,
    pub js: Option<PathBuf>,
}

pub fn dsh_cli(version_dir: &Path) -> Option<DshCli> {
    let bin = dsh_bin(version_dir)?;
    let js = version_dir
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("lib")
        .join("bin.js");
    Some(DshCli {
        bin,
        js: js.exists().then_some(js),
    })
}

/// 剥离 ANSI 转义序列（`ESC [ … 终止字节`），让日志/toast 不出现彩色控制码。
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next(); // '['
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 一次插件命令的执行结果。
#[derive(Debug)]
pub struct RunResult {
    pub ok: bool,
    pub output: String,
    pub timed_out: bool,
}

/// 读子进程输出并按行转发（解码 UTF-8/GBK + 剥 ANSI）。
fn pump_lines<R: std::io::Read + Send + 'static>(
    reader: R,
    tx: std::sync::mpsc::Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut br = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match br.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let decoded = crate::console_decode::decode_console(&buf);
                    let line = strip_ansi(decoded.trim_end_matches(['\r', '\n']));
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// 调 dsh CLI 执行插件操作（`dsh plugin --profile <p> <args...>`）。
///
/// - 输出按行流式回调（供前端进度展示），同时累积到返回值 `output`；
/// - 超过 `timeout` 杀掉子进程并返回 `timed_out = true`；
/// - 输出统一解码（D16）并剥离 ANSI。
pub fn run_dsh_plugin<F: FnMut(&str)>(
    dsh_bin: &Path,
    dsh_js: Option<&Path>,
    dsh_home: &Path,
    profile: &str,
    args: &[&str],
    timeout: Duration,
    mut on_line: F,
) -> RunResult {
    let mut cmd = if let Some(js) = dsh_js {
        // 直接 node 执行：避免 cmd /C 二次转义，支持 URL / 本地路径 spec
        let mut c = std::process::Command::new("node");
        c.arg(js);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(dsh_bin);
        c
    } else {
        std::process::Command::new(dsh_bin)
    };
    cmd.arg("plugin")
        .arg("--profile")
        .arg(profile)
        .args(args)
        .env("DSH_HOME", dsh_home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return RunResult {
                ok: false,
                output: format!("执行 dsh 失败：{e}"),
                timed_out: false,
            }
        }
    };

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(pump_lines(out, tx.clone()));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(pump_lines(err, tx.clone()));
    }
    drop(tx);

    let deadline = Instant::now() + timeout;
    let mut lines: Vec<String> = Vec::new();
    let mut ok = false;
    let mut timed_out = false;
    loop {
        while let Ok(line) = rx.try_recv() {
            on_line(&line);
            lines.push(line);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                ok = status.success();
                break;
            }
            Ok(None) => {}
            Err(e) => {
                lines.push(format!("等待 dsh 失败：{e}"));
                break;
            }
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for h in readers {
        let _ = h.join();
    }
    while let Ok(line) = rx.try_recv() {
        on_line(&line);
        lines.push(line);
    }
    RunResult {
        ok,
        output: lines.join("\n"),
        timed_out,
    }
}
