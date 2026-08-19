use std::ffi::{OsStr, OsString};

pub(crate) struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    pub(crate) fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }

    pub(crate) fn set(&mut self, key: &str, value: impl AsRef<OsStr>) {
        unsafe { std::env::set_var(key, value) };
    }

    pub(crate) fn remove(&mut self, key: &str) {
        unsafe { std::env::remove_var(key) };
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, previous) in self.0.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

/// Encode a path as a complete JSON string literal for hand-built fixtures.
pub(crate) fn json_path_literal(path: &std::path::Path) -> String {
    serde_json::to_string(&path.to_string_lossy()).expect("a string always serializes to JSON")
}
