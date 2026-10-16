//! v0.2.13 占用扫描结果缓存（版本级）。
//!
//! 每个版本单独记指纹：版本目录 mtime + store 目录 mtime。
//! 判定某版本缓存是否有效只看它自己的两个 mtime 是否对得上。
//! store 没变时，装/卸版本不会牵连其他版本的缓存。
//! 文件损坏/格式不符 → 当无缓存。

use crate::versions::SizeInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 缓存文件相对数据根目录的路径。
const CACHE_REL: &str = "cache/size-cache.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// 版本目录 mtime（秒）
    dir_mtime: u64,
    /// store 目录 mtime（秒）
    store_mtime: u64,
    /// 扫描结果
    info: SizeInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    generated_at: u64,
    versions: HashMap<String, Entry>,
}

/// 内存中的缓存视图：版本号 -> 条目。
pub type Cache = HashMap<String, Entry>;

fn cache_path(root: &Path) -> std::path::PathBuf {
    root.join(CACHE_REL)
}

/// 读缓存文件。文件不存在/损坏/格式不符 → 返回空表。
pub fn load(root: &Path) -> Cache {
    let Ok(s) = std::fs::read_to_string(cache_path(root)) else {
        return HashMap::new();
    };
    match serde_json::from_str::<CacheFile>(&s) {
        Ok(f) => f.versions,
        Err(_) => HashMap::new(),
    }
}

/// 写缓存文件（覆盖写）。
pub fn save(root: &Path, cache: &Cache) -> std::io::Result<()> {
    let path = cache_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = CacheFile {
        generated_at: now_secs(),
        versions: cache.clone(),
    };
    let s = serde_json::to_string(&f).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, s)
}

/// 某版本缓存是否有效：两个 mtime 都对得上。
pub fn is_valid(cache: &Cache, version: &str, dir_mtime: u64, store_mtime: u64) -> bool {
    match cache.get(version) {
        Some(e) => e.dir_mtime == dir_mtime && e.store_mtime == store_mtime,
        None => false,
    }
}

/// 写入/更新某版本缓存条目。
pub fn put(cache: &mut Cache, version: &str, dir_mtime: u64, store_mtime: u64, info: SizeInfo) {
    cache.insert(
        version.to_string(),
        Entry {
            dir_mtime,
            store_mtime,
            info,
        },
    );
}

/// 取某版本缓存结果（不校验 mtime，调用方先用 is_valid 判定）。
pub fn get_info(cache: &Cache, version: &str) -> Option<SizeInfo> {
    cache.get(version).map(|e| e.info.clone())
}

/// 目录 mtime（秒）。读不到返回 0。
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
        let p = std::env::temp_dir().join(format!(
            "dsh-sizecache2-test-{}-{}",
            tag,
            std::process::id()
        ));
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
    fn save_then_load_roundtrip() {
        let root = tmp_root("rt");
        let mut c = HashMap::new();
        put(&mut c, "1.0", 100, 200, sample());
        save(&root, &c).unwrap();
        let got = load(&root);
        assert!(is_valid(&got, "1.0", 100, 200));
        assert_eq!(get_info(&got, "1.0").unwrap().total, 500);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_when_dir_mtime_differs() {
        let mut c = HashMap::new();
        put(&mut c, "1.0", 100, 200, sample());
        assert!(!is_valid(&c, "1.0", 101, 200));
    }

    #[test]
    fn invalid_when_store_mtime_differs() {
        let mut c = HashMap::new();
        put(&mut c, "1.0", 100, 200, sample());
        assert!(!is_valid(&c, "1.0", 100, 201));
    }

    #[test]
    fn missing_version_is_invalid() {
        let c = HashMap::new();
        assert!(!is_valid(&c, "9.9", 1, 1));
    }

    #[test]
    fn independent_versions() {
        let mut c = HashMap::new();
        put(&mut c, "1.0", 100, 200, sample());
        put(&mut c, "2.0", 300, 200, sample());
        // store 变了，两个都失效
        assert!(!is_valid(&c, "1.0", 100, 999));
        assert!(!is_valid(&c, "2.0", 300, 999));
        // 只有 2.0 的目录变了
        assert!(is_valid(&c, "1.0", 100, 200));
        assert!(!is_valid(&c, "2.0", 301, 200));
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let root = tmp_root("corrupt");
        let path = cache_path(&root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json {{{").unwrap();
        assert!(load(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
