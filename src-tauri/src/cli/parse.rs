//! 参数解析（v0.3.0：`--flag` → 扁平子命令）。
//!
//! 语法：`dsh-multiver <子命令> [参数...] [修饰符...]`
//! 修饰符保持 `--`：`--json` / `--dry-run` / `--force` / `--registry <url>` / `--off`

use super::CliCommand;

/// 从命令行参数解析出 CLI 命令。
///
/// - `Ok(None)`：无 CLI 子命令 → 启动 GUI
/// - `Ok(Some(cmd))`：执行该 CLI 命令
/// - `Err(msg)`：参数有误
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    // GUI 专用参数（内部）：出现即走 GUI 路径，不当作 CLI。
    const GUI_FLAGS: [&str; 2] = ["--launch-version", "--debug-progress"];
    if let Some(first) = args.first() {
        if GUI_FLAGS.contains(&first.as_str()) {
            return Ok(None);
        }
    }

    // 兼容传统用法：--help/-h、--version/-V（v0.3.0 子命令重构后补回，避免报错提示自相矛盾）
    if let Some(first) = args.first() {
        match first.as_str() {
            "--help" | "-h" => return Ok(Some(CliCommand::Help)),
            "--version" | "-V" => return Ok(Some(CliCommand::Version)),
            _ => {}
        }
    }

    // 找第一个非修饰符（子命令）。
    // 修饰符：--json / --dry-run / --force（布尔）；--registry / --off（--off 是布尔）
    let mut cmd_index: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--json" | "--dry-run" | "--force" | "--off" => {
                i += 1;
            }
            "--registry" => {
                // 跳过它的值
                i += 2;
            }
            _ if a.starts_with("--") || a.starts_with('-') => {
                return Err(format!("未知参数：{}", a));
            }
            _ => {
                cmd_index = Some(i);
                break;
            }
        }
    }

    let Some(idx) = cmd_index else {
        // 没有子命令
        if has_dry_run(args) {
            return Err("--dry-run 需要搭配一个命令（如 install <版本> --dry-run）".to_string());
        }
        if has_json(args) {
            return Err("--json 需要搭配一个命令（如 list --json）".to_string());
        }
        // 无 CLI 参数 → GUI（只认 --launch-version / --debug-progress，已在上面处理）
        return Ok(None);
    };

    let name = args[idx].as_str();
    let rest = &args[idx + 1..];
    let cmd = match name {
        "help" => CliCommand::Help,
        "version" => CliCommand::Version,
        "list" => CliCommand::List,
        "which" => CliCommand::Which,
        "versions" => CliCommand::Versions,
        "env" => CliCommand::Env,
        "clean" => CliCommand::Clean,
        "doctor" => CliCommand::Doctor,
        "setup-runtime" => CliCommand::SetupRuntime,
        "update" => CliCommand::Update,
        "install" => {
            let version = take_value(rest, "install")?;
            let registry = find_opt(args, "--registry");
            CliCommand::Install { version, registry }
        }
        "uninstall" => CliCommand::Uninstall {
            version: take_value(rest, "uninstall")?,
        },
        "set-default" => CliCommand::SetDefault {
            version: take_value(rest, "set-default")?,
        },
        "info" => CliCommand::Info {
            version: take_value(rest, "info")?,
        },
        "shortcut" => CliCommand::Shortcut {
            version: take_value(rest, "shortcut")?,
        },
        "copy-shared" => CliCommand::CopyShared {
            version: take_value(rest, "copy-shared")?,
        },
        "clear-isolated" => CliCommand::ClearIsolated {
            version: take_value(rest, "clear-isolated")?,
        },
        "run" => CliCommand::Run {
            version: take_value(rest, "run")?,
        },
        "import" => CliCommand::Import {
            file: take_value(rest, "import")?,
        },
        "isolate" => {
            let version = take_value(rest, "isolate")?;
            let on = !args.iter().any(|x| x == "--off");
            CliCommand::Isolate { version, on }
        }
        "export" => {
            // 可选跟随一个非 -- 开头的值作为输出文件
            let file = rest
                .first()
                .filter(|v| !v.starts_with("--") && !v.starts_with('-'))
                .cloned();
            CliCommand::Export { file }
        }
        "maintenance" => {
            let kind = if args.iter().any(|x| x == "--cleanup") {
                "cleanup"
            } else if args.iter().any(|x| x == "--prune") {
                "prune"
            } else {
                "all"
            };
            CliCommand::Maintenance {
                kind: kind.to_string(),
            }
        }
        other => return Err(format!("未知命令：{}（用 --help 查看用法）", other)),
    };
    Ok(Some(cmd))
}

/// 取子命令后的第一个位置参数（不能是 `--` 开头的修饰符）。
fn take_value(rest: &[String], cmd: &str) -> Result<String, String> {
    match rest.first() {
        Some(v) if !v.starts_with("--") && !v.starts_with('-') => Ok(v.clone()),
        _ => Err(format!("{} 缺少参数值", cmd)),
    }
}

/// 在整个参数表里找 `key <value>`，返回 value（与 key 的先后位置无关）
fn find_opt(args: &[String], key: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == key {
            return it.next().cloned();
        }
    }
    None
}

/// 是否含 `--dry-run`（只预览不执行）。仅对破坏性命令有意义。
pub fn has_dry_run(args: &[String]) -> bool {
    args.iter().any(|a| a == "--dry-run")
}

/// 是否含 `--json`（结构化输出）。只影响输出格式，不改行为。
pub fn has_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

/// 是否带 `--force`（修饰符）。
pub fn has_force() -> bool {
    std::env::args().any(|a| a == "--force")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_returns_none() {
        assert!(matches!(parse(&args(&[])), Ok(None)));
    }

    #[test]
    fn gui_flags_pass_through() {
        assert!(matches!(
            parse(&args(&["--launch-version", "0.1.7"])),
            Ok(None)
        ));
        assert!(matches!(parse(&args(&["--debug-progress"])), Ok(None)));
    }

    #[test]
    fn unknown_is_error() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["bogus-cmd"])).is_err());
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(
            parse(&args(&["help"])),
            Ok(Some(CliCommand::Help))
        ));
        assert!(matches!(
            parse(&args(&["version"])),
            Ok(Some(CliCommand::Version))
        ));
    }

    #[test]
    fn simple_commands() {
        assert!(matches!(
            parse(&args(&["list"])),
            Ok(Some(CliCommand::List))
        ));
        assert!(matches!(
            parse(&args(&["which"])),
            Ok(Some(CliCommand::Which))
        ));
        assert!(matches!(
            parse(&args(&["versions"])),
            Ok(Some(CliCommand::Versions))
        ));
        assert!(matches!(parse(&args(&["env"])), Ok(Some(CliCommand::Env))));
        assert!(matches!(
            parse(&args(&["clean"])),
            Ok(Some(CliCommand::Clean))
        ));
        assert!(matches!(
            parse(&args(&["doctor"])),
            Ok(Some(CliCommand::Doctor))
        ));
        assert!(matches!(
            parse(&args(&["setup-runtime"])),
            Ok(Some(CliCommand::SetupRuntime))
        ));
        assert!(matches!(
            parse(&args(&["update"])),
            Ok(Some(CliCommand::Update))
        ));
    }

    #[test]
    fn commands_with_value() {
        match parse(&args(&["install", "0.1.7"])) {
            Ok(Some(CliCommand::Install { version, registry })) => {
                assert_eq!(version, "0.1.7");
                assert!(registry.is_none());
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&[
            "install",
            "0.1.7",
            "--registry",
            "https://r.example",
        ])) {
            Ok(Some(CliCommand::Install { version, registry })) => {
                assert_eq!(version, "0.1.7");
                assert_eq!(registry.as_deref(), Some("https://r.example"));
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["set-default", "0.1.6"])) {
            Ok(Some(CliCommand::SetDefault { version })) => assert_eq!(version, "0.1.6"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["install"])).is_err());
    }

    #[test]
    fn isolate_off() {
        match parse(&args(&["isolate", "0.1.7"])) {
            Ok(Some(CliCommand::Isolate { version, on })) => {
                assert_eq!(version, "0.1.7");
                assert!(on);
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["isolate", "0.1.7", "--off"])) {
            Ok(Some(CliCommand::Isolate { version, on })) => {
                assert_eq!(version, "0.1.7");
                assert!(!on);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn export_optional_file() {
        assert!(matches!(
            parse(&args(&["export"])),
            Ok(Some(CliCommand::Export { file: None }))
        ));
        match parse(&args(&["export", "out.json"])) {
            Ok(Some(CliCommand::Export { file })) => assert_eq!(file.as_deref(), Some("out.json")),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn maintenance_kinds() {
        match parse(&args(&["maintenance"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "all"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["maintenance", "--cleanup"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "cleanup"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["maintenance", "--prune"])) {
            Ok(Some(CliCommand::Maintenance { kind })) => assert_eq!(kind, "prune"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn modifiers_alone_are_error() {
        assert!(parse(&args(&["--json"])).is_err());
        assert!(parse(&args(&["--dry-run"])).is_err());
    }

    #[test]
    fn info_and_run_commands() {
        match parse(&args(&["info", "0.1.7"])) {
            Ok(Some(CliCommand::Info { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["info"])).is_err());
        match parse(&args(&["run", "0.1.7"])) {
            Ok(Some(CliCommand::Run { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["run"])).is_err());
    }

    #[test]
    fn shortcut_copy_clear_commands() {
        match parse(&args(&["shortcut", "0.1.7"])) {
            Ok(Some(CliCommand::Shortcut { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["copy-shared", "0.1.7"])) {
            Ok(Some(CliCommand::CopyShared { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["clear-isolated", "0.1.7"])) {
            Ok(Some(CliCommand::ClearIsolated { version })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["shortcut"])).is_err());
        assert!(parse(&args(&["copy-shared"])).is_err());
        assert!(parse(&args(&["clear-isolated"])).is_err());
    }

    #[test]
    fn import_command() {
        match parse(&args(&["import", "b.json"])) {
            Ok(Some(CliCommand::Import { file })) => assert_eq!(file, "b.json"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["import"])).is_err());
    }

    #[test]
    fn uninstall_and_value_required() {
        match parse(&args(&["uninstall", "0.1.6"])) {
            Ok(Some(CliCommand::Uninstall { version })) => assert_eq!(version, "0.1.6"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["uninstall"])).is_err());
        assert!(parse(&args(&["uninstall", "--json"])).is_err());
    }

    #[test]
    fn modifier_before_command() {
        // 修饰符在前也能正确找到子命令
        assert!(matches!(
            parse(&args(&["--json", "list"])),
            Ok(Some(CliCommand::List))
        ));
        match parse(&args(&["--dry-run", "install", "0.1.7"])) {
            Ok(Some(CliCommand::Install { version, .. })) => assert_eq!(version, "0.1.7"),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn legacy_help_version_flags() {
        for flag in ["--help", "-h"] {
            assert!(matches!(parse(&args(&[flag])), Ok(Some(CliCommand::Help))));
        }
        for flag in ["--version", "-V"] {
            assert!(matches!(
                parse(&args(&[flag])),
                Ok(Some(CliCommand::Version))
            ));
        }
    }
    #[test]
    fn modifiers_parse_as_flags() {
        assert!(has_json(&args(&["list", "--json"])));
        assert!(!has_json(&args(&["list"])));
        assert!(has_dry_run(&args(&["install", "0.1.7", "--dry-run"])));
    }
}
