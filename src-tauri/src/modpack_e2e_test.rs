//! 临时：在真实测试根上执行端到端导入（验证后删除）。
//! 测试根：E:/.LLM-AGENTS/deepseek-harness

#![cfg(test)]

use crate::modpack::*;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from("E:/.LLM-AGENTS/deepseek-harness")
}

#[test]
#[ignore] // 需手动跑：cargo test --lib modpack_e2e -- --ignored --nocapture
fn e2e_import_pokemon() {
    let r = root();
    let versions = r.join("versions");
    let store = r.join("store");
    let cache = r.join("cache");
    let state = r.join("state");

    let pack = PathBuf::from("E:/.LLM-AGENTS/dspack-fixtures/pokemon-1.0.0.dspack");
    let ex = extract(&pack).expect("解包失败");
    let text = read_manifest_text(&ex.dir).expect("读 manifest 失败");
    let m = Manifest::parse(&text).expect("解析失败");
    println!("包：{}@{} type={} dsh={:?}", m.name.clone().unwrap(), m.version.clone().unwrap(), m.kind, m.dsh_version);
    println!("bundles={:?}", m.bundles);
    println!("dependencies={:?}", m.dependencies);

    let idirs = ImportDirs {
        versions_dir: &versions,
        modpacks_dir: &versions, // 测试里统一用 versions 目录
        store_dir: &store,
        cache_dir: &cache,
        state_dir: &state,
    };
    let name = instance_dir_name(&m, None);
    // 注意：pokemon 声明的 0.1.5-alpha.2 是官方残缺版本（子包 dsh-tools 缺失），
    // 任何工具都装不了。为验证导入链路本身，改用本机可用的 0.1.7-rc.2。
    let declared = m.dsh_version.clone().unwrap_or_default();
    let dsh_version = "0.1.7-rc.2".to_string();
    println!("实例名：{}，包声明 dsh={}，实测用 {}", name, declared, dsh_version);

    let on_stage = |s: &str| println!("  [stage] {}", s);
    let ensure = |_v: &str| Ok(());
    let outcome = import(&ex, &m, &idirs, &name, &dsh_version, &ensure, &on_stage, None);

    match outcome {
        Ok(o) => {
            println!("导入成功：{}", o.instance_dir.display());
            println!("跳过条目：{:?}", o.skipped);
            assert!(o.instance_dir.join("node_modules").exists(), "应有 node_modules");
            assert!(o.instance_dir.join(".npmrc").exists(), "应有 .npmrc");
            assert!(o.instance_dir.join("home").exists(), "应有 home");
            // 写出元数据
            let meta = build_meta(&m, &o);
            write_meta(&o.instance_dir, &meta).expect("写元数据失败");
            println!("元数据：{}", serde_json::to_string_pretty(&meta).unwrap());
        }
        Err(e) => {
            println!("导入失败：{}", e);
            panic!("导入应成功");
        }
    }
    ex.cleanup();
}

#[test]
#[ignore]
fn e2e_rollback_on_bad_version() {
    // 用一个不存在的 dsh 版本，应在阶段 2 失败并回滚（实例目录被删）
    let r = root();
    let versions = r.join("versions");
    let store = r.join("store");
    let cache = r.join("cache");
    let state = r.join("state");

    let pack = PathBuf::from("E:/.LLM-AGENTS/dspack-fixtures/pokemon-1.0.0.dspack");
    let ex = extract(&pack).expect("解包失败");
    let text = read_manifest_text(&ex.dir).expect("读 manifest 失败");
    let m = Manifest::parse(&text).expect("解析失败");

    let idirs = ImportDirs {
        versions_dir: &versions,
        modpacks_dir: &versions, // 测试里统一用 versions 目录
        store_dir: &store,
        cache_dir: &cache,
        state_dir: &state,
    };
    let name = "modpack-rollback-test";
    // 先清掉可能的残留（否则 import 会在入口因「目录已存在」直接返回，不进入阶段）
    let _ = std::fs::remove_dir_all(versions.join(name));
    let on_stage = |_: &str| {};
    let ensure = |_v: &str| Ok(());

    let res = import(&ex, &m, &idirs, name, "0.0.0-nonexistent", &ensure, &on_stage, None);
    let err = res.expect_err("不存在的 dsh 版本应导致失败");
    println!("失败信息：{}", err);
    let exists = versions.join(name).exists();
    if exists {
        println!("目录仍存在，内容：");
        if let Ok(rd) = std::fs::read_dir(versions.join(name)) {
            for e in rd.flatten() {
                println!("  - {}", e.file_name().to_string_lossy());
            }
        }
    }
    assert!(!exists, "失败后实例目录应被删除（回滚）");
    println!("回滚验证通过");
    ex.cleanup();
}
