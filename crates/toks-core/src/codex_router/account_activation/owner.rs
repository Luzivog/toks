use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProcessOwner {
    pid: u32,
    start_ticks: u64,
}

impl ProcessOwner {
    pub(super) fn current() -> Option<Self> {
        Self::read(std::process::id(), Path::new("/proc"))
    }

    pub(super) fn is_alive(self) -> bool {
        Self::read(self.pid, Path::new("/proc")) == Some(self)
    }

    fn read(pid: u32, proc_root: &Path) -> Option<Self> {
        let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
        let tail = stat.rsplit_once(')')?.1;
        let start_ticks = tail.split_whitespace().nth(19)?.parse().ok()?;
        Some(Self { pid, start_ticks })
    }

    #[cfg(test)]
    pub(super) fn missing_for_test() -> Self {
        Self {
            pid: u32::MAX,
            start_ticks: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProcessOwner;

    #[test]
    fn owner_identity_rejects_missing_or_reused_processes() {
        assert!(ProcessOwner::current().unwrap().is_alive());
        assert!(!ProcessOwner::missing_for_test().is_alive());
    }
}
