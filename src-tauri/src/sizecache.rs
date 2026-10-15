//! v0.2.13 占用扫描结果缓存。
//!
//! 只存汇总（约 200B/版本），单文件覆盖写，不膨胀。
//! 指纹 = 已安装版本（名 + 目录 mtime）+ store 目录 mtime 的哈希。
//! 指纹一致 → 命中；否则失效重扫。文件损坏/读失败一律当无缓存。

use crate::versions::SizeInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 缓存文件相对数据根目录的路径。
const CACHE_REL: &str = "cache/size-cache.json";

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    fingerprint: String,
    generated_at: u64,
    versions: HashMap<String, SizeInfo>,
}

/// 计算指纹：已安装版本（名 + 目录 mtime）+ store 目录 mtime。
pub fn fingerprint(versions: &[(String, u64)], store_mtime: u64) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    // 排序保证顺序无关
    let mut v: Vec<&(String, u64)> = versions.iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, mtime) in v {
        name.hash(&mut h);
        mtime.hash(&mut h);
    }
    store_mtime.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn cache_path(root: &Path) -> std::path::PathBuf {
    root.join(CACHE_REL)
}

/// 读缓存；指纹一致则返回 Some(map)，否则 None。
pub fn load(root: &Path, fp: &str) -> Option<HashMap<String, SizeInfo>> {
    let s = std::fs::read_to_string(cache_path(root)).ok()?;
    let f: CacheFile = serde_json::from_str(&s).ok()?;
    if f.fingerprint != fp {
        return None;
    }
    Some(f.versions)
}

/// 写缓存（覆盖写，单文件）。
pub fn save(root: &Path, fp: &str, map: &HashMap<String, SizeInfo>) -> std::io::Result<()> {
    let path = cache_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = CacheFile {
        fingerprint: fp.to_string(),
        generated_at: now_secs(),
        versions: map.clone(),
    };
    let s = serde_json::to_string(&f).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, s)
}

/// 目录的 mtime（秒）；读不到返回 0。
pub fn dir_mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("dsh-sizecache-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample() -> SizeInfo {
        SizeInfo {
            total: 500,
            shared_size: 480,
            shared_count: 24,
            exclusive_size: 20,
        }
    }

    #[test]
    fn save_then_load_hits() {
        let root = tmp_root("hit");
        let fp = "abc123";
        let mut m = HashMap::new();
        m.insert("1.0".to_string(), sample());
        save(&root, fp, &m).unwrap();
        let got = load(&root, fp).expect("should hit");
        assert_eq!(got.get("1.0").unwrap().total, 500);
        assert_eq!(got.get("1.0").unwrap().shared_count, 24);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mismatched_fingerprint_misses() {
        let root = tmp_root("miss");
        let mut m = HashMap::new();
        m.insert("1.0".to_string(), sample());
        save(&root, "fp-A", &m).unwrap();
        assert!(load(&root, "fp-B").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_file_is_none() {
        let root = tmp_root("corrupt");
        let path = cache_path(&root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json {{{").unwrap();
        assert!(load(&root, "any").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn fingerprint_order_independent() {
        let a = vec![("1.0".to_string(), 100u64), ("2.0".to_string(), 200u64)];
        let b = vec![("2.0".to_string(), 200u64), ("1.0".to_string(), 100u64)];
        assert_eq!(fingerprint(&a, 9), fingerprint(&b, 9));
    }

    #[test]
    fn fingerprint_changes_with_mtime() {
        let a = vec![("1.0".to_string(), 100u64)];
        assert_ne!(fingerprint(&a, 9), fingerprint(&a, 10));
    }
}
