use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::parse;
use crate::limits::LimitSnapshot;

/// How many bytes of tail to inspect per rollout file.
const TAIL_BYTES: u64 = 512 * 1024;
/// How many recent files to inspect before giving up.
const MAX_FILES: usize = 12;

pub fn read() -> Result<LimitSnapshot> {
    let home = codex_home().context("no home dir")?;
    read_from_home(&home)
}

/// Read the newest locally cached rate-limit event from one explicit profile.
pub(crate) fn read_from_home(home: &Path) -> Result<LimitSnapshot> {
    let sessions = home.join("sessions");
    let files = recent_rollout_files(&sessions, MAX_FILES)?;
    // Files come newest-first; the first file containing a rate_limits event
    // wins (an event's own timestamp is authoritative for staleness display).
    let (ts, payload, path) = files
        .iter()
        .find_map(|f| last_rate_limit_event(f).map(|(ts, p)| (ts, p, f.clone())))
        .context("no token_count events with rate_limits found")?;
    Ok(parse(&payload, Some(ts), path.display().to_string()))
}

pub(crate) fn codex_home() -> Option<PathBuf> {
    std::env::var("CODEX_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))
}

pub(crate) fn read_email_from_home(home: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(home.join("auth.json")).ok()?;
    let auth: Value = serde_json::from_str(&raw).ok()?;
    let id_token = auth.pointer("/tokens/id_token").and_then(Value::as_str)?;
    let payload = id_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("email")
        .and_then(Value::as_str)
        .filter(|email| !email.is_empty())
        .map(str::to_string)
}

/// Newest rollout files without walking the whole 16 GB tree: descend the
/// lexicographically-greatest `YYYY/MM/DD` directories, newest days first.
fn recent_rollout_files(sessions: &Path, max: usize) -> Result<Vec<PathBuf>> {
    fn sorted_dirs_desc(p: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        v.sort();
        v.reverse();
        v
    }
    let mut out = Vec::new();
    'outer: for year in sorted_dirs_desc(sessions) {
        for month in sorted_dirs_desc(&year) {
            for day in sorted_dirs_desc(&month) {
                let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&day)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().map(|e| e == "jsonl").unwrap_or(false)
                            && p.file_name()
                                .and_then(|n| n.to_str())
                                .map(|n| n.starts_with("rollout-"))
                                .unwrap_or(false)
                    })
                    .filter_map(|p| {
                        p.metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(|t| (t, p))
                    })
                    .collect();
                files.sort_by_key(|entry| std::cmp::Reverse(entry.0));
                for (_, p) in files {
                    out.push(p);
                    if out.len() >= max {
                        break 'outer;
                    }
                }
            }
        }
    }
    if out.is_empty() {
        anyhow::bail!("no rollout files under {}", sessions.display());
    }
    Ok(out)
}

/// Scan a file's tail backwards for the last token_count event that carries a
/// non-null `rate_limits`. Returns (event timestamp, rate_limits value).
fn last_rate_limit_event(path: &Path) -> Option<(DateTime<Utc>, Value)> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    let mut lines: Vec<&str> = buf.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // first line is likely partial
    }
    for line in lines.iter().rev() {
        if !line.contains("\"token_count\"") || !line.contains("rate_limits") {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rl = v.pointer("/payload/rate_limits")?;
        if rl.is_null() {
            continue;
        }
        let ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))?;
        return Some((ts, rl.clone()));
    }
    None
}
