use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::sessions::codex::CodexParseState;

use super::types::{SourceKey, SourceKind};

mod scheduling;

const KEY_BYTES: usize = 32;
const KEY_FILE: &str = "accounting-source.key";
const LOCK_FILE: &str = "accounting-checkpoints.lock";
const STATE_FILE: &str = "accounting-checkpoints-v1.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SampleDigest {
    pub offset: u64,
    pub len: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct StoredCheckpoint {
    pub kind: SourceKind,
    pub parser_version: u32,
    pub committed_offset: u64,
    pub source_size: u64,
    pub modified_ns: u64,
    pub content_hash: [u8; 32],
    pub prefix_samples: Vec<SampleDigest>,
    pub codex_state: Option<CodexParseState>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct CheckpointFile {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    sources: BTreeMap<String, StoredCheckpoint>,
    #[serde(default)]
    rotation_cursor: Option<String>,
}

const fn schema_version() -> u32 {
    1
}

pub(crate) struct CheckpointStore {
    directory: PathBuf,
    _lock: fs::File,
    key: [u8; KEY_BYTES],
    state: CheckpointFile,
}

impl CheckpointStore {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        ensure_private_directory(&directory)?;
        let lock = super::lock::acquire(&directory.join(LOCK_FILE))?;
        let key = load_or_create_key(&directory.join(KEY_FILE))?;
        let state = load_state(&directory.join(STATE_FILE))?;
        if state.schema_version != schema_version() {
            return Err("unsupported accounting checkpoint schema".to_string());
        }
        Ok(Self {
            directory,
            _lock: lock,
            key,
            state,
        })
    }

    pub fn key(&self) -> &[u8; KEY_BYTES] {
        &self.key
    }

    pub fn get(&self, source: &SourceKey) -> Option<&StoredCheckpoint> {
        self.state.sources.get(source.as_str())
    }

    pub fn commit<'a>(
        &mut self,
        checkpoints: impl Iterator<Item = (&'a SourceKey, &'a StoredCheckpoint)>,
    ) -> Result<(), String> {
        let mut next = self.state.clone();
        for (key, checkpoint) in checkpoints {
            next.sources
                .insert(key.as_str().to_string(), checkpoint.clone());
        }
        save_state(&self.directory.join(STATE_FILE), &next)?;
        self.state = next;
        Ok(())
    }
}

fn load_or_create_key(path: &Path) -> Result<[u8; KEY_BYTES], String> {
    if let Some(key) = read_key(path)? {
        return Ok(key);
    }
    let mut key = [0_u8; KEY_BYTES];
    getrandom::fill(&mut key).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            sync_parent(path)?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_key(path)?.ok_or_else(|| "accounting source key has an invalid length".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn read_key(path: &Path) -> Result<Option<[u8; KEY_BYTES]>, String> {
    let mut bytes = Vec::new();
    match fs::File::open(path) {
        Ok(mut file) => file
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    bytes
        .try_into()
        .map(Some)
        .map_err(|_| "accounting source key has an invalid length".to_string())
}

fn load_state(path: &Path) -> Result<CheckpointFile, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(CheckpointFile {
            schema_version: schema_version(),
            ..Default::default()
        }),
        Err(error) => Err(error.to_string()),
    }
}

fn save_state(path: &Path, state: &CheckpointFile) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(state).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    crate::fs_atomic::replace_file(&temporary, path).map_err(|error| error.to_string())?;
    sync_parent(path)
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "missing parent".to_string())?;
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())
}
