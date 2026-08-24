//! Kimchi Coding session parser.
//!
//! Kimchi stores sessions in the Pi-compatible JSONL format under its own
//! agent directory. Reuse the shared Pi parser while stamping messages with
//! the distinct `kimchi` client id.

use super::pi::parse_pi_format_file_with_dedup;
use super::UnifiedMessage;
use std::path::Path;

pub fn parse_kimchi_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_pi_format_file_with_dedup(path, "kimchi", "kimchi")
}

#[cfg(test)]
mod kimchi_tests;
