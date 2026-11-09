use serde::Serialize;

/// 插件安装失败的结构化分类。`raw` 始终保留完整 pnpm/dsh 输出。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginFailure {
    pub code: String,
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
        "version_incompatible"
    } else if lower.contains("allowbuilds")
        || lower.contains("ignored builds")
        || lower.contains("ignored build script")
        || lower.contains("err_pnpm_ignored_builds")
        || lower.contains("prepare script, which pnpm blocks")
    {
        "build_blocked"
    } else if lower.contains("minimumreleaseage")
        || lower.contains("minimum release age")
        || lower.contains("published too recently")
        || lower.contains("release age")
    {
        "release_too_new"
    } else if lower.contains("ebusy")
        || lower.contains("eperm")
        || lower.contains("eacces")
        || lower.contains("elocked")
        || lower.contains("being used by another process")
        || lower.contains("resource busy")
        || lower.contains("enotempty")
    {
        "file_locked"
    } else if lower.contains("peer dependencies")
        || lower.contains("unmet peer")
        || lower.contains("peer dep")
    {
        "peer_warning"
    } else if lower.contains("enotfound")
        || lower.contains("econnreset")
        || lower.contains("etimedout")
        || lower.contains("eai_again")
        || lower.contains("fetch failed")
        || lower.contains("network")
        || lower.contains("err_pnpm_fetch")
        || lower.contains("registry")
    {
        "network"
    } else if lower.contains("profile")
        && (lower.contains("not found")
            || lower.contains("missing")
            || lower.contains("does not exist"))
        || lower.contains("cannot find package")
        || lower.contains("node_modules") && lower.contains("missing")
    {
        "wrong_environment"
    } else {
        "unknown"
    };
    let (message, detail) = match code {
        "version_incompatible" => (
            "插件与当前 DSH 版本不兼容",
            "可在安装失败窗口允许精确版本，或安装该插件适配的版本。",
        ),
        "release_too_new" => (
            "插件版本被发布保护期拦截",
            "等待保护期结束，或在 profile 配置中精确放行该版本后重试。",
        ),
        "file_locked" => (
            "文件被占用或权限不足",
            "关闭正在使用该 profile 的 dsh/插件进程，检查杀毒软件或文件权限后重试。",
        ),
        "network" => (
            "网络或 registry 访问失败",
            "检查网络/代理，切换安装源或 registry 后重试。",
        ),
        "wrong_environment" => (
            "插件装到了错误的 profile 或版本环境",
            "核对当前版本、profile、DSH_HOME 和安装位置；安装成功不等于已激活。",
        ),
        "peer_warning" => (
            "存在 peer 依赖警告",
            "多数警告可忽略；若 dsh 明确拒绝启动，按提示补齐或更新对应依赖。",
        ),
        "build_blocked" => (
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
        code: code.into(),
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
        assert_eq!(f.code, "version_incompatible");
        assert!(f.raw.contains("Plugin"));
    }

    #[test]
    fn classifies_release_age_and_lock() {
        assert_eq!(
            classify("ERR_PNPM_MINIMUM_RELEASE_AGE published too recently").code,
            "release_too_new"
        );
        assert_eq!(
            classify("EPERM: operation not permitted, being used by another process").code,
            "file_locked"
        );
    }

    #[test]
    fn classifies_network_peer_build_and_environment() {
        assert_eq!(
            classify("ERR_PNPM_FETCH_404 registry fetch failed").code,
            "network"
        );
        assert_eq!(
            classify("WARN Issues with peer dependencies found").code,
            "peer_warning"
        );
        assert_eq!(
            classify(
                "git-hosted plugins build on install via their prepare script, which pnpm blocks"
            )
            .code,
            "build_blocked"
        );
        assert_eq!(classify("profile web not found").code, "wrong_environment");
        assert_eq!(classify("something else").code, "unknown");
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
