//! 参数解析（v0.3.0：`--flag` → 扁平子命令）。
//!
//! 语法：`dsh-multiver <子命令> [参数...] [修饰符...]`
//! 修饰符保持 `--`：`--json` / `--dry-run` / `--force` / `--registry <url>` / `--off`

use super::{CliCommand, InstanceAction, IsolateAction};

/// 从命令行参数解析出 CLI 命令。
///
/// - `Ok(None)`：无 CLI 子命令 → 启动 GUI
/// - `Ok(Some(cmd))`：执行该 CLI 命令
/// - `Err(msg)`：参数有误
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    // GUI 专用参数（内部）：出现即走 GUI 路径，不当作 CLI。
    // `--open-settings` 允许 `=值` 形式，故用前缀匹配而非全等。
    const GUI_FLAGS: [&str; 3] = ["--launch-version", "--open-settings", "--debug-progress"];
    if let Some(first) = args.first() {
        if GUI_FLAGS
            .iter()
            .any(|f| first == f || first.starts_with(&format!("{}=", f)))
        {
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
        "set-root" => CliCommand::SetRoot {
            dir: take_value(rest, "set-root")?,
        },
        "info" => CliCommand::Info {
            version: take_value(rest, "info")?,
        },
        "shortcut" => CliCommand::Shortcut {
            version: take_value(rest, "shortcut")?,
        },
        "copy-shared" => {
            let (version, instance) = parse_version_with_instance(rest, "copy-shared")?;
            CliCommand::CopyShared { version, instance }
        }
        "clear-isolated" => {
            let (version, instance) = parse_version_with_instance(rest, "clear-isolated")?;
            CliCommand::ClearIsolated { version, instance }
        }
        "run" => {
            let (version, instance) = parse_version_with_instance(rest, "run")?;
            CliCommand::Run { version, instance }
        }
        "import" => CliCommand::Import {
            file: take_value(rest, "import")?,
        },
        "isolate" => parse_isolate(rest, args[..idx].iter().any(|x| x == "--off"))?,
        "instance" => parse_instance(rest)?,
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

fn is_global_flag(a: &str) -> bool {
    matches!(a, "--json" | "--dry-run" | "--force")
}

fn next_value(rest: &[String], i: usize, key: &str) -> Result<String, String> {
    match rest.get(i + 1) {
        Some(v) if !v.starts_with("--") && !v.starts_with('-') => Ok(v.clone()),
        _ => Err(format!("{key} 缺少参数值")),
    }
}

/// 解析 `<版本> [--instance <id>]` 形式。
fn parse_version_with_instance(
    rest: &[String],
    cmd: &str,
) -> Result<(String, Option<String>), String> {
    let version = match rest.first() {
        Some(v) if !v.starts_with('-') => v.clone(),
        _ => return Err(format!("{cmd} 缺少版本号")),
    };
    let mut instance = None;
    let mut i = 1;
    while i < rest.len() {
        match rest[i].as_str() {
            "--instance" => {
                if instance.is_some() {
                    return Err("--instance 只能出现一次".into());
                }
                instance = Some(next_value(rest, i, "--instance")?);
                i += 2;
            }
            a if is_global_flag(a) => i += 1,
            a if a.starts_with('-') => return Err(format!("未知参数：{a}")),
            a => return Err(format!("{cmd} 多余参数：{a}")),
        }
    }
    Ok((version, instance))
}

fn set_isolate_action(slot: &mut Option<IsolateAction>, next: IsolateAction) -> Result<(), String> {
    if slot.is_some() {
        return Err("isolate 的 --off / --list / --attach / --detach 只能选一个".into());
    }
    *slot = Some(next);
    Ok(())
}

/// 解析 `isolate <版本> [--off|--list|--attach <id>|--detach <id>]`。
fn parse_isolate(rest: &[String], off_before: bool) -> Result<CliCommand, String> {
    let version = match rest.first() {
        Some(v) if !v.starts_with('-') => v.clone(),
        _ => return Err("isolate 缺少版本号".into()),
    };
    let mut action = None;
    if off_before {
        action = Some(IsolateAction::Off);
    }
    let mut i = 1;
    while i < rest.len() {
        match rest[i].as_str() {
            "--off" => {
                set_isolate_action(&mut action, IsolateAction::Off)?;
                i += 1;
            }
            "--list" => {
                set_isolate_action(&mut action, IsolateAction::List)?;
                i += 1;
            }
            "--attach" => {
                let id = next_value(rest, i, "--attach")?;
                set_isolate_action(&mut action, IsolateAction::Attach { id })?;
                i += 2;
            }
            "--detach" => {
                let id = next_value(rest, i, "--detach")?;
                set_isolate_action(&mut action, IsolateAction::Detach { id })?;
                i += 2;
            }
            a if is_global_flag(a) => i += 1,
            a if a.starts_with('-') => return Err(format!("未知参数：{a}")),
            a => return Err(format!("isolate 多余参数：{a}")),
        }
    }
    Ok(CliCommand::Isolate {
        version,
        action: action.unwrap_or(IsolateAction::Ensure),
    })
}

/// 解析 `instance create|list|rename|remove`。
fn parse_instance(rest: &[String]) -> Result<CliCommand, String> {
    let Some(sub) = rest.first() else {
        return Err("instance 缺少子命令（create / list / rename / remove）".into());
    };
    let action = match sub.as_str() {
        "create" => {
            let mut name = None;
            let mut attaches = Vec::new();
            let mut i = 1;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--name" => {
                        if name.is_some() {
                            return Err("--name 只能出现一次".into());
                        }
                        name = Some(next_value(rest, i, "--name")?);
                        i += 2;
                    }
                    "--attach" => {
                        attaches.push(next_value(rest, i, "--attach")?);
                        i += 2;
                    }
                    a if is_global_flag(a) => i += 1,
                    a if a.starts_with('-') => return Err(format!("未知参数：{a}")),
                    a => return Err(format!("instance create 不接受位置参数：{a}")),
                }
            }
            let name = name.ok_or_else(|| "instance create 缺少 --name".to_string())?;
            InstanceAction::Create { name, attaches }
        }
        "list" => {
            let mut sizes = false;
            for a in &rest[1..] {
                match a.as_str() {
                    "--sizes" => {
                        if sizes {
                            return Err("--sizes 只能出现一次".into());
                        }
                        sizes = true;
                    }
                    a if is_global_flag(a) => {}
                    a => return Err(format!("instance list 不接受参数：{a}")),
                }
            }
            InstanceAction::List { sizes }
        }
        "rename" => {
            let mut positional = Vec::new();
            for a in &rest[1..] {
                if is_global_flag(a) {
                    continue;
                }
                if a.starts_with('-') {
                    return Err(format!("未知参数：{a}"));
                }
                positional.push(a.clone());
            }
            if positional.len() != 2 {
                return Err("instance rename 需要 <id> <新名>".into());
            }
            InstanceAction::Rename {
                id: positional[0].clone(),
                name: positional[1].clone(),
            }
        }
        "remove" => {
            let mut positional = Vec::new();
            for a in &rest[1..] {
                if is_global_flag(a) {
                    continue;
                }
                if a.starts_with('-') {
                    return Err(format!("未知参数：{a}"));
                }
                positional.push(a.clone());
            }
            if positional.len() != 1 {
                return Err("instance remove 需要 <id>".into());
            }
            InstanceAction::Remove {
                id: positional[0].clone(),
            }
        }
        other => return Err(format!("未知 instance 子命令：{other}")),
    };
    Ok(CliCommand::Instance { action })
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

    /// `--open-settings` 支持独立参数与 `=值` 两种写法，都必须走 GUI。
    /// 曾因只做全等匹配，`--open-settings=paths` 被当作未知参数报错。
    #[test]
    fn open_settings_forms_pass_through() {
        assert!(matches!(parse(&args(&["--open-settings"])), Ok(None)));
        assert!(matches!(
            parse(&args(&["--open-settings", "paths"])),
            Ok(None)
        ));
        assert!(matches!(parse(&args(&["--open-settings=paths"])), Ok(None)));
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
    fn isolate_actions() {
        match parse(&args(&["isolate", "0.1.7"])) {
            Ok(Some(CliCommand::Isolate { version, action })) => {
                assert_eq!(version, "0.1.7");
                assert!(matches!(action, IsolateAction::Ensure));
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["isolate", "0.1.7", "--off"])) {
            Ok(Some(CliCommand::Isolate { action, .. })) => {
                assert!(matches!(action, IsolateAction::Off));
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["isolate", "0.1.7", "--list"])) {
            Ok(Some(CliCommand::Isolate { action, .. })) => {
                assert!(matches!(action, IsolateAction::List));
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["isolate", "0.1.7", "--attach", "i2"])) {
            Ok(Some(CliCommand::Isolate { action, .. })) => match action {
                IsolateAction::Attach { id } => assert_eq!(id, "i2"),
                other => panic!("unexpected action: {other:?}"),
            },
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["isolate", "0.1.7", "--detach", "i2"])) {
            Ok(Some(CliCommand::Isolate { action, .. })) => match action {
                IsolateAction::Detach { id } => assert_eq!(id, "i2"),
                other => panic!("unexpected action: {other:?}"),
            },
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["isolate", "0.1.7", "--off", "--list"])).is_err());
        assert!(parse(&args(&["isolate", "0.1.7", "--attach"])).is_err());
    }

    #[test]
    fn instance_commands() {
        match parse(&args(&[
            "instance", "create", "--name", "A", "--attach", "0.1.7", "--attach", "0.2.0",
        ])) {
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::Create { name, attaches },
            })) => {
                assert_eq!(name, "A");
                assert_eq!(attaches, vec!["0.1.7", "0.2.0"]);
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["instance", "create", "--name", "A"])) {
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::Create { attaches, .. },
            })) => assert!(attaches.is_empty()),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["instance", "create"])).is_err());
        assert!(parse(&args(&["instance", "create", "--name"])).is_err());
        assert!(parse(&args(&["instance", "create", "--name", "A", "--name", "B"])).is_err());
        assert!(parse(&args(&["instance", "create", "--name", "A", "--attach"])).is_err());
        assert!(parse(&args(&["instance", "create", "--name", "A", "extra"])).is_err());

        assert!(matches!(
            parse(&args(&["instance", "list"])),
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::List { sizes: false }
            }))
        ));
        assert!(matches!(
            parse(&args(&["instance", "list", "--sizes"])),
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::List { sizes: true }
            }))
        ));
        assert!(parse(&args(&["instance", "list", "--sizes", "--sizes"])).is_err());
        assert!(parse(&args(&["instance", "list", "--unknown"])).is_err());
        match parse(&args(&["instance", "rename", "i1", "B"])) {
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::Rename { id, name },
            })) => {
                assert_eq!(id, "i1");
                assert_eq!(name, "B");
            }
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["instance", "rename", "i1"])).is_err());
        match parse(&args(&["instance", "remove", "i1"])) {
            Ok(Some(CliCommand::Instance {
                action: InstanceAction::Remove { id },
            })) => assert_eq!(id, "i1"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(parse(&args(&["instance", "remove"])).is_err());
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
            Ok(Some(CliCommand::Run { version, instance })) => {
                assert_eq!(version, "0.1.7");
                assert!(instance.is_none());
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["run", "0.1.7", "--instance", "i2"])) {
            Ok(Some(CliCommand::Run { version, instance })) => {
                assert_eq!(version, "0.1.7");
                assert_eq!(instance.as_deref(), Some("i2"));
            }
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
            Ok(Some(CliCommand::CopyShared { version, instance })) => {
                assert_eq!(version, "0.1.7");
                assert!(instance.is_none());
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["copy-shared", "0.1.7", "--instance", "i2"])) {
            Ok(Some(CliCommand::CopyShared { instance, .. })) => {
                assert_eq!(instance.as_deref(), Some("i2"))
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["clear-isolated", "0.1.7"])) {
            Ok(Some(CliCommand::ClearIsolated { version, instance })) => {
                assert_eq!(version, "0.1.7");
                assert!(instance.is_none());
            }
            other => panic!("unexpected: {:?}", other),
        }
        match parse(&args(&["clear-isolated", "0.1.7", "--instance", "i2"])) {
            Ok(Some(CliCommand::ClearIsolated { instance, .. })) => {
                assert_eq!(instance.as_deref(), Some("i2"))
            }
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

    #[test]
    fn set_root_command() {
        match parse(&args(&["set-root", "D:/dsh-data"])) {
            Ok(Some(CliCommand::SetRoot { dir })) => assert_eq!(dir, "D:/dsh-data"),
            other => panic!("unexpected: {:?}", other),
        }
        // 缺少目录参数 → 报错
        assert!(parse(&args(&["set-root"])).is_err());
        // 修饰符在前也认得
        match parse(&args(&["--json", "set-root", "E:/x"])) {
            Ok(Some(CliCommand::SetRoot { dir })) => assert_eq!(dir, "E:/x"),
            other => panic!("unexpected: {:?}", other),
        }
    }
}
