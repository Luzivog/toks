use super::{CodexIdentityTracker, TimestampOccurrence, TimestampOccurrences};
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct CompactTrackerWire {
    #[serde(default)]
    token_count_sequence: u64,
    #[serde(default)]
    timestamp_occurrences: TimestampOccurrences,
}

#[derive(serde::Deserialize)]
struct HumanReadableTrackerWire {
    #[serde(default)]
    token_count_sequence: u64,
    #[serde(default)]
    timestamp_occurrences: TimestampOccurrencesWire,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TimestampOccurrencesWire {
    Legacy(BTreeMap<String, u64>),
    Compact(TimestampOccurrences),
}

impl Default for TimestampOccurrencesWire {
    fn default() -> Self {
        Self::Compact(BTreeMap::new())
    }
}

impl<'de> serde::Deserialize<'de> for CodexIdentityTracker {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let (token_count_sequence, mut occurrences) = if deserializer.is_human_readable() {
            let wire = <HumanReadableTrackerWire as serde::Deserialize>::deserialize(deserializer)?;
            let occurrences = match wire.timestamp_occurrences {
                TimestampOccurrencesWire::Compact(compact) => compact,
                TimestampOccurrencesWire::Legacy(legacy) => compact_legacy(legacy)
                    .ok_or_else(|| serde::de::Error::custom("invalid legacy Codex identity key"))?,
            };
            (wire.token_count_sequence, occurrences)
        } else {
            let wire = <CompactTrackerWire as serde::Deserialize>::deserialize(deserializer)?;
            (wire.token_count_sequence, wire.timestamp_occurrences)
        };
        for entries in occurrences.values_mut() {
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            entries.dedup_by(|next, previous| {
                if next.0 != previous.0 {
                    return false;
                }
                previous.1 = previous.1.saturating_add(next.1);
                true
            });
        }
        Ok(Self {
            token_count_sequence,
            timestamp_occurrences: occurrences,
        })
    }
}

fn compact_legacy(legacy: BTreeMap<String, u64>) -> Option<TimestampOccurrences> {
    let mut compact = TimestampOccurrences::new();
    for (key, count) in legacy {
        let (lineage, timestamp) = decode_two_parts(&key)?;
        compact
            .entry(lineage.into())
            .or_default()
            .push(TimestampOccurrence(timestamp.into(), count));
    }
    Some(compact)
}

fn decode_two_parts(value: &str) -> Option<(&str, &str)> {
    let (first, remainder) = decode_part(value)?;
    let (second, trailing) = decode_part(remainder.strip_prefix('|')?)?;
    trailing.is_empty().then_some((first, second))
}

fn decode_part(value: &str) -> Option<(&str, &str)> {
    let (length, rest) = value.split_once(':')?;
    let length = length.parse::<usize>().ok()?;
    Some((rest.get(..length)?, rest.get(length..)?))
}
