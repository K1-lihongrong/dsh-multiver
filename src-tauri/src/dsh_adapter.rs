use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshGeneration {
    V01,
    V03,
}

pub trait DshAdapter: Send + Sync {
    fn generation(&self) -> DshGeneration;
    fn supports_allow_version(&self) -> bool;
    fn profile_dir(&self, home: &std::path::Path, profile: &str) -> PathBuf;
}

pub struct V01Adapter;
pub struct V03Adapter;

impl DshAdapter for V01Adapter {
    fn generation(&self) -> DshGeneration {
        DshGeneration::V01
    }
    fn supports_allow_version(&self) -> bool {
        false
    }
    fn profile_dir(&self, home: &std::path::Path, profile: &str) -> PathBuf {
        home.join("profiles").join(profile)
    }
}

impl DshAdapter for V03Adapter {
    fn generation(&self) -> DshGeneration {
        DshGeneration::V03
    }
    fn supports_allow_version(&self) -> bool {
        true
    }
    fn profile_dir(&self, home: &std::path::Path, profile: &str) -> PathBuf {
        home.join("profiles").join(profile)
    }
}

pub fn detect(version: &str) -> DshGeneration {
    let core = version.trim().trim_start_matches('v');
    let mut parts = core.split('.').filter_map(|p| p.parse::<u64>().ok());
    match (parts.next(), parts.next()) {
        (Some(0), Some(0)) | (Some(0), Some(1)) => DshGeneration::V01,
        _ => DshGeneration::V03,
    }
}

pub fn adapter_for(version: &str) -> Box<dyn DshAdapter> {
    match detect(version) {
        DshGeneration::V01 => Box::new(V01Adapter),
        DshGeneration::V03 => Box::new(V03Adapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_generations() {
        assert_eq!(detect("0.0.1-rc.5"), DshGeneration::V01);
        assert_eq!(detect("0.1.7-rc.2"), DshGeneration::V01);
        assert_eq!(detect("0.2.2-rc.1"), DshGeneration::V03);
        assert_eq!(detect("garbage"), DshGeneration::V03);
    }

    #[test]
    fn allow_version_only_for_v03() {
        assert!(!adapter_for("0.1.7-rc.2").supports_allow_version());
        assert!(adapter_for("0.2.2-rc.1").supports_allow_version());
    }
}
