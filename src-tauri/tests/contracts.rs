use dsh_multiver_lib::contract_api::{
    classify_error, decode_console, list_remote, parse_remote_versions, ProgressParser,
};

/// 用固定 pnpm 输出驱动生产解析器，锁住「输出格式 → 阶段/进度」契约。
#[test]
fn fake_pnpm_progress_output_contract() {
    let mut parser = ProgressParser::new();

    assert!(parser.parse("Packages: +445").is_none());

    let (step, detail, frac) = parser
        .parse(".../profiles/web  | Progress: resolved 234, reused 222, downloaded 0, added 0")
        .expect("resolved 行应识别");
    assert_eq!(step, 4);
    assert!(detail.contains("234"));
    assert_eq!(frac, 0.0);

    let (step, detail, frac) = parser
        .parse("Progress: resolved 234, reused 222, downloaded 5, added 0")
        .expect("downloaded 行应识别");
    assert_eq!(step, 5);
    assert!(detail.contains('5'));
    assert!((frac - 5.0 / 445.0).abs() < 0.001);

    let (step, detail, frac) = parser
        .parse("Progress: resolved 234, reused 222, downloaded 5, added 12")
        .expect("added 行应识别");
    assert_eq!(step, 6);
    assert!(detail.contains("12"));
    assert!((frac - 12.0 / 445.0).abs() < 0.001);

    let (step, _, _) = parser
        .parse("Done in 47.6s using pnpm v11")
        .expect("Done 行应识别");
    assert_eq!(step, 8);
}

/// pnpm 错误文本是外部契约；分类结果直接决定前端提示与处置建议。
#[test]
fn fake_pnpm_error_classification_contract() {
    assert_eq!(
        classify_error("ERR_PNPM_NO_MATCHING_VERSION No matching version found"),
        "incomplete-version"
    );
    assert_eq!(
        classify_error("ERR_PNPM_FETCH_404 GET https://registry.npmjs.org/@deepseek-ai/dsh"),
        "private-package"
    );
    assert_eq!(classify_error("connect ETIMEDOUT 1.2.3.4:443"), "network");
}

/// `pnpm view --json` 三种实际输出形态都要能解析，且保持「最新在前」。
#[test]
fn fake_pnpm_version_output_contract() {
    let plain = parse_remote_versions(r#"["0.1.0", "0.2.0", "0.3.0"]"#);
    assert_eq!(plain, vec!["0.3.0", "0.2.0", "0.1.0"]);

    let nested = parse_remote_versions(r#"[["0.1.0", "0.2.0"]]"#);
    assert_eq!(nested, vec!["0.2.0", "0.1.0"]);

    let lines = parse_remote_versions("[\n  \"0.1.0\",\n  \"0.2.0\"\n]\n");
    assert_eq!(lines, vec!["0.2.0", "0.1.0"]);
}

/// 真实 GBK 字节必须经生产解码器还原，不允许出现 U+FFFD 或整行丢失。
#[test]
fn real_gbk_bytes_decode_contract() {
    // "安装失败" 的 GBK 字节
    let gbk: &[u8] = &[0xB0, 0xB2, 0xD7, 0xB0, 0xCA, 0xA7, 0xB0, 0xDC];
    let got = decode_console(gbk);
    assert_eq!(got, "安装失败");
    assert!(!got.contains('\u{FFFD}'));
}

/// 真实 pnpm 输出对拍（联网；只在独立 job 或手动 `--ignored` 时运行）。
#[test]
#[ignore = "需要网络与 pnpm；在 contracts job 中显式运行"]
fn real_pnpm_view_contract() {
    let root =
        std::env::temp_dir().join(format!("dsh-multiver-contract-pnpm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let (ok, versions, err) = list_remote(
        &root.join("store"),
        &root.join("cache"),
        &root.join("state"),
    );
    assert!(ok, "pnpm view 失败：{err}");
    assert!(!versions.is_empty(), "pnpm view 返回空版本列表");
    assert!(
        versions
            .iter()
            .all(|v| !v.trim().is_empty() && !v.contains('"')),
        "版本号解析残留了 JSON 字符：{:?}",
        versions
    );

    let _ = std::fs::remove_dir_all(&root);
}
