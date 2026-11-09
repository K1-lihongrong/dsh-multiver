use serde::Serialize;

/// A2-8/A2-12：插件失败的结构化错误分类。
/// 序列化后 code 仍是稳定的 snake_case 字符串（与前端 ErrorKind 对齐），
/// 但 TypeScript bindings 会得到精确的联合类型，不再靠字符串猜测。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    VersionIncompatible,
    ReleaseTooNew,
    FileLocked,
    Network,
    WrongEnvironment,
    PeerWarning,
    BuildBlocked,
    Unknown,
}

impl ErrorKind {
    /// 与 serde / 前端 ErrorKind 保持一致的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::VersionIncompatible => "version_incompatible",
            ErrorKind::ReleaseTooNew => "release_too_new",
            ErrorKind::FileLocked => "file_locked",
            ErrorKind::Network => "network",
            ErrorKind::WrongEnvironment => "wrong_environment",
            ErrorKind::PeerWarning => "peer_warning",
            ErrorKind::BuildBlocked => "build_blocked",
            ErrorKind::Unknown => "unknown",
        }
    }
}

/// 插件安装失败的结构化分类。`raw` 始终保留完整 pnpm/dsh 输出。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
pub struct PluginFailure {
    pub code: ErrorKind,
    pub message: String,
    pub detail: String,
    pub raw: String,
    pub log_path: Option<String>,
}

pub fn classify(raw: &str) -> PluginFailure {
    let lower = raw.to_ascii_lowercase();
    let code = if lower.contains("is incompatible with dsh")
        || lower.contains("exact-version exemption")
    {
        ErrorKind::VersionIncompatible
    } else if lower.contains("allowbuilds")
        || lower.contains("ignored builds")
        || lower.contains("ignored build script")
        || lower.contains("err_pnpm_ignored_builds")
        || lower.contains("prepare script, which pnpm blocks")
    {
        ErrorKind::BuildBlocked
    } else if lower.contains("minimumreleaseage")
        || lower.contains("minimum release age")
        || lower.contains("published too recently")
        || lower.contains("release age")
    {
        ErrorKind::ReleaseTooNew
    } else if lower.contains("ebusy")
        || lower.contains("eperm")
        || lower.contains("eacces")
        || lower.contains("elocked")
        || lower.contains("being used by another process")
        || lower.contains("resource busy")
        || lower.contains("enotempty")
    {
        ErrorKind::FileLocked
    } else if lower.contains("peer dependencies")
        || lower.contains("unmet peer")
        || lower.contains("peer dep")
    {
        ErrorKind::PeerWarning
    } else if lower.contains("enotfound")
        || lower.contains("econnreset")
        || lower.contains("etimedout")
        || lower.contains("eai_again")
        || lower.contains("fetch failed")
        || lower.contains("network")
        || lower.contains("err_pnpm_fetch")
        || lower.contains("registry")
    {
        ErrorKind::Network
    } else if lower.contains("profile")
        && (lower.contains("not found")
            || lower.contains("missing")
            || lower.contains("does not exist"))
        || lower.contains("cannot find package")
        || lower.contains("node_modules") && lower.contains("missing")
    {
        ErrorKind::WrongEnvironment
    } else {
        ErrorKind::Unknown
    };
    let (message, detail) = match code {
        ErrorKind::VersionIncompatible => (
            "插件与当前 DSH 版本不兼容",
            "可在安装失败窗口允许精确版本，或安装该插件适配的版本。",
        ),
        ErrorKind::ReleaseTooNew => (
            "插件版本被发布保护期拦截",
            "等待保护期结束，或在 profile 配置中精确放行该版本后重试。",
        ),
        ErrorKind::FileLocked => (
            "文件被占用或权限不足",
            "关闭正在使用该 profile 的 dsh/插件进程，检查杀毒软件或文件权限后重试。",
        ),
        ErrorKind::Network => (
            "网络或 registry 访问失败",
            "检查网络/代理，切换安装源或 registry 后重试。",
        ),
        ErrorKind::WrongEnvironment => (
            "插件装到了错误的 profile 或版本环境",
            "核对当前版本、profile、DSH_HOME 和安装位置；安装成功不等于已激活。",
        ),
        ErrorKind::PeerWarning => (
            "存在 peer 依赖警告",
            "多数警告可忽略；若 dsh 明确拒绝启动，按提示补齐或更新对应依赖。",
        ),
        ErrorKind::BuildBlocked => (
            "构建脚本被 pnpm 拦截",
            "在失败窗口点击允许构建并重试，或把提示的 key 写入 profile 的 allowBuilds。",
        ),
        _ => ("插件操作失败", "查看原始输出和诊断日志定位具体原因。"),
    };
    let log_path = raw.lines().find_map(|line| {
        let line = line.trim();
        line.find("diagnostics:")
            .map(|idx| line[idx + "diagnostics:".len()..].trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    });
    PluginFailure {
        code,
        message: message.into(),
        detail: detail.into(),
        raw: raw.to_string(),
        log_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_version_incompatible() {
        let f = classify("Plugin @a/b@1.0.0 is incompatible with dsh 0.2.2-rc.1");
        assert_eq!(f.code, ErrorKind::VersionIncompatible);
        assert!(f.raw.contains("Plugin"));
    }

    #[test]
    fn classifies_release_age_and_lock() {
        assert_eq!(
            classify("ERR_PNPM_MINIMUM_RELEASE_AGE published too recently").code,
            ErrorKind::ReleaseTooNew
        );
        assert_eq!(
            classify("EPERM: operation not permitted, being used by another process").code,
            ErrorKind::FileLocked
        );
    }

    #[test]
    fn classifies_network_peer_build_and_environment() {
        assert_eq!(
            classify("ERR_PNPM_FETCH_404 registry fetch failed").code,
            ErrorKind::Network
        );
        assert_eq!(
            classify("WARN Issues with peer dependencies found").code,
            ErrorKind::PeerWarning
        );
        assert_eq!(
            classify(
                "git-hosted plugins build on install via their prepare script, which pnpm blocks"
            )
            .code,
            ErrorKind::BuildBlocked
        );
        assert_eq!(
            classify("profile web not found").code,
            ErrorKind::WrongEnvironment
        );
        assert_eq!(classify("something else").code, ErrorKind::Unknown);
    }

    #[test]
    fn keeps_raw_and_extracts_log_path() {
        let f = classify("dsh: plugin command failed; diagnostics: E:/tmp/pnpm.log");
        assert_eq!(f.log_path.as_deref(), Some("E:/tmp/pnpm.log"));
        assert_eq!(
            f.raw,
            "dsh: plugin command failed; diagnostics: E:/tmp/pnpm.log"
        );
    }
}
