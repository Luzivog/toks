use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::http::LiveError;
use super::LimitIssueKind;

const STALE_AFTER: Duration = Duration::from_secs(60);

/// The two lock directories used by Claude Code 2.1.x while rotating OAuth
/// credentials. Holding both prevents either process from overwriting a token
/// rotation performed by the other.
pub(crate) struct ClaudeRefreshLock {
    paths: Vec<PathBuf>,
}

impl ClaudeRefreshLock {
    pub(crate) fn acquire(config_dir: &Path) -> Result<Self, LiveError> {
        let canonical = fs::canonicalize(config_dir).unwrap_or_else(|_| config_dir.to_path_buf());
        let mut legacy = canonical.as_os_str().to_os_string();
        legacy.push(".lock");
        let paths = vec![
            config_dir.join(".oauth_refresh.lock"),
            PathBuf::from(legacy),
        ];
        let mut acquired = Vec::with_capacity(paths.len());
        for path in paths {
            if let Err(error) = acquire_directory(&path) {
                for held in acquired.iter().rev() {
                    let _ = fs::remove_dir(held);
                }
                return Err(error);
            }
            acquired.push(path);
        }
        Ok(Self { paths: acquired })
    }
}

impl Drop for ClaudeRefreshLock {
    fn drop(&mut self) {
        for path in self.paths.iter().rev() {
            let _ = fs::remove_dir(path);
        }
    }
}

fn acquire_directory(path: &Path) -> Result<(), LiveError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && stale(path) => {
            fs::remove_dir(path)
                .and_then(|()| fs::create_dir(path))
                .map_err(lock_error)
        }
        Err(error) => Err(lock_error(error)),
    }
}

fn stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > STALE_AFTER)
}

fn lock_error(_error: std::io::Error) -> LiveError {
    LiveError::new(
        LimitIssueKind::Network,
        "Claude credentials are already being refreshed",
    )
}
