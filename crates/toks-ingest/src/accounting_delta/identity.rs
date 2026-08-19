use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::path::{Path, PathBuf};

use super::types::{SourceKey, SourceKind};

pub(crate) fn source_key(key: &[u8], kind: SourceKind, path: &Path) -> SourceKey {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("32-byte HMAC key");
    mac.update(b"tokscope.accounting-source.v1\0");
    mac.update(match kind {
        SourceKind::Codex => b"codex",
        SourceKind::Claude => b"claude",
    });
    mac.update(&[0]);
    mac.update(source_domain(kind, path).to_string_lossy().as_bytes());
    mac.update(&[0]);
    mac.update(logical_source_name(kind, path).as_bytes());
    SourceKey::new(hex(&mac.finalize().into_bytes()))
}

fn source_domain(kind: SourceKind, path: &Path) -> PathBuf {
    let markers: &[&str] = match kind {
        SourceKind::Codex => &["sessions", "archived_sessions"],
        SourceKind::Claude => &["projects", "transcripts"],
    };
    path.ancestors()
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| markers.contains(&name))
        })
        .and_then(Path::parent)
        .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")))
        .to_path_buf()
}

fn logical_source_name(kind: SourceKind, path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown");
    if kind == SourceKind::Claude {
        if let Some(subagents) = path.ancestors().find(|ancestor| {
            ancestor.file_name().and_then(|name| name.to_str()) == Some("subagents")
        }) {
            if let Some(session) = subagents.parent().and_then(Path::file_name) {
                return format!("{}/{}", session.to_string_lossy(), stem);
            }
        }
    }
    stem.to_string()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").expect("writing to String");
    }
    result
}
