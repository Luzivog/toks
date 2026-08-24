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
mod processed_hashes_tests;
