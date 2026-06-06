use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalStorageConfig {
    pub base_path: PathBuf,
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("/var/lib/aether/local_storage/"),
        }
    }
}
