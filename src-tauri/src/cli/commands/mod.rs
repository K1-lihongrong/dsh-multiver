//! CLI 命令实现与分发。

mod r#default;
mod env;
mod misc;
mod store;
mod version;

use super::CliCommand;

/// 按命令分发到对应实现，返回进程退出码。
pub fn dispatch(cmd: CliCommand, dry_run: bool, json: bool) -> i32 {
    match cmd {
        CliCommand::Help => {
            super::help::print_help();
            0
        }
        CliCommand::Version => {
            if json {
                println!(
                    "{}",
                    super::output::json_str(&serde_json::json!({ "version": super::VERSION }))
                );
            } else {
                println!("dsh-multiver {}", super::VERSION);
            }
            0
        }
        // list / which 是只读命令，dry-run 对它们无意义（照常执行）
        CliCommand::List => version::cmd_list(json),
        CliCommand::Which => version::cmd_which(json),
        CliCommand::Install { version, registry } => {
            version::cmd_install(&version, registry.as_deref(), dry_run, json)
        }
        CliCommand::Uninstall { version } => version::cmd_uninstall(&version, dry_run, json),
        CliCommand::Info { version } => version::cmd_info(&version, json),
        CliCommand::Versions => version::cmd_versions(json),
        CliCommand::SetDefault { version } => r#default::cmd_set_default(&version, dry_run, json),
        CliCommand::Maintenance { kind } => store::cmd_maintenance(&kind, dry_run, json),
        CliCommand::Clean => store::cmd_clean(dry_run, json),
        CliCommand::Doctor => store::cmd_doctor(json),
        CliCommand::Env => env::cmd_env(json),
        CliCommand::Isolate { version, on } => misc::cmd_isolate(&version, on, dry_run, json),
        CliCommand::Shortcut { version } => misc::cmd_shortcut(&version, dry_run, json),
        CliCommand::CopyShared { version } => misc::cmd_copy_shared(&version, dry_run, json),
        CliCommand::ClearIsolated { version } => misc::cmd_clear_isolated(&version, dry_run, json),
        CliCommand::Export { file } => misc::cmd_export(file.as_deref(), json),
        CliCommand::Import { file } => misc::cmd_import(&file, dry_run, json),
        CliCommand::Run { version } => misc::cmd_run(&version, json),
        CliCommand::SetupRuntime => misc::cmd_setup_runtime(dry_run, json),
        CliCommand::Update => misc::cmd_update(dry_run, json),
    }
}
