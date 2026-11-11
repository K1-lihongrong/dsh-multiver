use crate::config::Config;
use std::path::Path;

/// 安装完成账本：GUI 与 CLI 共用。
/// 只负责根据安装结果更新 broken_versions 并保存配置，不参与安装过程本身。
pub fn finish_install(
    cfg: &mut Config,
    manager_dir: &Path,
    version: &str,
    ok: bool,
    message: &str,
) -> bool {
    let kind = if ok {
        ""
    } else {
        crate::versions::classify_error(message)
    };
    let changed = cfg.apply_install_result(version, ok, kind);
    if changed {
        let _ = cfg.save(manager_dir);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_install_marks_and_clears_broken() {
        let base = std::env::temp_dir().join(format!("dsh-install-service-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let mut cfg = Config::default();
        assert!(finish_install(
            &mut cfg,
            &base,
            "0.1.0",
            false,
            "ERR_PNPM_FETCH_404 @deepseek-ai/pkg"
        ));
        assert!(cfg.broken_versions.contains(&"0.1.0".to_string()));
        assert!(finish_install(&mut cfg, &base, "0.1.0", true, "ok"));
        assert!(!cfg.broken_versions.contains(&"0.1.0".to_string()));
        let _ = std::fs::remove_dir_all(&base);
    }
}
