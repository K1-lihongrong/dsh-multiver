//! 启动失败诊断与会话日志解析（D14 拆分：原 launcher.rs 的日志诊断）。
//!
//! - read_recent_errors：从会话日志提取 [err] 行 + 追加诊断建议
//! - diagnose：识别已知故障模式，返回可操作建议（纯函数，可单测）
//! - latest_session_errors：找该版本最新会话日志并提取错误
//! - parse_url：从 dsh 输出行解析访问 URL

use std::path::Path;
/// 从会话日志里提取最近的 `[err]` 行（dsh 启动失败时的真实错误），用于拼进错误提示。
/// 只取末尾最多 `max_lines` 行、总长不超过 ~800 字符，避免提示过长。
/// 若识别出已知故障模式，会在末尾追加「诊断建议」（见 `diagnose`）。
pub(crate) fn read_recent_errors(session_path: &Path, max_lines: usize) -> String {
    let content = match std::fs::read_to_string(session_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let errs: Vec<&str> = content
        .lines()
        .filter(|l| l.starts_with("[err] "))
        .map(|l| l.trim_start_matches("[err] "))
        .collect();
    if errs.is_empty() {
        return String::new();
    }
    // 诊断用**完整**错误文本（诊断关键词可能出现在早于末尾 max_lines 的行，
    // 如 profile bundle 错误在栈顶、末尾只是调用栈）。先算，再截断显示。
    let full_text = errs.join("\n");
    let advice = diagnose(&full_text);
    // 取末尾 max_lines 行用于显示
    let start = errs.len().saturating_sub(max_lines);
    let mut joined = errs[start..].join("\n");
    if joined.chars().count() > 800 {
        joined = joined.chars().take(800).collect::<String>() + " …";
    }
    if let Some(advice) = advice {
        joined.push_str("\n\n【诊断建议】");
        joined.push_str(advice);
    }
    joined
}

/// 识别 dsh 启动失败日志里的**已知故障模式**，返回可操作建议。
/// 纯函数（无 IO），便于单测。识别不到返回 `None`。
///
/// 返回的字符串**不含**「诊断建议」前缀，由调用方拼接。
pub(crate) fn diagnose(err_text: &str) -> Option<&'static str> {
    let t = err_text;
    // 1. profile bundle 缺失：共享 home 的 profile 要求某版本没有的插件
    if t.contains("cannot resolve profile bundle") {
        return Some(
            "该版本与共享 home 的 profile 不兼容（共享 home 里配置了此版本没有的插件）。\
建议：给该版本开启「数据隔离」，使用独立 home；或在该版本下重装所需插件。",
        );
    }
    // 2. Node 版本过低：dsh 依赖 import.meta.main，旧版 22.x 不支持
    //    （表现为无输出 / exit 0；此处以显式文本为准）
    if t.contains("import.meta.main") {
        return Some(
            "当前 Node 版本过低。dsh 需要 Node.js 22.19+ 或 24+，\
请升级 Node 后重试（可在环境检查里查看详情）。",
        );
    }
    // 3. 端口占用
    if t.contains("EADDRINUSE") {
        return Some("端口被占用。可稍后重试，或关闭占用该端口的程序。");
    }
    // 4. 网络 / registry 不通
    if t.contains("ETIMEDOUT")
        || t.contains("ENOTFOUND")
        || t.contains("ECONNREFUSED")
        || t.contains("fetch failed")
    {
        return Some(
            "网络或 npm 源不可达。请检查网络/代理；安装时可在弹窗里换一个 npm 源（如阿里云 npmmirror）重试。",
        );
    }
    // 5. 磁盘满
    if t.contains("ENOSPC") {
        return Some("磁盘空间不足。请清理磁盘后重试。");
    }
    // 6. 权限不足
    if t.contains("EACCES") || t.contains("EPERM") {
        return Some("权限不足。请把数据根目录改到一个有写权限的位置，或以管理员身份运行。");
    }
    // 7. profile 已存在（初始化冲突）—— dsh 源码 profile-boot
    if t.contains("already exists") && t.contains("profile") {
        return Some(
            "该 profile 已存在（可能之前初始化过）。可换个未用过的 profile 名，\
或去掉 --from-default-profile 直接使用现有 profile。",
        );
    }
    // 8. desktop profile 由 Electron 应用独占 —— dsh 源码 bin.js
    if t.contains("managed exclusively by the Electron application") {
        return Some(
            "desktop profile 由桌面应用独占，命令行不能直接引导。\
请改用 web 或 tui profile。",
        );
    }
    // 9. 插件版本与 DSH 运行时不兼容 —— dsh 源码 plugin
    if t.contains("allow-version") && t.contains("accept-risk") {
        return Some(
            "插件版本与当前 DSH 运行时不兼容。若确认风险可接受，\
按提示运行 dsh plugin ... allow-version <包>@<版本> --accept-risk；否则换装兼容版本。",
        );
    }
    // 10. git 源插件 build 被 pnpm 拦截 —— dsh 源码 plugin
    if t.contains("prepare script, which pnpm blocks") {
        return Some(
            "该 git 插件安装时需构建（prepare 脚本），被 pnpm 默认拦截。\
请把 pnpm 提示的 key 加到该 profile 的 pnpm-workspace.yaml 的 allowBuilds 下，再重试。",
        );
    }
    // 11. 缺 profile 参数 —— dsh 源码 bin.js
    if t.contains("--profile <name> is required") {
        return Some(
            "启动时缺少 profile 参数。请指定要引导的 profile，如 dsh web 或 dsh --profile web。",
        );
    }
    None
}

/// 在日志目录里找该版本**最新**的会话日志，提取其 `[err]` 行。
/// 用于「dsh 启动后立即崩溃」时向用户展示真实错误。
pub(crate) fn latest_session_errors(log_dir: &Path, version: &str, max_lines: usize) -> String {
    let suffix = format!("-{}.log", version);
    let mut newest_name = String::new();
    let mut newest_path: Option<std::path::PathBuf> = None;
    if let Ok(rd) = std::fs::read_dir(log_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("session-") && name.ends_with(&suffix) {
                // 文件名内嵌时间戳，字典序即时间序
                if newest_path.is_none() || name > newest_name {
                    newest_name = name;
                    newest_path = Some(e.path());
                }
            }
        }
    }
    newest_path
        .map(|p| read_recent_errors(&p, max_lines))
        .unwrap_or_default()
}

/// 从一行文本中解析完整 URL：取 `dsh web: <URL>` 之后的 URL 整体（到行尾/空格前）。
/// 兼容 127.0.0.1 与 localhost。返回形如 http://127.0.0.1:<port>/?token=<token>。
pub(crate) fn parse_url(line: &str) -> Option<String> {
    // 优先识别 `dsh web:` 标记行
    let candidate = if let Some(idx) = line.find("dsh web:") {
        line[idx + "dsh web:".len()..].trim()
    } else {
        line.trim()
    };
    let start = candidate.find("http://")?;
    let rest = &candidate[start..];
    // URL 到行尾或第一个空白字符结束
    let url: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    if url.contains("127.0.0.1:") || url.contains("localhost:") {
        Some(url)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_marker_line() {
        let line = "dsh web: http://127.0.0.1:44179/?token=abc123";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:44179/?token=abc123".to_string())
        );
    }

    #[test]
    fn parse_url_bare_url() {
        let line = "  http://127.0.0.1:8080/?token=xyz  ";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:8080/?token=xyz".to_string())
        );
    }

    #[test]
    fn parse_url_localhost_variant() {
        let line = "dsh web: http://localhost:3000/";
        assert_eq!(parse_url(line), Some("http://localhost:3000/".to_string()));
    }

    #[test]
    fn parse_url_ignores_trailing_text() {
        // URL 后跟空白/其他内容时只取到空白前
        let line = "dsh web: http://127.0.0.1:5000/?token=t  (按 Ctrl+C 退出)";
        assert_eq!(
            parse_url(line),
            Some("http://127.0.0.1:5000/?token=t".to_string())
        );
    }

    #[test]
    fn parse_url_rejects_non_loopback() {
        // 非 127.0.0.1 / localhost 的地址不识别
        assert_eq!(parse_url("http://192.168.1.5:8080/"), None);
        assert_eq!(parse_url("no url here"), None);
    }

    // ── read_recent_errors：从会话日志提取 [err] 行 ──

    fn tmp_session(content: &str) -> std::path::PathBuf {
        // 用纳秒 + 原子计数保证唯一，避免同一秒内多个测试互相覆盖
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "dsh-launch-{}-{}-{}.log",
            std::process::id(),
            nanos,
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn read_recent_errors_extracts_err_lines() {
        let p = tmp_session(
            "===== session =====\n[out] dsh web: http://x\n[err] boom 1\n[err] boom 2\n",
        );
        let got = read_recent_errors(&p, 6);
        assert!(got.contains("boom 1"), "got={}", got);
        assert!(got.contains("boom 2"), "got={}", got);
        assert!(!got.contains("[err]"), "应去掉 [err] 前缀: {}", got);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_recent_errors_keeps_only_last_n() {
        let content = (1..=10)
            .map(|i| format!("[err] line {}\n", i))
            .collect::<String>();
        let p = tmp_session(&content);
        let got = read_recent_errors(&p, 3);
        assert!(got.contains("line 10"));
        assert!(got.contains("line 8"));
        assert!(!got.contains("line 7"), "只保留末尾 3 行: {}", got);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_recent_errors_empty_when_no_err() {
        let p = tmp_session("[out] dsh web: http://x\n");
        assert_eq!(read_recent_errors(&p, 6), "");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_recent_errors_appends_diagnosis_on_known_pattern() {
        // GAP-015：识别 profile bundle 缺失并追加诊断建议
        let p = tmp_session(
            "[err] Error: dsh: cannot resolve profile bundle \"@x/y\" from the dsh installation\n",
        );
        let got = read_recent_errors(&p, 6);
        assert!(got.contains("cannot resolve profile bundle"), "got={}", got);
        assert!(got.contains("【诊断建议】"), "应追加诊断建议: {}", got);
        assert!(got.contains("数据隔离"), "建议应提到数据隔离: {}", got);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_recent_errors_no_diagnosis_on_unknown_pattern() {
        let p = tmp_session("[err] some totally unknown failure xyz\n");
        let got = read_recent_errors(&p, 6);
        assert!(got.contains("unknown failure"), "got={}", got);
        assert!(
            !got.contains("【诊断建议】"),
            "未知错误不应追加建议: {}",
            got
        );
        let _ = std::fs::remove_file(&p);
    }

    // ── diagnose：已知故障模式识别 ──

    #[test]
    fn diagnose_matches_all_known_patterns() {
        // 11 类模式各命中一次
        assert!(diagnose("cannot resolve profile bundle \"@a/b\"").is_some());
        assert!(diagnose("TypeError: import.meta.main is undefined").is_some());
        assert!(diagnose("Error: listen EADDRINUSE :::9323").is_some());
        assert!(diagnose("Error: connect ETIMEDOUT 1.2.3.4:443").is_some());
        assert!(diagnose("Error: ENOSPC: no space left on device").is_some());
        assert!(diagnose("Error: EACCES: permission denied").is_some());
        // 新增 5 条（dsh 源码调研）
        assert!(diagnose("dsh: profile \"web\" already exists at /x").is_some());
        assert!(diagnose(
            "error: profile \"desktop\" is managed exclusively by the Electron application"
        )
        .is_some());
        assert!(diagnose("dsh: to accept the risk, run: dsh plugin --profile web allow-version x@1 --accept-risk").is_some());
        assert!(diagnose(
            "dsh: git-hosted plugins build on install via their prepare script, which pnpm blocks"
        )
        .is_some());
        assert!(diagnose("error: --profile <name> is required").is_some());
    }

    #[test]
    fn diagnose_returns_none_for_unknown() {
        assert!(diagnose("").is_none());
        assert!(diagnose("some random error").is_none());
    }

    #[test]
    fn read_recent_errors_diagnoses_when_keyword_before_last_lines() {
        // 关键词在栈顶（早于末尾 max_lines 行），末尾只是调用栈。
        // 诊断应用完整文本，不能被 max_lines 截断掉关键词。
        let content = "\
[err] Error: dsh: cannot resolve profile bundle \"@a/b\" from the dsh installation\n\
[err]     at resolveBundleDir (x.js:1)\n\
[err]     at loadProfileDirectory (x.js:2)\n\
[err]     at loadProfile (x.js:3)\n\
[err]     at prepareProfile (x.js:4)\n\
[err]     at composeProfile (x.js:5)\n\
[err]     at runProfile (x.js:6)\n\
[err]     at async runCli (x.js:7)\n";
        let p = tmp_session(content);
        // 只显示末尾 3 行，但诊断应命中（关键词在第 1 行）
        let got = read_recent_errors(&p, 3);
        assert!(
            !got.contains("cannot resolve"),
            "显示应只末尾 3 行: {}",
            got
        );
        assert!(got.contains("【诊断建议】"), "诊断应命中完整文本: {}", got);
        let _ = std::fs::remove_file(&p);
    }
}
