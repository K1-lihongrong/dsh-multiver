//! DSH 0.2.x 及以上的 v0.3 代际适配器。

use super::{
    standard_cli_entry, standard_profile_dir, AdapterCapabilities, DshAdapter, DshGeneration,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Copy)]
pub struct V03Adapter;

impl DshAdapter for V03Adapter {
    fn generation(&self) -> DshGeneration {
        DshGeneration::V03
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
        AdapterCapabilities::new(true, true)
    }
}
