use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::clients::ClientId;

/// Persistent scanner settings from the `scanner` key in settings.json.
///
/// `#[serde(default)]` keeps older files without this key compatible.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScannerSettings {
    /// Extra OpenCode databases outside its default data directory.
    /// Missing files and SQLite sidecars are ignored during discovery.
    #[serde(default)]
    pub opencode_db_paths: Vec<PathBuf>,
    /// Additional roots keyed by public client id.
    #[serde(default)]
    pub extra_scan_paths: BTreeMap<String, Vec<PathBuf>>,
    /// IANA timezone used to keep local usage-day buckets stable.
    ///
    /// `None` preserves the historical `chrono::Local` behavior. Invalid names
    /// also degrade to unpinned behavior elsewhere in the ingest pipeline.
    #[serde(default)]
    pub bucket_timezone: Option<String>,
}

/// Parse comma-separated `client:path` entries from the extra-dirs setting.
pub fn parse_extra_dirs(value: &str, enabled: &HashSet<ClientId>) -> Vec<(ClientId, String)> {
    if value.is_empty() {
        return Vec::new();
    }

    value
        .split(',')
        .filter_map(|entry| {
            let (client, path) = entry.trim().split_once(':')?;
            let client_id = ClientId::from_str(client.trim())?;
            if !enabled.contains(&client_id) || !supports_extra_dir_scanning(client_id) {
                return None;
            }
            let path = path.trim().to_string();
            (!path.is_empty()).then_some((client_id, path))
        })
        .collect()
}

pub fn extra_scan_paths_for(
    settings: &ScannerSettings,
    enabled: &HashSet<ClientId>,
) -> Vec<(ClientId, PathBuf)> {
    settings
        .extra_scan_paths
        .iter()
        .filter_map(|(client, paths)| {
            let client_id = ClientId::from_str(client)?;
            if !enabled.contains(&client_id) || !supports_extra_dir_scanning(client_id) {
                return None;
            }
            Some(
                paths
                    .iter()
                    .filter(|path| !path.as_os_str().is_empty())
                    .cloned()
                    .map(move |path| (client_id, path)),
            )
        })
        .flatten()
        .collect()
}

pub(super) fn supports_extra_dir_scanning(client_id: ClientId) -> bool {
    // These clients use dedicated database or multi-root discovery.
    !matches!(
        client_id,
        ClientId::Kilo | ClientId::Crush | ClientId::Goose
    )
}
