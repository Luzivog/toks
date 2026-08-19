use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

const APP_DIR: &str = "toks";
const LEGACY_APP_DIR: &str = "tokscope";
static LEGACY_PROCESS_WARNING: Once = Once::new();

pub(super) fn resolved_fallback_root(parent: &Path) -> PathBuf {
    resolved_root(parent.join(".tokscope"), parent.join(".toks"))
}

pub(crate) fn resolved_named_root(parent: &Path) -> PathBuf {
    resolved_root(parent.join(LEGACY_APP_DIR), parent.join(APP_DIR))
}

pub(super) fn resolved_root(legacy: PathBuf, current: PathBuf) -> PathBuf {
    if !legacy.exists() {
        return current;
    }
    // Moving this directory beneath a live legacy process splits its open
    // SQLite handles from later path-based opens. Use one legacy root for this
    // run and retry the migration after the old process exits.
    if legacy_process_is_running() {
        LEGACY_PROCESS_WARNING.call_once(|| {
            eprintln!(
                "[toks] The previous app version is still running; using legacy storage for this run and deferring migration"
            );
        });
        return legacy;
    }
    if !current.exists() && fs::rename(&legacy, &current).is_ok() {
        return current;
    }
    if let Err(error) = merge_legacy_directory(&legacy, &current) {
        eprintln!(
            "[toks] Warning: could not fully migrate {} to {}: {error}",
            legacy.display(),
            current.display()
        );
    }
    current
}

fn legacy_process_is_running() -> bool {
    #[cfg(test)]
    if let Some(value) = std::env::var_os("TOKS_TEST_LEGACY_PROCESS_RUNNING") {
        return value != "0";
    }

    #[cfg(target_os = "linux")]
    {
        let current_pid = std::process::id().to_string();
        let Ok(processes) = fs::read_dir("/proc") else {
            return false;
        };
        for process in processes.flatten() {
            if process.file_name().to_string_lossy() == current_pid {
                continue;
            }
            let Ok(executable) = fs::read_link(process.path().join("exe")) else {
                continue;
            };
            if is_legacy_executable(&executable.to_string_lossy()) {
                return true;
            }
        }
    }
    false
}

pub(super) fn is_legacy_executable(executable: &str) -> bool {
    executable
        .strip_suffix(" (deleted)")
        .unwrap_or(executable)
        .rsplit(['/', '\\'])
        .next()
        == Some("tokscope")
}

fn merge_legacy_directory(legacy: &Path, current: &Path) -> std::io::Result<()> {
    if !legacy.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(current)?;
    for entry in fs::read_dir(legacy)? {
        let source = entry?.path();
        let destination = current.join(
            source
                .file_name()
                .expect("directory entries always have a file name"),
        );
        if !destination.exists() {
            match fs::rename(&source, &destination) {
                Ok(()) => continue,
                Err(_) if source.is_dir() => {
                    merge_legacy_directory(&source, &destination)?;
                    if source.read_dir()?.next().is_none() {
                        fs::remove_dir(&source)?;
                    }
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        if source.is_dir() && destination.is_dir() {
            merge_legacy_directory(&source, &destination)?;
            if source.read_dir()?.next().is_none() {
                fs::remove_dir(&source)?;
            }
        }
    }
    if legacy.read_dir()?.next().is_none() {
        fs::remove_dir(legacy)?;
    }
    Ok(())
}
