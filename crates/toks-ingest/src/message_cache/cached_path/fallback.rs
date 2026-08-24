use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct CachedPath(String);

#[cfg(not(any(unix, windows)))]
impl CachedPath {
    pub(crate) fn from_path(path: &Path) -> Self {
        Self(path.to_string_lossy().into_owned())
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }

    pub(in crate::message_cache) fn update_digest(&self, hasher: &mut Sha256) {
        hasher.update(self.0.as_bytes());
    }
}
