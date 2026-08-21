use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::UnifiedMessage;

#[derive(Debug)]
enum ProcessedSlots {
    One(usize),
    Many(Vec<usize>),
}

impl ProcessedSlots {
    fn find(&self, mut predicate: impl FnMut(usize) -> bool) -> Option<usize> {
        match self {
            Self::One(index) => predicate(*index).then_some(*index),
            Self::Many(indices) => indices.iter().copied().find(|index| predicate(*index)),
        }
    }

    fn push(&mut self, index: usize) {
        match self {
            Self::One(first) => {
                let first = *first;
                *self = Self::Many(vec![first, index]);
            }
            Self::Many(indices) => indices.push(index),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ClaudeProcessedHashes {
    indices: HashMap<u64, ProcessedSlots>,
}

impl ClaudeProcessedHashes {
    pub(super) fn get(&self, key: &str, messages: &[UnifiedMessage]) -> Option<usize> {
        self.indices
            .get(&dedup_hash(key))
            .and_then(|slots| slots.find(|index| messages[index].dedup_key.as_deref() == Some(key)))
    }

    pub(super) fn insert(&mut self, key: &str, index: usize) {
        self.indices
            .entry(dedup_hash(key))
            .and_modify(|slots| slots.push(index))
            .or_insert(ProcessedSlots::One(index));
    }
}

fn dedup_hash(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::parse_claude_file;

    #[test]
    fn parser_keeps_valid_rows_after_invalid_utf8() {
        let first = br#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#;
        let second = br#"{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":100}}}
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(first).unwrap();
        file.write_all(b"{\"invalid\":\"").unwrap();
        file.write_all(&[0xff, b'\n']).unwrap();
        file.write_all(second).unwrap();
        file.flush().unwrap();

        let messages = parse_claude_file(file.path());

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].dedup_key.as_deref(), Some("msg_001:req_001"));
        assert_eq!(messages[1].dedup_key.as_deref(), Some("msg_002:req_002"));
    }
}
