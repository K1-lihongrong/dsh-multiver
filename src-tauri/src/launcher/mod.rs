//! dsh 启动与桌面集成（D14 技术债拆分，v0.3.0）。
//!
//! 原 `launcher.rs`（979 行）按职责拆分为 3 个子模块，经本模块 `pub use` 重导出，
//! **对外路径 `crate::launcher::X` 完全不变**（调用方零改动）。
//!
//! | 模块 | 内容 |
//! | :--- | :--- |
//! | `process` | spawn_web_hidden / spawn_web_console / kill_tree / port_in_use |
//! | `diagnose` | read_recent_errors / diagnose / latest_session_errors / parse_url |
//! | `shortcut` | create_desktop_shortcut / AUMID / 图标 / 桌面目录定位 |

mod diagnose;
mod process;
mod shortcut;

#[allow(unused_imports)]
pub(crate) use diagnose::{diagnose, latest_session_errors, read_recent_errors};
#[allow(unused_imports)]
pub use process::{kill_tree, port_in_use, spawn_web_console, spawn_web_hidden};
#[allow(unused_imports)]
pub use shortcut::create_desktop_shortcut;
#[allow(unused_imports)]
pub(crate) use shortcut::{materialize_dsh_icon, register_aumid_icon};
