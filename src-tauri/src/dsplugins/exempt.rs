//! 版本豁免与 pnpm 构建脚本豁免（compatibility.json / allowBuilds）。

use std::path::Path;

use super::profile::profiles_dir;

/// 读 profile 的版本豁免（官方 `compatibility.json`：`{ "pkg@版本": ["DSH 版本", ...] }`）。
pub fn read_exemptions(dsh_home: &Path, profile: &str) -> serde_json::Value {
    let file = profiles_dir(dsh_home)
        .join(profile)
        .join("compatibility.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// 官方「待决定的构建脚本」占位值（pnpm-workspace.yaml 的 allowBuilds 映射里）。
pub const BUILD_UNDECIDED: &str = "set this to true or false";

/// 读 profile 的 `pnpm-workspace.yaml` 中 allowBuilds 里「待决定」的包名。
pub fn read_pending_builds(profile_dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(profile_dir.join("pnpm-workspace.yaml")) else {
        return Vec::new();
    };
    parse_pending_builds(&text)
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

pub(crate) fn parse_pending_builds(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_allow = false;
    for line in text.lines() {
        if line.starts_with("allowBuilds:") {
            in_allow = true;
            continue;
        }
        if !in_allow {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_allow = false;
            continue;
        }
        if let Some((k, v)) = line.trim().split_once(':') {
            if unquote(v) == BUILD_UNDECIDED {
                out.push(unquote(k));
            }
        }
    }
    out
}

fn yaml_key(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\\\""))
}

/// 把待决定的构建脚本写成 `allowBuilds: { name: true }`（保留其余内容；官方格式）。
pub fn approve_builds(profile_dir: &Path, names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }
    let path = profile_dir.join("pnpm-workspace.yaml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 pnpm-workspace.yaml 失败：{e}"))?;
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut handled = vec![false; names.len()];
    let start = lines.iter().position(|l| l.starts_with("allowBuilds:"));
    match start {
        Some(si) => {
            let mut end = si + 1;
            while end < lines.len() {
                let line = &lines[end];
                if line.trim().is_empty() {
                    end += 1;
                    continue;
                }
                if !line.starts_with(' ') && !line.starts_with('\t') {
                    break;
                }
                if let Some((k, _)) = line.trim().split_once(':') {
                    let key = unquote(k);
                    if let Some(idx) = names.iter().position(|n| n == &key) {
                        let indent = line.len() - line.trim_start().len();
                        lines[end] = format!("{}{}: true", " ".repeat(indent), yaml_key(&key));
                        handled[idx] = true;
                    }
                }
                end += 1;
            }
            let mut insert_at = end;
            while insert_at > si + 1 && lines[insert_at - 1].trim().is_empty() {
                insert_at -= 1;
            }
            let mut extra = Vec::new();
            for (i, n) in names.iter().enumerate() {
                if !handled[i] {
                    extra.push(format!("  {}: true", yaml_key(n)));
                }
            }
            lines.splice(insert_at..insert_at, extra);
        }
        None => {
            if !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push("allowBuilds:".to_string());
            for n in names {
                lines.push(format!("  {}: true", yaml_key(n)));
            }
        }
    }
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    // 原子写：先写临时文件再替换，避免半截文件（官方同类操作也用原子写）。
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, out).map_err(|e| format!("写入临时文件失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("替换 pnpm-workspace.yaml 失败：{e}"))?;
    Ok(())
}
