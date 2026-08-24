use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

// Some diagnostics (e.g. cache save failures) fall back to a direct
// eprintln! when no tracing subscriber is guaranteed to be installed, so
// non-TUI commands still surface them. The TUI owns raw mode and the
// crossterm alternate screen for its whole lifetime, and a stray stdio
// write there corrupts the rendered display instead of being visible as a
// normal log line. Diagnostics routed through this module are held until the
// TUI releases the terminal, then written once the normal screen is restored.
static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEFERRED_STDERR: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn deferred_stderr() -> &'static Mutex<Vec<String>> {
    DEFERRED_STDERR.get_or_init(|| Mutex::new(Vec::new()))
}

pub(super) fn transition_tui_active(active: bool) -> Vec<String> {
    // Coordinate the state transition with routing a diagnostic. Without the
    // shared lock, a writer could observe active, lose a race with the flush,
    // and enqueue a message after the queue had already been drained.
    let mut deferred = deferred_stderr()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    TUI_ACTIVE.store(active, Ordering::Relaxed);

    if active {
        Vec::new()
    } else {
        std::mem::take(&mut *deferred)
    }
}

pub fn set_tui_active(active: bool) {
    for message in transition_tui_active(active) {
        eprintln!("{message}");
    }
}

pub fn is_tui_active() -> bool {
    TUI_ACTIVE.load(Ordering::Relaxed)
}

pub(super) fn route_stderr(message: String) -> Option<String> {
    let mut deferred = deferred_stderr()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if is_tui_active() {
        deferred.push(message);
        None
    } else {
        Some(message)
    }
}

pub(crate) fn emit_or_defer_stderr(message: String) {
    if let Some(message) = route_stderr(message) {
        eprintln!("{message}");
    }
}

#[cfg(test)]
pub(crate) fn take_deferred_stderr_for_test() -> Vec<String> {
    let mut deferred = deferred_stderr()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *deferred)
}

/// Sets `TUI_ACTIVE` for the duration of a test and restores the previous
/// value on `Drop`, so an unwind restores it too.
///
/// `TUI_ACTIVE` and the deferred queue behind it are process-global. Restoring
/// them by hand just before a test's final assertion leaks the mutation into
/// every later test in the binary if anything in between panics, and the next
/// test to run then defers diagnostics it expected to see on stderr — a
/// failure with nothing to do with what it asserts. This mirrors
/// `paths::test_env::EnvGuard`, which exists for the same reason.
///
/// `Drop` restores through `transition_tui_active`, which makes the queue
/// follow the state being restored. Restoring to inactive drains whatever the
/// test deferred: restoring through `set_tui_active` would instead `eprintln!`
/// the test's synthetic diagnostics onto the real stderr, and leaving them
/// queued would surface in the next test's `take_deferred_stderr_for_test`.
/// Restoring to *active* leaves the queue alone, because an enclosing active
/// scope still owns the terminal and its deferred diagnostics are owed to the
/// eventual restore, not to this guard.
#[cfg(test)]
pub(crate) struct TuiActiveGuard {
    pub(super) previous: bool,
}

#[cfg(test)]
impl TuiActiveGuard {
    pub(crate) fn capture() -> Self {
        Self {
            previous: is_tui_active(),
        }
    }

    /// Takes `&mut self` for the same reason `EnvGuard::set` does: it reads
    /// correctly for a method whose whole purpose is to mutate process-global
    /// state, and it keeps the guard's owner from being aliased away.
    pub(crate) fn set(&mut self, active: bool) {
        let _discarded = transition_tui_active(active);
    }
}

#[cfg(test)]
impl Drop for TuiActiveGuard {
    fn drop(&mut self) {
        // One transition, so the queue's fate matches the state actually being
        // restored. Draining unconditionally first would discard diagnostics an
        // enclosing active scope had deferred and still owes its user, turning
        // "restore the previous value" into a silent teardown of state this
        // guard never owned.
        let _discarded = transition_tui_active(self.previous);
    }
}
