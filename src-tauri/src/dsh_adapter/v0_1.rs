//! DSH 0.1.x 代际适配器。

use super::{
    standard_cli_entry, standard_profile_dir, AdapterCapabilities, DshAdapter, DshGeneration,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Copy)]
pub struct V01Adapter;

impl DshAdapter for V01Adapter {
    fn generation(&self) -> DshGeneration {
        DshGeneration::V01
    }

    fn supports_allow_version(&self) -> bool {
        self.capabilities().supports_allow_version()
    }

    fn profile_dir(&self, home: &Path, profile: &str) -> PathBuf {
        standard_profile_dir(home, profile)
    }

    fn cli_entry(&self, version_dir: &Path) -> PathBuf {
        standard_cli_entry(version_dir)
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::new(false, true)
    }
}
