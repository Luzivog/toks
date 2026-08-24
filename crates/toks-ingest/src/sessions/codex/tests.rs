use super::*;
use crate::{aggregate_model_usage_entries, GroupBy};
use std::io::{BufRead, Cursor, Error, ErrorKind, Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

mod durations;
mod fork_boundaries;
mod fork_replay;
mod io_errors;
mod metadata;
mod parsing;
mod turn_state;
mod usage_deltas;

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}
