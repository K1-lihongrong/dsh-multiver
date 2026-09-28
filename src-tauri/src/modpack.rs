//! DSH 整合包（.dspack）支持。
//!
//! 一期实现「导入」：容器解包、manifest 解析校验、四阶段导入与回滚。
//! 协议见 DSH-PackForge（`protocol-2026-09`），方案见项目文档
//! `.cuckooCode/兼容DSH-PackForge-方案.md`。
//!
//! 本模块第一步只做：容器识别（`dspack.json`）与解包。

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `.dspack` 根部的容器标记文件内容。
#[derive(Debug, Clone, Deserialize)]
pub struct DspackMarker {
    pub format: String,
    pub version: u32,
}

/// 支持的容器版本：现行 v3，兼容 v2（manifest v4）。
pub const SUPPORTED_CONTAINER_VERSIONS: &[u32] = &[2, 3];

/// 解包并预检的结果。此时尚未落地任何用户文件。
#[derive(Debug)]
pub struct Extracted {
    /// 解包到的临时目录（调用方负责在结束后清理）
    pub dir: PathBuf,
    /// 容器标记
    pub marker: DspackMarker,
}

impl Extracted {
    /// 清理临时目录。忽略失败（临时目录残留无害）。
    pub fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 解包 `.dspack` 到系统临时目录，并校验容器标记。
///
/// 失败时自动清理临时目录；成功后由调用方持有 `Extracted`，用完调 `cleanup()`。
pub fn extract(dspack_path: &Path) -> Result<Extracted, String> {
    if !dspack_path.is_file() {
        return Err(format!("文件不存在: {}", dspack_path.display()));
    }

    // 临时目录：dsh-multiver-import-<纳秒>，避免并发冲突
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("dsh-multiver-import-{}", stamp));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;

    // 用闭包包住解包+校验，失败时统一清理临时目录
    let result = (|| -> Result<DspackMarker, String> {
        let file = std::fs::File::open(dspack_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| {
            // 非 ZIP（普通压缩包 / 损坏文件）走这里
            format!("这不是 dspack 整合包（无法作为 ZIP 打开）: {}", e)
        })?;

        // 逐条解压。zip crate 的 by_index 保证 name 是相对路径，
        // 但仍要防御性地拒绝越界路径（`..` / 绝对路径）。
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取归档条目失败: {}", e))?;
            let rel = match entry.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => {
                    return Err(format!("归档内存在非法路径: {}", entry.name()));
                }
            };
            let out = tmp.join(&rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)
                    .map_err(|e| format!("创建目录失败 {}: {}", out.display(), e))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败 {}: {}", parent.display(), e))?;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("读取条目内容失败 {}: {}", rel.display(), e))?;
            std::fs::write(&out, &buf)
                .map_err(|e| format!("写入文件失败 {}: {}", out.display(), e))?;
        }

        // 容器标记必须在归档根
        let marker_path = tmp.join("dspack.json");
        if !marker_path.is_file() {
            return Err("这不是 dspack 整合包（根目录缺少 dspack.json）".to_string());
        }
        let text = std::fs::read_to_string(&marker_path)
            .map_err(|e| format!("读取 dspack.json 失败: {}", e))?;
        let marker: DspackMarker = serde_json::from_str(&text)
            .map_err(|e| format!("dspack.json 格式非法: {}", e))?;

        if marker.format != "dspack" {
            return Err(format!(
                "这不是 dspack 整合包（format 为 \"{}\"）",
                marker.format
            ));
        }
        if !SUPPORTED_CONTAINER_VERSIONS.contains(&marker.version) {
            return Err(format!(
                "不支持的容器版本 v{}（本管理器支持 v2 / v3），请升级管理器",
                marker.version
            ));
        }

        Ok(marker)
    })();

    match result {
        Ok(marker) => Ok(Extracted { dir: tmp, marker }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// 读取解包目录内的 manifest.json 原文（解析留给下一步）。
pub fn read_manifest_text(dir: &Path) -> Result<String, String> {
    let p = dir.join("manifest.json");
    if !p.is_file() {
        return Err("整合包内缺少 manifest.json".to_string());
    }
    std::fs::read_to_string(&p).map_err(|e| format!("读取 manifest.json 失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用代码动态构造一个最小 `.dspack`（ZIP + dspack.json + manifest.json）。
    /// 只用于本地构造的测试，不依赖网络。
    fn build_min_dspack(dir: &Path, marker_json: &str, with_manifest: bool) -> PathBuf {
        let path = dir.join("test.dspack");
        let file = std::fs::File::create(&path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("dspack.json", opts).unwrap();
        use std::io::Write;
        zw.write_all(marker_json.as_bytes()).unwrap();
        if with_manifest {
            zw.start_file("manifest.json", opts).unwrap();
            zw.write_all(b"{\"manifestVersion\":5,\"type\":\"profile\"}").unwrap();
        }
        zw.finish().unwrap();
        path
    }

    #[test]
    fn rejects_non_zip() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nonzip");
        let _ = std::fs::create_dir_all(&tmp);
        let p = tmp.join("not-a-zip.dspack");
        std::fs::write(&p, b"hello world").unwrap();
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不是 dspack 整合包"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_missing_marker() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-nomarker");
        let _ = std::fs::create_dir_all(&tmp);
        // 一个合法 ZIP，但根没有 dspack.json
        let p = tmp.join("empty.dspack");
        let file = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("readme.txt", opts).unwrap();
        use std::io::Write;
        zw.write_all(b"hi").unwrap();
        zw.finish().unwrap();

        let err = extract(&p).unwrap_err();
        assert!(err.contains("缺少 dspack.json"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_unsupported_container_version() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-badver");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":9}"#, true);
        let err = extract(&p).unwrap_err();
        assert!(err.contains("不支持的容器版本 v9"), "实际错误: {}", err);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn accepts_v3_container() {
        let tmp = std::env::temp_dir().join("dsh-multiver-test-ok");
        let _ = std::fs::create_dir_all(&tmp);
        let p = build_min_dspack(&tmp, r#"{"format":"dspack","version":3}"#, true);
        let ex = extract(&p).unwrap();
        assert_eq!(ex.marker.version, 3);
        assert_eq!(ex.marker.format, "dspack");
        assert!(ex.dir.join("manifest.json").is_file());
        ex.cleanup();
        assert!(!ex.dir.exists(), "cleanup 后临时目录应被删除");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

