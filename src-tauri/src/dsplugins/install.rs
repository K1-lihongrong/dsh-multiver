//! 安装、更新与回滚：输入校验与更新事务。

use serde::{Deserialize, Serialize};

/// 校验用户输入的 npm 包规格：`name` / `@scope/name`，可带 `@版本或标签`。
///
/// 拒绝空、空白、控制字符、以 `-` 开头（防止把 pnpm 选项当包名透传）与非法字符。
/// 允许的字符集与 npm 包名/常见 semver 范围一致（如 `pkg@^1.2.3`、`@scope/pkg@latest`）。
pub fn validate_package_spec(spec: &str) -> Result<(), String> {
    let s = spec.trim();
    if s.is_empty() {
        return Err("包名不能为空".into());
    }
    if s.len() > 214 {
        return Err("包名过长（最多 214 字符）".into());
    }
    if s.starts_with('-') {
        return Err("包名不能以 - 开头（禁止透传 pnpm 选项）".into());
    }
    if s.chars().any(|c| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, ';' | '&' | '|' | '`' | '$' | '"' | '\'' | '\\')
    }) {
        return Err("包名不能包含空白、控制字符或 shell 特殊字符".into());
    }

    // 拆出可选的 @版本/标签：scope 包名以 @ 开头，故用 rfind 且要求位置 > 0。
    let (body, version) = match s.rfind('@') {
        Some(i) if i > 0 => (&s[..i], Some(&s[i + 1..])),
        _ => (s, None),
    };
    if let Some(v) = version {
        if v.is_empty() {
            return Err("版本/标签不能为空（如 pkg@1.2.3）".into());
        }
        if !v.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '-' | '_' | '~' | '^' | '*' | '+' | '=' | '<' | '>')
        }) {
            return Err(format!("版本/标签含非法字符：{v}"));
        }
    }

    let (scope, name) = if let Some(rest) = body.strip_prefix('@') {
        match rest.split_once('/') {
            Some((sc, nm)) => (Some(sc), nm),
            None => return Err("scope 包名需形如 @scope/name".into()),
        }
    } else {
        (None, body)
    };
    let valid_part = |x: &str| {
        !x.is_empty()
            && x.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    if let Some(sc) = scope {
        if !valid_part(sc) {
            return Err("scope 含非法字符".into());
        }
    }
    if !valid_part(name) {
        return Err("包名含非法字符".into());
    }
    Ok(())
}

/// 安装 spec 校验：npm 包名 / GitHub 地址 / 本地目录路径（与官方安装对话框一致的三类输入）。
pub fn validate_install_spec(spec: &str) -> Result<(), String> {
    let s = spec.trim();
    if s.is_empty() {
        return Err("请输入插件包名、GitHub 地址或本地目录".into());
    }
    if s.len() > 2048 {
        return Err("插件标识过长".into());
    }
    if s.starts_with('-') {
        return Err("不能以 - 开头（禁止透传 pnpm 选项）".into());
    }
    if s.chars().any(|c| c.is_control()) {
        return Err("插件标识含控制字符".into());
    }
    if validate_package_spec(s).is_ok() {
        return Ok(());
    }
    let lower = s.to_ascii_lowercase();
    let is_url = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git+")
        || lower.starts_with("github:")
        || lower.starts_with("file:")
        || lower.starts_with("link:");
    let bytes = s.as_bytes();
    let is_path = lower.starts_with("./")
        || lower.starts_with("../")
        || lower.starts_with(".\\")
        || lower.starts_with("..\\")
        || lower.starts_with('/')
        || (bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_url && !is_path {
        return Err("请输入 npm 包名、GitHub 地址或本地目录".into());
    }
    if s.chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '`' | '$' | '"' | '\''))
    {
        return Err("路径/URL 含 shell 特殊字符".into());
    }
    Ok(())
}

/// 安装源校验（对齐官方规则：http(s)、无用户名/密码、无 `#` 片段）。
pub fn validate_registry_url(url: &str) -> Result<(), String> {
    let s = url.trim();
    let lower = s.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("安装源必须是 http(s) 地址".into());
    }
    if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("安装源不能含空白/控制字符".into());
    }
    if s.contains('#') {
        return Err("安装源不能包含 # 片段".into());
    }
    let after = s.split_once("://").map(|(_, r)| r).unwrap_or("");
    if let Some(at) = after.find('@') {
        let slash = after.find('/').unwrap_or(after.len());
        if at < slash {
            return Err("安装源不能包含用户名/密码".into());
        }
    }
    Ok(())
}

/// 生成失败回滚时要执行的 `add` 参数。
///
/// package.json 的依赖值可能是：
/// - semver / range：`^1.2.3` → 需要拼成 `pkg@^1.2.3`
/// - git / URL / 本地路径：`github:user/repo` / `file:../p` → 原样作为 add 参数
/// - 缺失：退回 `pkg@current_version`
pub fn rollback_add_spec(
    name: &str,
    original_dependency_spec: Option<&str>,
    current_version: &str,
) -> Option<String> {
    let declared = original_dependency_spec
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match declared {
        Some(s) => {
            let first = s.chars().next().unwrap_or_default();
            let semver_like =
                first.is_ascii_digit() || matches!(first, '^' | '~' | '*' | '>' | '<' | '=');
            if semver_like {
                Some(format!("{name}@{s}"))
            } else {
                Some(s.to_string())
            }
        }
        None if !current_version.trim().is_empty() => {
            Some(format!("{name}@{}", current_version.trim()))
        }
        None => None,
    }
}

/// 前端提交的单个更新目标。只信任包名与目标版本，原版本/原 spec 由后端现场读取。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct PluginUpdateTarget {
    pub name: String,
    pub latest: String,
}

/// 更新事务的计划（后端从当前 profile 现场生成）。
#[derive(Debug, Clone)]
pub struct PluginUpdatePlan {
    pub name: String,
    pub current: String,
    pub latest: String,
    pub original_spec: String,
}

/// 更新事务的阶段，供调用方生成进度事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginUpdateStep {
    Remove,
    Install,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginUpdateStatus {
    Updated,
    RolledBack,
    RollbackFailed,
    Failed,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PluginUpdateResult {
    pub name: String,
    pub current: String,
    pub latest: String,
    pub status: PluginUpdateStatus,
    pub message: Option<String>,
    pub rollback_error: Option<String>,
}

/// 执行单个插件的更新事务。
///
/// - remove 失败：未产生变更，返回 Failed；
/// - add 成功但 verify 失败：按原 spec 回滚；
/// - add 失败：按原 spec 回滚；
/// - 回滚失败：返回原错误与回滚错误，前端给人工恢复命令。
pub fn execute_plugin_update<F, V>(
    plan: &PluginUpdatePlan,
    mut run: F,
    mut verify: V,
) -> PluginUpdateResult
where
    F: FnMut(PluginUpdateStep, &[&str]) -> Result<String, String>,
    V: FnMut() -> Result<(), String>,
{
    let result =
        |status, message: Option<String>, rollback_error: Option<String>| PluginUpdateResult {
            name: plan.name.clone(),
            current: plan.current.clone(),
            latest: plan.latest.clone(),
            status,
            message,
            rollback_error,
        };

    if let Err(e) = run(PluginUpdateStep::Remove, &["remove", plan.name.as_str()]) {
        return result(
            PluginUpdateStatus::Failed,
            Some(format!("卸载旧版本失败：{e}")),
            None,
        );
    }

    let target = format!("{}@{}", plan.name, plan.latest);
    let install_err = match run(PluginUpdateStep::Install, &["add", target.as_str()]) {
        Ok(_) => match verify() {
            Ok(()) => return result(PluginUpdateStatus::Updated, None, None),
            Err(e) => format!("安装后校验失败：{e}"),
        },
        Err(e) => e,
    };

    match run(
        PluginUpdateStep::Rollback,
        &["add", plan.original_spec.as_str()],
    ) {
        Ok(_) => match verify() {
            Ok(()) => result(
                PluginUpdateStatus::RolledBack,
                Some(format!(
                    "更新失败，已回滚到 {}：{install_err}",
                    plan.current
                )),
                None,
            ),
            Err(e) => result(
                PluginUpdateStatus::RollbackFailed,
                Some(format!("更新失败：{install_err}；回滚后校验失败：{e}")),
                Some(e),
            ),
        },
        Err(e) => result(
            PluginUpdateStatus::RollbackFailed,
            Some(format!("更新失败：{install_err}；回滚失败：{e}")),
            Some(e),
        ),
    }
}
