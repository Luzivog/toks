use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::clients::ClientId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrushDbSource {
    pub db_path: PathBuf,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
}

/// Result of scanning all session directories.
#[derive(Debug)]
pub struct ScanResult {
    pub files: [Vec<PathBuf>; ClientId::COUNT],
    /// OpenCode databases, including default and channel-specific files.
    pub opencode_dbs: Vec<PathBuf>,
    pub copilot_desktop_db: Option<PathBuf>,
    pub synthetic_db: Option<PathBuf>,
    pub kilo_db: Option<PathBuf>,
    pub hermes_db: Option<PathBuf>,
    pub goose_db: Option<PathBuf>,
    pub zed_db: Option<PathBuf>,
    pub kiro_db: Option<PathBuf>,
    pub crush_dbs: Vec<CrushDbSource>,
    /// ZCode v2 CLI usage database at `~/.zcode/cli/db/db.sqlite`.
    pub zcode_db: Option<PathBuf>,
    pub micode_dbs: Vec<PathBuf>,
    /// OpenCode legacy JSON directory, used for migration cache stat checks.
    pub opencode_json_dir: Option<PathBuf>,
    /// Devin CLI databases from the default and configured scan roots.
    pub devin_dbs: Vec<PathBuf>,
    /// VS Code Copilot chat sessions under `workspaceStorage`.
    pub copilot_vscode_sessions: Vec<PathBuf>,
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            files: std::array::from_fn(|_| Vec::new()),
            opencode_dbs: Vec::new(),
            copilot_desktop_db: None,
            synthetic_db: None,
            kilo_db: None,
            hermes_db: None,
            goose_db: None,
            zed_db: None,
            kiro_db: None,
            crush_dbs: Vec::new(),
            zcode_db: None,
            micode_dbs: Vec::new(),
            opencode_json_dir: None,
            devin_dbs: Vec::new(),
            copilot_vscode_sessions: Vec::new(),
        }
    }
}

impl ScanResult {
    pub fn get(&self, client: ClientId) -> &Vec<PathBuf> {
        &self.files[client as usize]
    }

    pub fn get_mut(&mut self, client: ClientId) -> &mut Vec<PathBuf> {
        &mut self.files[client as usize]
    }

    pub fn total_files(&self) -> usize {
        self.files.iter().map(Vec::len).sum()
    }

    pub fn all_files(&self) -> Vec<(ClientId, PathBuf)> {
        let mut result = Vec::with_capacity(self.total_files());
        for client in ClientId::iter() {
            for path in self.get(client) {
                result.push((client, path.clone()));
            }
        }
        result
    }

    /// Return every Hermes database, with canonical paths deduplicated.
    pub fn hermes_db_paths(&self) -> Vec<PathBuf> {
        dedup_primary_and_extra(self.hermes_db.as_ref(), self.get(ClientId::Hermes))
    }

    /// Return every Zed threads database, with canonical paths deduplicated.
    pub fn zed_db_paths(&self) -> Vec<PathBuf> {
        dedup_primary_and_extra(self.zed_db.as_ref(), self.get(ClientId::Zed))
    }
}

fn dedup_primary_and_extra(primary: Option<&PathBuf>, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |path: &Path| {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if seen.insert(key) {
            paths.push(path.to_path_buf());
        }
    };

    if let Some(path) = primary {
        push(path);
    }
    for path in extra {
        push(path);
    }
    paths
}
