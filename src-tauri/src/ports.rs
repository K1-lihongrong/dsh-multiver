//! 外部依赖端口（v0.4.0-alpha.1）。
//!
//! 目标：把时间、文件系统、pnpm 进程、dsh 启动这四类副作用从业务逻辑里露出
//! 最小 trait 边界。生产 adapter 保持现有行为；测试可注入 fake，不再依赖真实
//! 网络、真实子进程或系统时钟。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// 时钟端口。
pub trait Clock: Send + Sync {
    fn now_secs(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// 文件系统端口（只包含迁移/清理需要的最小集合）。
pub trait FileSystem: Send + Sync {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn remove_file(&self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
}

pub struct SystemFs;

impl FileSystem for SystemFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// pnpm 命令端口。
pub trait PnpmRunner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<CommandOutput, String>;
}

pub struct ProcessPnpmRunner;

impl PnpmRunner for ProcessPnpmRunner {
    fn run(&self, args: &[String]) -> Result<CommandOutput, String> {
        let mut cmd = crate::actions::pnpm_command();
        cmd.args(args);
        let out = cmd.output().map_err(|e| e.to_string())?;
        Ok(CommandOutput {
            success: out.status.success(),
            stdout: crate::console_decode::decode_console(&out.stdout),
            stderr: crate::console_decode::decode_console(&out.stderr),
        })
    }
}

/// dsh 子进程启动请求。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
}

/// dsh 启动端口。生产 adapter 负责隐藏控制台并返回 Child；测试可替换为 fake。
pub trait DshLauncher: Send + Sync {
    #[allow(dead_code)]
    fn spawn(&self, request: &LaunchCommand) -> Result<Child, String>;

    /// 用已构造好的 Command spawn；供现有 launcher 保留自己的 env / Job Object 配置。
    fn spawn_command(&self, cmd: &mut Command) -> Result<Child, String> {
        cmd.spawn().map_err(|e| e.to_string())
    }
}

pub struct ProcessDshLauncher;

impl DshLauncher for ProcessDshLauncher {
    fn spawn(&self, request: &LaunchCommand) -> Result<Child, String> {
        let mut cmd = Command::new(&request.program);
        cmd.args(&request.args)
            .envs(request.envs.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &request.cwd {
            cmd.current_dir(cwd);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.spawn().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClock(u64);
    impl Clock for FakeClock {
        fn now_secs(&self) -> u64 {
            self.0
        }
    }

    #[test]
    fn fake_clock_is_used_without_system_time() {
        assert_eq!(FakeClock(42).now_secs(), 42);
        assert!(SystemClock.now_secs() > 1_600_000_000);
    }

    #[test]
    fn system_fs_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "dsh-ports-fs-{}-{}",
            std::process::id(),
            SystemClock.now_secs()
        ));
        let _ = std::fs::remove_dir_all(&root);
        SystemFs.create_dir_all(&root).unwrap();
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        SystemFs.write(&a, b"hello").unwrap();
        assert_eq!(SystemFs.read_to_string(&a).unwrap(), "hello");
        SystemFs.rename(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(SystemFs.read_to_string(&b).unwrap(), "hello");
        SystemFs.remove_file(&b).unwrap();
        SystemFs.remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dsh_launcher_spawns_prebuilt_command() {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd.exe");
            c.args(["/C", "exit", "0"]);
            c
        } else {
            Command::new("/bin/true")
        };
        let mut child = ProcessDshLauncher.spawn_command(&mut cmd).unwrap();
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn dsh_launcher_spawns_a_real_child() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe",
                vec!["/C".to_string(), "exit".to_string(), "0".to_string()],
            )
        } else {
            ("/bin/sh", vec!["-c".to_string(), "exit 0".to_string()])
        };
        let req = LaunchCommand {
            program: PathBuf::from(program),
            args,
            envs: Vec::new(),
            cwd: None,
        };
        let mut child = ProcessDshLauncher.spawn(&req).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
