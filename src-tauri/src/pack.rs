//! 导出（pack）：把一个整合包实例目录打回 .dspack 容器。
//!
//! 与 modpack.rs 的导入互为逆操作：
//!   实例目录 → 收集 profile 根 → overrides/ + home/ → 生成 manifest/dspack.json → ZIP
//!
//! 关键设计：
//! - 依赖坐标从实例 package.json 的 dependencies 反推（导入时由 manifest 权威重建，故准确）
//! - bundles 从 package.json 的 dsh.profile.bundles 读
//! - 安全过滤复用 modpack::sensitive_reason（跳过 node_modules/凭据/密钥等）
//! - profile 根：实例目录里排除 home/ node_modules/ package.json .npmrc 元数据后的其余内容
//! - home/：只收 skills/ 与 profiles/（sessions/storages 等运行时数据不导出）

use std::io::Write;
use std::path::{Path, PathBuf};

/// 导出结果。
#[derive(Debug)]
pub struct ExportOutcome {
    /// 生成的 .dspack 路径
    pub output: PathBuf,
    /// 打进包的文件数
    pub file_count: u64,
    /// 被安全过滤跳过的条目
    pub skipped: Vec<String>,
}

/// 导出用的元信息（从实例 package.json + 元数据收集）。
struct InstanceInfo {
    name: String,
    version: String,
    display_name: String,
    description: String,
    author: String,
    icon: String,
    dsh_version: String,
    modpack_type: String,
    profile_name: String,
    bundles: Vec<String>,
    /// 从 package.json 反推的 manifest 坐标（dependencies 字段）
    manifest_deps: serde_json::Map<String, serde_json::Value>,
}

/// 从实例目录读取 package.json 与元数据，构造导出所需信息。
fn collect_info(instance_dir: &Path) -> Result<InstanceInfo, String> {
    // 1) package.json（依赖与 bundles 的事实源）
    let pkg_path = instance_dir.join("package.json");
    let pkg_text = std::fs::read_to_string(&pkg_path)
        .map_err(|e| format!("读取实例 package.json 失败 {}: {}", pkg_path.display(), e))?;
    let pkg: serde_json::Value = serde_json::from_str(&pkg_text)
        .map_err(|e| format!("实例 package.json 解析失败: {}", e))?;

    let bundles: Vec<String> = pkg
        .get("dsh")
        .and_then(|d| d.get("profile"))
        .and_then(|p| p.get("bundles"))
        .and_then(|b| b.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    // package.json dependencies → manifest 坐标（反向转换）
    let mut manifest_deps = serde_json::Map::new();
    if let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_object()) {
        for (name, spec) in deps {
            let spec = spec.as_str().unwrap_or("").to_string();
            // 反向：github:owner/repo#sha → "github:owner/repo": "sha"
            if let Some(rest) = spec.strip_prefix("github:") {
                // rest 可能是 "owner/repo#sha" 或 "owner/repo#sha&path:pkg"
                let (repo_part, frag) = match rest.split_once('#') {
                    Some((r, f)) => (r, Some(f)),
                    None => (rest, None),
                };
                let (sha, path) = match frag {
                    Some(f) => match f.split_once("&path:") {
                        Some((s, p)) => (s.to_string(), Some(p.to_string())),
                        None => (f.to_string(), None),
                    },
                    None => (String::new(), None),
                };
                let coord = match path {
                    Some(p) => format!("github:{}#path:/{}", repo_part, p),
                    None => format!("github:{}", repo_part),
                };
                manifest_deps.insert(coord, serde_json::Value::String(sha));
            } else {
                // npm 精确版：原样
                manifest_deps.insert(name.clone(), serde_json::Value::String(spec.clone()));
            }
        }
    }

    // 2) 元数据（展示字段 / dsh 版本 / profile 名 / 类型）
    let meta_path = instance_dir.join(".dsh-multiver-meta.json");
    let meta: serde_json::Value = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(serde_json::Value::Null);

    let dir_name = instance_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    // 从目录名 modpack-<name>-<version> 兜底解析
    let (fallback_name, fallback_version) = parse_dir_name(&dir_name);

    let get_str = |k: &str, fb: String| -> String {
        meta.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(fb)
    };

    let info = InstanceInfo {
        name: get_str("modpack_name", fallback_name),
        version: get_str("modpack_version", fallback_version),
        display_name: get_str("display_name", String::new()),
        description: get_str("description", String::new()),
        author: get_str("author", String::new()),
        icon: get_str("icon", String::new()),
        dsh_version: get_str("packed_dsh_version", String::new()),
        modpack_type: get_str("modpack_type", "profile".to_string()),
        profile_name: get_str("launch_profile", String::new()),
        bundles,
        manifest_deps,
    };
    Ok(info)
}

/// 从 modpack-<name>-<version> 目录名解析 name / version。
/// 版本形如 1.0.0 / 0.2.0-rc.1，name 是 kebab-case。
fn parse_dir_name(dir: &str) -> (String, String) {
    let s = dir.strip_prefix("modpack-").unwrap_or(dir);
    // 从右往左找第一段形如 <数字>.<数字>.<数字>... 的开始位置
    let bytes = s.as_bytes();
    let mut idx = None;
    for i in 1..bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1..].first().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            // 检查这一段是否像版本号（含点）
            let rest = &s[i + 1..];
            if rest.contains('.') && rest.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                idx = Some(i);
            }
        }
    }
    match idx {
        Some(i) => (s[..i].to_string(), s[i + 1..].to_string()),
        None => (s.to_string(), "0.0.0".to_string()),
    }
}

/// 生成 manifest.json（v5，profile 形态）。
fn build_manifest(info: &InstanceInfo) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("manifestVersion".into(), serde_json::json!(5));
    m.insert("type".into(), serde_json::json!(info.modpack_type));
    m.insert("name".into(), serde_json::json!(info.name));
    m.insert("version".into(), serde_json::json!(info.version));
    if !info.display_name.is_empty() {
        m.insert("displayName".into(), serde_json::json!(info.display_name));
    }
    if !info.description.is_empty() {
        m.insert("description".into(), serde_json::json!(info.description));
    }
    if !info.author.is_empty() {
        m.insert("author".into(), serde_json::json!(info.author));
    }
    if !info.icon.is_empty() {
        m.insert("icon".into(), serde_json::json!(info.icon));
    }
    if !info.dsh_version.is_empty() {
        m.insert("dshVersion".into(), serde_json::json!(info.dsh_version));
    }
    m.insert("profileName".into(), serde_json::json!(info.profile_name));
    m.insert(
        "bundles".into(),
        serde_json::Value::Array(
            info.bundles.iter().map(|b| serde_json::json!(b)).collect(),
        ),
    );
    m.insert(
        "dependencies".into(),
        serde_json::Value::Object(info.manifest_deps.clone()),
    );
    serde_json::Value::Object(m)
}

/// 递归收集目录内容到 ZIP（施加安全过滤）。
///
/// `zip_prefix` 是写入 ZIP 时的路径前缀（如 "overrides" / "home"）。
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    cur: &Path,
    zip_prefix: &str,
    skipped: &mut Vec<String>,
    count: &mut u64,
) -> Result<(), String> {
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let entries = std::fs::read_dir(cur)
        .map_err(|e| format!("读取目录失败 {}: {}", cur.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| format!("路径解析失败: {}", e))?;

        // 安全过滤（复用导入侧规则）
        if let Some(reason) = crate::modpack::sensitive_reason(rel) {
            skipped.push(format!("{}/{}（{}）", zip_prefix, rel.display(), reason));
            continue;
        }

        if path.is_dir() {
            add_dir_to_zip(zip, root, &path, zip_prefix, skipped, count)?;
        } else {
            let name = format!("{}/{}", zip_prefix, rel.to_string_lossy().replace('\\', "/"));
            zip.start_file(name, opts)
                .map_err(|e| format!("写入 ZIP 条目失败: {}", e))?;
            let data = std::fs::read(&path)
                .map_err(|e| format!("读取文件失败 {}: {}", path.display(), e))?;
            zip.write_all(&data)
                .map_err(|e| format!("写入 ZIP 数据失败: {}", e))?;
            *count += 1;
        }
    }
    Ok(())
}

/// 把一个整合包实例目录导出为 .dspack。
pub fn export_instance(instance_dir: &Path, output: &Path) -> Result<ExportOutcome, String> {
    if !instance_dir.is_dir() {
        return Err(format!("实例目录不存在: {}", instance_dir.display()));
    }
    let info = collect_info(instance_dir)?;

    let file = std::fs::File::create(output)
        .map_err(|e| format!("创建输出文件失败 {}: {}", output.display(), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut skipped: Vec<String> = Vec::new();
    let mut count = 0u64;

    // 1) dspack.json（容器标记，v3）
    zip.start_file("dspack.json", opts)
        .map_err(|e| format!("写入 dspack.json 失败: {}", e))?;
    zip.write_all(br#"{ "format": "dspack", "version": 3 }"#)
        .map_err(|e| format!("写入 dspack.json 失败: {}", e))?;

    // 2) manifest.json
    zip.start_file("manifest.json", opts)
        .map_err(|e| format!("写入 manifest.json 失败: {}", e))?;
    let manifest_text = serde_json::to_string_pretty(&build_manifest(&info))
        .map_err(|e| format!("生成 manifest 失败: {}", e))?;
    zip.write_all(manifest_text.as_bytes())
        .map_err(|e| format!("写入 manifest.json 失败: {}", e))?;

    // 3) profile 根 → overrides/
    //    排除 home/ node_modules/ package.json .npmrc 元数据
    let overrides_tmp = std::env::temp_dir().join(format!(
        "dsh-modpack-export-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&overrides_tmp)
        .map_err(|e| format!("创建临时目录失败: {}", e))?;

    let collect_result = (|| -> Result<(), String> {
        // 收集 profile 根文件
        let entries = std::fs::read_dir(instance_dir)
            .map_err(|e| format!("读取实例目录失败: {}", e))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                "home" | "node_modules" | "package.json" | ".npmrc" | ".dsh-multiver-meta.json"
            ) {
                continue;
            }
            let src = entry.path();
            if src.is_file() {
                if let Some(reason) = crate::modpack::sensitive_reason(Path::new(&name)) {
                    skipped.push(format!("overrides/{}（{}）", name, reason));
                    continue;
                }
                zip.start_file(format!("overrides/{}", name), opts)
                    .map_err(|e| format!("写入 ZIP 失败: {}", e))?;
                let data = std::fs::read(&src)
                    .map_err(|e| format!("读取文件失败 {}: {}", src.display(), e))?;
                zip.write_all(&data).map_err(|e| format!("写入 ZIP 失败: {}", e))?;
                count += 1;
            } else if src.is_dir() {
                add_dir_to_zip(&mut zip, instance_dir, &src, "overrides", &mut skipped, &mut count)?;
            }
        }

        // 4) home/skills 与 home/profiles → home/
        let home = instance_dir.join("home");
        for sub in ["skills", "profiles"] {
            let p = home.join(sub);
            if p.is_dir() {
                let prefix = format!("home/{}", sub);
                add_dir_to_zip(&mut zip, &home, &p, &prefix, &mut skipped, &mut count)?;
            }
        }

        zip.finish().map_err(|e| format!("完成 ZIP 失败: {}", e))?;
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&overrides_tmp);

    collect_result?;

    Ok(ExportOutcome {
        output: output.to_path_buf(),
        file_count: count,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dir_name() {
        assert_eq!(
            parse_dir_name("modpack-dsh-simple-drawing-1.0.0"),
            ("dsh-simple-drawing".to_string(), "1.0.0".to_string())
        );
        assert_eq!(
            parse_dir_name("modpack-pokemon-0.2.0-rc.1"),
            ("pokemon".to_string(), "0.2.0-rc.1".to_string())
        );
    }

    #[test]
    fn reverse_github_coord() {
        // 通过 collect_info 不便单测（需目录），这里直接验证转换逻辑片段
        // github:owner/repo#sha → "github:owner/repo": "sha"
        let spec = "github:o/r#abc123";
        let rest = spec.strip_prefix("github:").unwrap();
        let (repo_part, frag) = rest.split_once('#').unwrap();
        assert_eq!(repo_part, "o/r");
        assert_eq!(frag, "abc123");
    }
}
