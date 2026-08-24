use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// V2 intentionally starts cold and leaves source-message-cache.bin untouched:
// the monolith did not record a trustworthy parser owner for migration.
const CACHE_SHARD_DIRNAME: &str = "source-message-cache-v2";
const CACHE_LOCK_FILENAME: &str = "source-message-cache.lock";

fn cache_dir() -> Option<PathBuf> {
    if crate::paths::is_config_dir_overridden()
        || dirs::config_dir().is_some()
        || cfg!(target_os = "macos") && crate::paths::home_dir().is_some()
    {
        Some(crate::paths::get_cache_dir())
    } else {
        fallback_cache_dir()
    }
}

pub(super) fn cache_shard_dir() -> Option<PathBuf> {
    Some(cache_dir()?.join(CACHE_SHARD_DIRNAME))
}

pub(super) fn cache_lock_path() -> Option<PathBuf> {
    Some(cache_dir()?.join(CACHE_LOCK_FILENAME))
}

pub(super) fn fallback_cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|path| path.join("tokscope"))
        .or_else(user_scoped_temp_dir)
}

#[cfg(unix)]
fn user_scoped_temp_dir() -> Option<PathBuf> {
    let uid = unsafe { libc::geteuid() };
    Some(std::env::temp_dir().join(format!("tokscope-uid-{uid}")))
}

#[cfg(not(unix))]
fn user_scoped_temp_dir() -> Option<PathBuf> {
    std::env::var_os("USERNAME")
        .or_else(|| std::env::var_os("USER"))
        .map(|user| {
            let mut path = std::env::temp_dir();
            path.push(format!("tokscope-user-{}", user.to_string_lossy()));
            path
        })
}

pub(super) fn ensure_cache_dir(dir: &Path) -> std::io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(dir) {
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(std::io::Error::other(
                "cache directory is not a real directory",
            ));
        }
    }
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

static WARNED_CONTEXTS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();

fn warned_contexts() -> &'static Mutex<HashSet<&'static str>> {
    WARNED_CONTEXTS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) fn warn_cache_failure_once(
    context: &'static str,
    path: &Path,
    error: &impl std::fmt::Display,
) {
    warn_cache_failure_once_in(warned_contexts(), context, path, error);
}

/// The once-only set is a parameter purely so the poisoned-set regression test
/// can supply its own. Mutex poisoning is irreversible, so a test that poisoned
/// the process-global set would leave every later test in the binary depending
/// on the very recovery it is checking. Production has exactly one caller and
/// it always passes `warned_contexts()`, so the once-per-process,
/// once-per-context semantics are unchanged.
pub(super) fn warn_cache_failure_once_in(
    warned: &Mutex<HashSet<&'static str>>,
    context: &'static str,
    path: &Path,
    error: &impl std::fmt::Display,
) {
    tracing::warn!(path = %path.display(), %error, %context, "source message cache failure");

    // Most non-TUI commands (including `submit`) do not install a tracing
    // subscriber. Surface persistence failures directly once per process so a
    // permanently cold cache can never fail silently again. The TUI owns raw
    // mode and the alternate screen for its whole run, so a raw stdio write
    // there corrupts the rendered display. Defer that fallback until the TUI
    // restores the terminal instead of consuming the once-only warning while
    // leaving the user with no visible diagnostic (#941).
    // Recover from a poisoned set the way tui_signal does: an unrelated panic
    // elsewhere must not be what silences the diagnostic this block exists to
    // guarantee. The set only tracks which contexts were already reported, so
    // its contents stay meaningful across an unwind.
    if warned
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(context)
    {
        crate::tui_signal::emit_or_defer_stderr(format!(
            "toks: warning: {context} ({}): {error}",
            path.display()
        ));
    }
}
