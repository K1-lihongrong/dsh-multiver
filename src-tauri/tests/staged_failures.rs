use dsh_multiver_lib::contract_api::{
    pending_path, save_pending, stage_dir, staged_exe, take_over_pending_at, Pending,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "dsh-multiver-staged-{}-{}-{}",
        name,
        std::process::id(),
        stamp
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn pending_for(staged: &Path, version: &str) -> Pending {
    let bytes = fs::read(staged).unwrap();
    Pending {
        version: version.into(),
        sha256: sha256(&bytes),
        staged_at: 0,
        size: bytes.len() as u64,
    }
}

fn prepare(root: &Path, name: &str) -> (PathBuf, Pending) {
    let staged = staged_exe(root);
    fs::create_dir_all(stage_dir(root)).unwrap();
    fs::write(&staged, b"new exe bytes").unwrap();
    let p = pending_for(&staged, name);
    save_pending(root, &p).unwrap();
    (staged, p)
}

/// 目标 exe 缺失（上次替换与回滚都失败的残留态）时必须中止，且保留 .old。
#[test]
fn target_missing_keeps_only_usable_backup() {
    let root = tmp("target-missing");
    let (staged, p) = prepare(&root, "9.9.9");
    let target = root.join("no-such-app.exe");
    let backup = target.with_extension("exe.old");
    fs::write(&backup, b"old but alive").unwrap();

    assert!(take_over_pending_at(&root, &p, &target).is_none());
    assert_eq!(fs::read(&backup).unwrap(), b"old but alive");
    assert!(!target.exists());
    assert!(staged.exists(), "暂存应保留，等用户修复后再试");
    assert!(pending_path(&root).exists(), "标记应保留");
    let _ = fs::remove_dir_all(&root);
}

/// rename 目标失败（模拟目标被占用 / 杀软拦截）时，原 exe、暂存、标记都必须保留。
#[test]
fn target_rename_failure_preserves_everything() {
    let root = tmp("rename-failure");
    let (staged, p) = prepare(&root, "9.9.9");
    let target = root.join("app.exe");
    fs::write(&target, b"old exe").unwrap();

    // 备份路径被一个非空目录占住：remove_file 失败且 rename(file → dir) 失败。
    let backup = target.with_extension("exe.old");
    fs::create_dir_all(&backup).unwrap();
    fs::write(backup.join("blocker"), b"x").unwrap();

    assert!(take_over_pending_at(&root, &p, &target).is_none());
    assert_eq!(fs::read(&target).unwrap(), b"old exe");
    assert!(backup.is_dir());
    assert!(staged.exists(), "暂存应保留，下次启动重试");
    assert!(pending_path(&root).exists(), "标记应保留");
    let _ = fs::remove_dir_all(&root);
}

/// 放入新 exe 失败（模拟磁盘满 / 写入失败）时必须回滚，旧 exe 仍可启动。
#[test]
fn placing_new_exe_failure_rolls_back_old_exe() {
    let root = tmp("rollback");
    let (staged, p) = prepare(&root, "9.9.9");
    let target = root.join("app.exe");
    fs::write(&target, b"old exe").unwrap();
    let backup = target.with_extension("exe.old");

    // 暂存文件在进入替换流程前消失，使第二步 rename(staged → target) 失败。
    fs::remove_file(&staged).unwrap();

    assert!(take_over_pending_at(&root, &p, &target).is_none());
    assert_eq!(fs::read(&target).unwrap(), b"old exe", "回滚后必须是旧 exe");
    assert!(!backup.exists(), "回滚后备份已移回，不应残留");
    assert!(pending_path(&root).exists(), "标记保留，等待下次重试");
    let _ = fs::remove_dir_all(&root);
}

/// 成功路径对照组：正常替换后消费暂存与标记。
#[test]
fn success_path_consumes_stage_and_pending() {
    let root = tmp("success");
    let (staged, p) = prepare(&root, "9.9.9");
    let target = root.join("app.exe");
    fs::write(&target, b"old exe").unwrap();
    let backup = target.with_extension("exe.old");

    assert_eq!(
        take_over_pending_at(&root, &p, &target).as_deref(),
        Some("9.9.9")
    );
    assert_eq!(fs::read(&target).unwrap(), b"new exe bytes");
    assert!(!staged.exists(), "暂存已被消费");
    assert!(!pending_path(&root).exists(), "标记已消费");
    assert!(!backup.exists(), "成功路径应尽力清理备份");
    let _ = fs::remove_dir_all(&root);
}
