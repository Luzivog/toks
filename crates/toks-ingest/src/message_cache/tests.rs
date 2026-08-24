use super::*;
use crate::clients::ClientId;
use crate::paths::json_path_literal;
use crate::sessions::codex::CodexParseState;
use crate::{TokenBreakdown, UnifiedMessage};
use bincode::Options;
#[cfg(windows)]
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::{NamedTempFile, TempDir};

mod cache_dir_resolution;
#[cfg(unix)]
mod cached_path_unix;
#[cfg(windows)]
mod cached_path_windows;
mod parser_version;
mod shard_io;
mod source_fingerprint_claude;
mod source_fingerprint_core;
mod source_fingerprint_related;
mod source_message_cache_history;
mod source_message_cache_mutation;

/// Pin every env var the cache resolvers consult so the test stays
/// inside `temp_home`, until the returned guard drops. CI runners can leak
/// `XDG_CONFIG_HOME` / `XDG_CACHE_HOME` from the host, which would resolve
/// cache shards outside the sandbox.
///
/// The restore has to be a `Drop` guard rather than a trailing call. A
/// failing assertion panics before any trailing restore runs, and of the
/// four keys here `TOKSCOPE_CONFIG_DIR` is consulted first on every
/// platform — so a leaked one aims every later test in this binary at a
/// `TempDir` that has already been dropped, which is the contamination this
/// sandbox exists to prevent. `serial_test` prevents overlap, not
/// inheritance.
#[must_use = "the sandbox is torn down as soon as the guard drops; bind it to a \
              named variable that outlives the test body"]
fn sandbox_cache_env(temp_home: &std::path::Path) -> crate::paths::test_env::EnvGuard {
    let mut env = crate::paths::test_env::EnvGuard::capture(&[
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "TOKSCOPE_CONFIG_DIR",
    ]);
    env.set("HOME", temp_home);
    env.set("XDG_CONFIG_HOME", temp_home.join(".config"));
    env.set("XDG_CACHE_HOME", temp_home.join(".cache"));
    // The three above isolate the cache on Unix and none of them reach
    // it on Windows: `paths::get_config_dir` resolves the Windows root
    // with `dirs::config_dir()`, a known-folder lookup that reads no
    // environment variable. Without this line every test here shared
    // one real `%APPDATA%\tokscope\cache`, so `SourceMessageCache::load`
    // returned its neighbours' shards along with its own and the entry
    // counts came out too high. `TOKSCOPE_CONFIG_DIR` is the override
    // paths.rs documents for this case and is consulted first
    // everywhere; on Unix it names the directory the redirects above
    // already produced.
    env.set(
        "TOKSCOPE_CONFIG_DIR",
        temp_home.join(".config").join("tokscope"),
    );
    env
}

fn write_temp_file(content: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();
    file
}

fn test_entry(identity: CacheIdentity, path: &Path, session_id: &str) -> CachedSourceEntry {
    CachedSourceEntry::new(
        identity,
        path,
        SourceFingerprint::from_path(path).unwrap(),
        vec![UnifiedMessage::new(
            identity.namespace,
            "gpt-5",
            "provider",
            session_id,
            1,
            TokenBreakdown {
                input: 1,
                output: 2,
                cache_read: 3,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
        )],
        Vec::new(),
        None,
    )
}

fn cache_shard_path(identity: CacheIdentity, path: &Path) -> PathBuf {
    let root = cache_shard_dir().unwrap();
    shard_path(&root, &CacheKey::new(identity, path).shard())
}
