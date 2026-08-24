//! Senpi (OmO Native) session parser
//!
//! Senpi is a pi-mono descendant using the same JSONL record format, so parsing
//! delegates to [`super::pi::parse_pi_format_file`]; only the scan root and
//! client id differ. Two divergences from Pi matter here: `usage.reasoning` is
//! parsed but never summed, because senpi documents it as a subset of `output`
//! while Toks totals reasoning as its own additive bucket; and
//! `session_info.name` carries a human session title rather than Pi's
//! `subagent-<name>-<id>` marker.
//!
//! OmO task children are senpi sessions too, but `SENPI_CODING_AGENT_SESSION_DIR`
//! redirects them to `<project>/.omo/senpi-task/children/<taskId>/sessions/`, so
//! they need an explicit `scanner.extraScanPaths.senpi` entry to be counted.

use super::pi::parse_pi_format_file;
use super::UnifiedMessage;
use std::path::Path;

/// Parse a Senpi JSONL session file.
pub fn parse_senpi_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_pi_format_file(path, "senpi", "senpi")
}

#[cfg(test)]
mod senpi_tests;
