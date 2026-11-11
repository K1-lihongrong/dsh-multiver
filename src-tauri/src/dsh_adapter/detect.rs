//! DSH 版本识别。
//!
//! 只识别已知行为代际；空、垃圾或无法解析的版本返回 `Unknown`，由未知适配器
//! 走保守默认，而不是误判成新版。

use super::DshGeneration;

/// 识别 DSH 版本代际。
///
/// - `0.0.x` / `0.1.x` → V01
/// - `0.2.x` 及以上 0.x → V03
/// - 空、垃圾、缺失 major/minor 或未来 major → Unknown
pub fn detect(version: &str) -> DshGeneration {
    let core = version.trim();
    let core = core
        .strip_prefix('v')
        .or_else(|| core.strip_prefix('V'))
        .unwrap_or(core);
    let mut parts = core.split('.');
    let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
        return DshGeneration::Unknown;
    };
    if !is_ascii_number(major) || !is_ascii_number(minor) {
        return DshGeneration::Unknown;
    }
    let Ok(major) = major.parse::<u64>() else {
        return DshGeneration::Unknown;
    };
    let Ok(minor) = minor.parse::<u64>() else {
        return DshGeneration::Unknown;
    };
    match (major, minor) {
        (0, 0) | (0, 1) => DshGeneration::V01,
        (0, minor) if minor >= 2 => DshGeneration::V03,
        _ => DshGeneration::Unknown,
    }
}

fn is_ascii_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_v01_and_v03() {
        assert_eq!(detect("0.0.1-rc.5"), DshGeneration::V01);
        assert_eq!(detect("0.1.7-rc.2"), DshGeneration::V01);
        assert_eq!(detect("0.2.2-rc.1"), DshGeneration::V03);
        assert_eq!(detect("  V0.3.0  "), DshGeneration::V03);
    }

    #[test]
    fn unknown_is_safe_default() {
        for version in ["", "   ", "garbage", "0", "0.x.1", "1.0.0"] {
            assert_eq!(detect(version), DshGeneration::Unknown, "{version:?}");
        }
    }
}
