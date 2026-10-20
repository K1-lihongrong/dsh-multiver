//! 环境检查命令。

use crate::cli::load_ctx;
use crate::cli::output::json_str;

/// `env`：环境检查。有致命项失败 → 退出码 1（便于脚本 if 判断）。
pub fn cmd_env(json: bool) -> i32 {
    let ctx = load_ctx();
    let items = crate::envcheck::run_all(&ctx.dirs.root, true);
    let fatal_fail = items.iter().any(|i| i.critical && !i.ok);
    if json {
        println!("{}", json_str(&items));
    } else {
        for it in &items {
            let mark = if it.ok { "✓" } else { "✗" };
            let crit = if it.critical { "" } else { "（非致命）" };
            println!("[{}] {}{}  {}", mark, it.name, crit, it.detail);
        }
    }
    if fatal_fail {
        1
    } else {
        0
    }
}
