use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default, specta::Type)]
pub struct HealthSummary {
    pub scanned: usize,
    pub stale: usize,
    pub dangling: usize,
    pub duplicate: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
pub struct HealthIssue {
    pub kind: String,
    pub package: String,
    pub source_version: Option<String>,
    pub target: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
pub struct SharedHomeHealth {
    pub home: String,
    pub current_version: String,
    pub summary: HealthSummary,
    pub issues: Vec<HealthIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkRecord {
    package: String,
    target: String,
    source_version: Option<String>,
    exists: bool,
}

pub fn scan(home: &Path, current_version: &str) -> SharedHomeHealth {
    let nm = home.join("profiles").join("node_modules");
    let mut links = Vec::new();
    collect_links(&nm, &mut links);
    analyze(home, current_version, links)
}

fn collect_links(nm: &Path, out: &mut Vec<LinkRecord>) {
    let Ok(entries) = std::fs::read_dir(nm) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('@') && !is_reparse_point(&path) {
            if let Ok(children) = std::fs::read_dir(&path) {
                for child in children.flatten() {
                    let child_path = child.path();
                    if is_reparse_point(&child_path) {
                        let package = format!("{}/{}", name, child.file_name().to_string_lossy());
                        push_link(&package, &child_path, out);
                    }
                }
            }
        } else if is_reparse_point(&path) {
            push_link(&name, &path, out);
        }
    }
}

fn push_link(package: &str, path: &Path, out: &mut Vec<LinkRecord>) {
    let Ok(target) = std::fs::read_link(path) else {
        return;
    };
    let target_string = target.to_string_lossy().to_string();
    let source_version = version_from_target(&target);
    let exists = target.exists();
    out.push(LinkRecord {
        package: package.to_string(),
        target: target_string,
        source_version,
        exists,
    });
}

fn version_from_target(target: &Path) -> Option<String> {
    let parts: Vec<String> = target
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let idx = parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("versions"))?;
    parts.get(idx + 1).cloned()
}

fn analyze(home: &Path, current_version: &str, links: Vec<LinkRecord>) -> SharedHomeHealth {
    let mut issues = Vec::new();
    let mut summary = HealthSummary {
        scanned: links.len(),
        ..Default::default()
    };
    let mut by_package: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for link in &links {
        if !seen.insert((link.package.clone(), link.target.clone())) {
            continue;
        }
        if let Some(v) = &link.source_version {
            by_package
                .entry(link.package.clone())
                .or_default()
                .insert(v.clone());
        }
        if !link.exists {
            summary.dangling += 1;
            issues.push(HealthIssue {
                kind: "dangling_junction".into(),
                package: link.package.clone(),
                source_version: link.source_version.clone(),
                target: Some(link.target.clone()),
                detail: "链接目标已不存在".into(),
            });
        }
        if let Some(v) = &link.source_version {
            if v != current_version {
                summary.stale += 1;
                issues.push(HealthIssue {
                    kind: "stale_junction".into(),
                    package: link.package.clone(),
                    source_version: Some(v.clone()),
                    target: Some(link.target.clone()),
                    detail: format!("链接来自 DSH {}，当前运行 {}", v, current_version),
                });
            }
        }
    }
    for (package, versions) in by_package {
        if versions.len() > 1 {
            summary.duplicate += 1;
            issues.push(HealthIssue {
                kind: "duplicate_source".into(),
                package,
                source_version: None,
                target: None,
                detail: format!(
                    "同一包同时来自多个 DSH 版本：{}",
                    versions.into_iter().collect::<Vec<_>>().join(", ")
                ),
            });
        }
    }
    issues.sort_by(|a, b| a.package.cmp(&b.package).then(a.kind.cmp(&b.kind)));
    SharedHomeHealth {
        home: home.to_string_lossy().to_string(),
        current_version: current_version.to_string(),
        summary,
        issues,
    }
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    std::fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(package: &str, version: Option<&str>, exists: bool) -> LinkRecord {
        LinkRecord {
            package: package.into(),
            target: format!(
                "versions/{}/node_modules/{}",
                version.unwrap_or("x"),
                package
            ),
            source_version: version.map(|v| v.to_string()),
            exists,
        }
    }

    #[test]
    fn analyzes_stale_dangling_and_duplicate() {
        let links = vec![
            rec("@scope/a", Some("0.0.1-rc.5"), true),
            rec("@scope/a", Some("0.2.2-rc.1"), false),
            rec("@scope/b", Some("0.0.1-rc.5"), true),
        ];
        let report = analyze(Path::new("E:/home"), "0.2.2-rc.1", links);
        assert_eq!(report.summary.scanned, 3);
        assert!(report.issues.iter().any(|i| i.kind == "stale_junction"));
        assert!(report.issues.iter().any(|i| i.kind == "dangling_junction"));
        assert!(report.issues.iter().any(|i| i.kind == "duplicate_source"));
        assert!(report.issues.iter().any(|i| i.package == "@scope/a"));
    }

    #[test]
    fn clean_links_have_no_issues() {
        let links = vec![rec("pkg", Some("0.2.2-rc.1"), true)];
        let report = analyze(Path::new("E:/home"), "0.2.2-rc.1", links);
        assert_eq!(report.summary.stale, 0);
        assert_eq!(report.summary.dangling, 0);
        assert!(report.issues.is_empty());
    }
}
