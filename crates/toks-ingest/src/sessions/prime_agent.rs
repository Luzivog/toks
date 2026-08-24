//! Prime Agent session parser.
//!
//! Prime Agent stores root sessions in `~/.prime/agent/sessions/*.jsonl` and
//! RLM child sessions below the sibling `session-artifacts` tree. Both use the
//! Pi append-only JSONL record format, so token extraction is shared with the
//! Pi parser. `child_usage_attributed` records are never emitted as messages:
//! Toks scans each child's own transcript directly. Their usage metadata is
//! used only to reverse aggregate parent usage that Prime may persist while
//! serializing a fork, before the copied parent is deduplicated across files.

use super::pi::{
    parse_pi_format_rlm_file_with_observer, PiFormatObserver, PiSessionEntry, PiSessionHeader,
    PiUsage,
};
use super::utils::{lossy_lines, parse_timestamp_str};
use super::UnifiedMessage;
use crate::TokenBreakdown;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[derive(Default)]
struct PrimeDecodeCounter {
    root: Option<PathBuf>,
    messages: usize,
    accounting: usize,
}

#[cfg(test)]
static PRIME_DECODE_COUNTER: std::sync::LazyLock<std::sync::Mutex<PrimeDecodeCounter>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(PrimeDecodeCounter::default()));

#[cfg(test)]
static ACCOUNTING_BACKFILL_REWRITE: std::sync::LazyLock<
    std::sync::Mutex<Option<(PathBuf, String)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

#[cfg(test)]
static STABLE_PARSE_REWRITE: std::sync::LazyLock<std::sync::Mutex<Option<(PathBuf, String)>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

#[cfg(test)]
pub(crate) fn schedule_accounting_backfill_test_rewrite(path: &Path, contents: String) {
    *ACCOUNTING_BACKFILL_REWRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((path.to_path_buf(), contents));
}

#[cfg(test)]
pub(crate) fn schedule_stable_parse_test_rewrite(path: &Path, contents: String) {
    *STABLE_PARSE_REWRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((path.to_path_buf(), contents));
}

#[cfg(test)]
pub(crate) fn run_accounting_backfill_test_hook(path: &Path) {
    let mut scheduled = ACCOUNTING_BACKFILL_REWRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if scheduled
        .as_ref()
        .is_some_and(|(scheduled_path, _)| scheduled_path == path)
    {
        let (_, contents) = scheduled.take().unwrap();
        let modified = std::fs::metadata(path).unwrap().modified().unwrap();
        std::fs::write(path, contents).unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
}

#[cfg(test)]
pub(crate) fn run_stable_parse_test_hook(path: &Path) {
    let mut scheduled = STABLE_PARSE_REWRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if scheduled
        .as_ref()
        .is_some_and(|(scheduled_path, _)| scheduled_path == path)
    {
        let (_, contents) = scheduled.take().unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

#[cfg(test)]
fn record_transcript_decode(path: &Path, accounting: bool) {
    let mut counter = PRIME_DECODE_COUNTER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if counter
        .root
        .as_deref()
        .is_some_and(|root| path.starts_with(root))
    {
        if accounting {
            counter.accounting += 1;
        } else {
            counter.messages += 1;
        }
    }
}

pub fn parse_prime_agent_file(path: &Path) -> Vec<UnifiedMessage> {
    parse_prime_agent_file_with_accounting(path).0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrimeAttribution {
    id: String,
    timestamp: Option<i64>,
    child_usage: TokenBreakdown,
    aggregate_usage: TokenBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChildMessageUsage {
    timestamp: Option<i64>,
    usage: TokenBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrimeUsageAdjustment {
    dedup_key: String,
    persisted_usage: TokenBreakdown,
    attributions: Vec<PrimeAttribution>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct PrimeFileAccounting {
    source_path: PathBuf,
    attributions: Vec<PrimeAttribution>,
    adjustments: Vec<PrimeUsageAdjustment>,
    child_message_usages: Vec<ChildMessageUsage>,
    child_parent_path: Option<PathBuf>,
    fork_parent_path: Option<PathBuf>,
}

struct PrimeAccountingBuilder<'a> {
    path: &'a Path,
    found_header: bool,
    is_rlm_child: bool,
    child_parent_path: Option<PathBuf>,
    fork_parent_path: Option<PathBuf>,
    targets: HashMap<String, (String, TokenBreakdown)>,
    attributions: HashMap<String, Vec<PrimeAttribution>>,
    child_message_usages: Vec<ChildMessageUsage>,
}

impl<'a> PrimeAccountingBuilder<'a> {
    fn new(path: &'a Path) -> Self {
        Self {
            path,
            found_header: false,
            is_rlm_child: false,
            child_parent_path: None,
            fork_parent_path: None,
            targets: HashMap::new(),
            attributions: HashMap::new(),
            child_message_usages: Vec::new(),
        }
    }

    fn finish(self) -> PrimeFileAccounting {
        if !self.found_header {
            return PrimeFileAccounting::default();
        }

        let all_attributions = self
            .attributions
            .values()
            .flat_map(|entries| entries.iter().cloned())
            .collect();
        let mut adjustments = Vec::new();
        for (target_id, entries) in self.attributions {
            let Some((dedup_key, persisted_usage)) = self.targets.get(&target_id) else {
                continue;
            };
            let mut matching_prefix = None;
            for (index, entry) in entries.iter().enumerate() {
                if entry.aggregate_usage == *persisted_usage {
                    matching_prefix = Some(entries[..=index].to_vec());
                }
            }
            if let Some(prefix) = matching_prefix {
                adjustments.push(PrimeUsageAdjustment {
                    dedup_key: dedup_key.clone(),
                    persisted_usage: persisted_usage.clone(),
                    attributions: prefix,
                });
            }
        }

        PrimeFileAccounting {
            source_path: lineage_path(self.path),
            attributions: all_attributions,
            adjustments,
            child_message_usages: self.child_message_usages,
            child_parent_path: self.child_parent_path,
            fork_parent_path: self.fork_parent_path,
        }
    }
}

impl PiFormatObserver for PrimeAccountingBuilder<'_> {
    fn observe_header(&mut self, header: &PiSessionHeader) {
        self.found_header = true;
        self.is_rlm_child = header.rlm_depth.unwrap_or(0) > 0;
        let parent_path = header
            .parent_session
            .as_deref()
            .map(Path::new)
            .map(|parent| referenced_lineage_path(self.path, parent));
        if self.is_rlm_child {
            self.child_parent_path = parent_path;
        } else {
            self.fork_parent_path = parent_path;
        }
    }

    fn observe_entry(&mut self, entry: &PiSessionEntry, emitted: Option<&UnifiedMessage>) {
        let entry_timestamp = entry.timestamp.as_deref().and_then(parse_timestamp_str);
        if entry.entry_type == "child_usage_attributed" {
            if let (Some(id), Some(target_id), Some(child_usage), Some(aggregate_usage)) = (
                entry.id.as_ref(),
                entry.target_id.as_ref(),
                entry.child_usage.as_ref(),
                entry.aggregate_usage.as_ref(),
            ) {
                self.attributions
                    .entry(target_id.clone())
                    .or_default()
                    .push(PrimeAttribution {
                        id: id.clone(),
                        timestamp: entry_timestamp,
                        child_usage: usage_breakdown(child_usage),
                        aggregate_usage: usage_breakdown(aggregate_usage),
                    });
            }
            return;
        }

        let Some(parsed) = emitted else {
            return;
        };
        if self.is_rlm_child {
            self.child_message_usages.push(ChildMessageUsage {
                timestamp: entry_timestamp,
                usage: parsed.tokens.clone(),
            });
        }
        if let (Some(id), Some(dedup_key)) = (entry.id.as_ref(), parsed.dedup_key.as_ref()) {
            self.targets
                .insert(id.clone(), (dedup_key.clone(), parsed.tokens.clone()));
        }
    }
}

pub(crate) fn parse_prime_agent_file_with_accounting(
    path: &Path,
) -> (Vec<UnifiedMessage>, PrimeFileAccounting) {
    #[cfg(test)]
    record_transcript_decode(path, false);

    let mut accounting = PrimeAccountingBuilder::new(path);
    let messages =
        parse_pi_format_rlm_file_with_observer(path, "prime-agent", "prime-agent", &mut accounting);
    (messages, accounting.finish())
}

#[cfg(test)]
pub(crate) fn reset_transcript_decode_call_counts(root: &Path) {
    *PRIME_DECODE_COUNTER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = PrimeDecodeCounter {
        root: Some(root.to_path_buf()),
        messages: 0,
        accounting: 0,
    };
}

#[cfg(test)]
pub(crate) fn transcript_decode_call_counts() -> (usize, usize) {
    let counter = PRIME_DECODE_COUNTER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (counter.messages, counter.accounting)
}

fn usage_breakdown(usage: &PiUsage) -> TokenBreakdown {
    TokenBreakdown {
        input: usage.input.unwrap_or(0).max(0),
        output: usage.output.unwrap_or(0).max(0),
        cache_read: usage.cache_read.unwrap_or(0).max(0),
        cache_write: usage.cache_write.unwrap_or(0).max(0),
        reasoning: 0,
    }
}

fn add_usage(total: &mut TokenBreakdown, usage: &TokenBreakdown) {
    *total += usage;
}

// Prime accounting snapshots currently expose only these four cumulative
// fields. This is an intentional field-wise max, not additive aggregation;
// reasoning remains zero until the transcript schema provides it.
fn maximize_usage(total: &mut TokenBreakdown, usage: &TokenBreakdown) {
    total.input = total.input.max(usage.input);
    total.output = total.output.max(usage.output);
    total.cache_read = total.cache_read.max(usage.cache_read);
    total.cache_write = total.cache_write.max(usage.cache_write);
}

// Residual accounting subtracts the same four cumulative snapshot fields.
// This is deliberately not whole-breakdown addition.
fn subtract_usage(total: &mut TokenBreakdown, usage: &TokenBreakdown) {
    total.input = total.input.saturating_sub(usage.input).max(0);
    total.output = total.output.saturating_sub(usage.output).max(0);
    total.cache_read = total.cache_read.saturating_sub(usage.cache_read).max(0);
    total.cache_write = total.cache_write.saturating_sub(usage.cache_write).max(0);
}

type UsageKey = (i64, i64, i64, i64);
type LineageUsageKey = (PathBuf, UsageKey);
/// Attribution ids are only unique within one session: Prime mints them with
/// `randomUUID().slice(0, 8)` and collision-checks against that session's own id
/// map alone. Pairing an id with its resolved lineage root keeps fork copies of
/// one attribution collapsed while keeping a colliding id in an unrelated
/// lineage independent.
type AttributionKey = (PathBuf, String);
/// One parsed child response: the pool bucket it landed in plus its position
/// inside that bucket. Buckets are keyed by parent lineage and usage, so this
/// identifies a single transcript entry without depending on scan order.
type ChildResponseRef = (LineageUsageKey, usize);

fn usage_key(usage: &TokenBreakdown) -> UsageKey {
    (
        usage.input,
        usage.output,
        usage.cache_read,
        usage.cache_write,
    )
}

/// Resolve every file to the head of its fork chain. Serializing a fork copies
/// the parent's `child_usage_attributed` records verbatim, so all copies within
/// one chain describe the same invocation and must share an attribution
/// identity. Files in different chains never do.
///
/// A chain can loop: two files can name each other as fork parent, and a rewritten
/// or relocated session can close a longer loop. Stopping the walk on a repeat is
/// not enough, because each member would then stop at itself and the copies would
/// be accounted for as unrelated attributions, restoring the same child delta once
/// per member. Every member of a loop therefore resolves to a single deterministic
/// representative instead.
fn lineage_roots(accounting: &[PrimeFileAccounting]) -> HashMap<PathBuf, PathBuf> {
    let forked_from: HashMap<&PathBuf, &PathBuf> = accounting
        .iter()
        .filter_map(|file| Some((&file.source_path, file.fork_parent_path.as_ref()?)))
        .collect();
    let mut roots: HashMap<PathBuf, PathBuf> = HashMap::new();
    for file in accounting {
        // Walk the fork chain, remembering the order the files were seen so a
        // cycle can be recognized by where it closes rather than merely stopped.
        let mut chain: Vec<PathBuf> = Vec::new();
        let mut position: HashMap<PathBuf, usize> = HashMap::new();
        let mut node = file.source_path.clone();
        let root = loop {
            if let Some(resolved) = roots.get(&node) {
                break resolved.clone();
            }
            if let Some(entered) = position.get(&node).copied() {
                // A fork chain that loops back on itself: every file in the loop
                // is a copy of the same fork history, so they must collapse onto
                // one representative instead of each becoming its own root. Take
                // the smallest path in the loop, which no scan order can change.
                break chain[entered..].iter().min().cloned().unwrap_or(node);
            }
            position.insert(node.clone(), chain.len());
            chain.push(node.clone());
            match forked_from.get(&node) {
                Some(parent) => node = (*parent).clone(),
                // The head of an acyclic chain is its own root.
                None => break node,
            }
        };
        // Memoized for the whole walk: every file on a chain shares its root, and
        // a chain that runs into a loop adopts the loop's representative.
        for member in chain {
            roots.insert(member, root.clone());
        }
        roots.entry(file.source_path.clone()).or_insert(root);
    }
    roots
}

fn lineage_root(roots: &HashMap<PathBuf, PathBuf>, file: &PrimeFileAccounting) -> PathBuf {
    roots
        .get(&file.source_path)
        .cloned()
        .unwrap_or_else(|| file.source_path.clone())
}

fn lineage_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn referenced_lineage_path(source_file: &Path, referenced: &Path) -> PathBuf {
    if referenced.is_absolute() {
        lineage_path(referenced)
    } else {
        lineage_path(
            &source_file
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(referenced),
        )
    }
}

/// Read Prime-only accounting records that are intentionally absent from the
/// shared Pi message representation. `messages` may come from the source cache;
/// their stable order is used to associate target entry ids with emitted rows.
pub(crate) fn analyze_prime_agent_accounting(
    path: &Path,
    messages: &[UnifiedMessage],
) -> PrimeFileAccounting {
    #[cfg(test)]
    record_transcript_decode(path, true);

    let Ok(file) = std::fs::File::open(path) else {
        return PrimeFileAccounting::default();
    };

    let mut accounting = PrimeAccountingBuilder::new(path);
    let mut found_header = false;
    let mut message_index = 0usize;
    for line in lossy_lines(BufReader::new(file)) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !found_header {
            if let Ok(header) = serde_json::from_str::<PiSessionHeader>(trimmed) {
                if header.entry_type == "session" {
                    found_header = true;
                    accounting.observe_header(&header);
                    continue;
                }
            }
            let is_pre_session_title = serde_json::from_str::<serde_json::Value>(trimmed)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(|kind| kind.as_str())
                        .map(str::to_owned)
                })
                .is_some_and(|kind| kind == "title");
            if is_pre_session_title {
                continue;
            }
            return PrimeFileAccounting::default();
        }

        let Ok(entry) = serde_json::from_str::<PiSessionEntry>(trimmed) else {
            continue;
        };
        let emitted = entry
            .message
            .as_ref()
            .filter(|message| {
                entry.entry_type == "message"
                    && message.role.as_deref() == Some("assistant")
                    && message.usage.is_some()
                    && message.model.is_some()
            })
            .and_then(|_| {
                let parsed = messages.get(message_index);
                message_index += 1;
                parsed
            });
        accounting.observe_entry(&entry, emitted);
    }

    accounting.finish()
}

fn fallback_key_base(key: &str) -> Option<&str> {
    if !key.starts_with("prime-agent:message:") {
        return None;
    }
    let mut parts = key.rsplitn(5, ':');
    parts.next()?;
    parts.next()?;
    parts.next()?;
    parts.next()?;
    parts.next()
}

fn rewrite_fallback_usage(key: &str, usage: &TokenBreakdown) -> String {
    fallback_key_base(key).map_or_else(
        || key.to_string(),
        |base| {
            format!(
                "{base}:{}:{}:{}:{}",
                usage.input, usage.output, usage.cache_read, usage.cache_write
            )
        },
    )
}

/// Timestamp distance in milliseconds between an attribution and a parsed child
/// response. A lower cost is a better explanation of one completion event.
type MatchCost = i64;

/// One independent contention group, in dense local indices: the attributions
/// that reach a shared set of child responses, directly or transitively.
/// Separate components never influence each other, so each is matched alone.
struct MatchingComponent {
    /// Local attribution index -> index into the global attribution key list.
    attributions: Vec<usize>,
    /// Local attribution index -> its eligible (local child index, cost) pairs.
    edges: Vec<Vec<(usize, MatchCost)>>,
    children: usize,
}

fn disjoint_set_root(parents: &mut [usize], node: usize) -> usize {
    let mut root = node;
    while parents[root] != root {
        root = parents[root];
    }
    let mut walk = node;
    while parents[walk] != root {
        let next = parents[walk];
        parents[walk] = root;
        walk = next;
    }
    root
}

/// Split the attribution/child graph into connected components, so the
/// attributions that genuinely contend for the same child responses are matched
/// together while unrelated sessions stay out of each other's cost accounting.
fn matching_components(eligible: &[Vec<(MatchCost, ChildResponseRef)>]) -> Vec<MatchingComponent> {
    let mut child_indices: BTreeMap<ChildResponseRef, usize> = BTreeMap::new();
    for candidates in eligible {
        for (_, candidate) in candidates {
            let next = child_indices.len();
            child_indices.entry(candidate.clone()).or_insert(next);
        }
    }
    let mut parents: Vec<usize> = (0..eligible.len() + child_indices.len()).collect();
    for (attribution, candidates) in eligible.iter().enumerate() {
        for (_, candidate) in candidates {
            let child = eligible.len() + child_indices[candidate];
            let left = disjoint_set_root(&mut parents, attribution);
            let right = disjoint_set_root(&mut parents, child);
            if left != right {
                parents[left] = right;
            }
        }
    }
    let mut grouped: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (attribution, candidates) in eligible.iter().enumerate() {
        if candidates.is_empty() {
            continue;
        }
        let root = disjoint_set_root(&mut parents, attribution);
        grouped.entry(root).or_default().push(attribution);
    }
    grouped
        .into_values()
        .map(|attributions| {
            let mut local_children: BTreeMap<ChildResponseRef, usize> = BTreeMap::new();
            let mut edges = Vec::with_capacity(attributions.len());
            for attribution in &attributions {
                let mut candidates = Vec::new();
                for (cost, candidate) in &eligible[*attribution] {
                    let next = local_children.len();
                    let child = *local_children.entry(candidate.clone()).or_insert(next);
                    candidates.push((child, *cost));
                }
                edges.push(candidates);
            }
            MatchingComponent {
                attributions,
                edges,
                children: local_children.len(),
            }
        })
        .collect()
}

/// Minimum-cost maximum matching over one component, by successive shortest
/// augmenting paths: every augmentation takes the cheapest path that adds one
/// pair, so the result is a maximum matching whose total timestamp distance is
/// the smallest of any maximum matching. Plain maximum-cardinality matching is
/// not enough here -- it fixes how many attributions are matched but not which
/// ones -- so this is what stops an attribution that merely lands inside the
/// tolerance window from consuming a child response another attribution explains
/// exactly.
///
/// `blocked` removes one attribution from the component, which answers whether
/// that attribution is dispensable at no extra cost by brute force. Production
/// code derives that from a single matching via `indispensable_attributions`;
/// the parameter is kept so the tests can check the fast derivation against the
/// definition it implements.
///
/// Returns the cardinality, the total cost, and each local attribution's child.
fn min_cost_max_matching(
    component: &MatchingComponent,
    blocked: Option<usize>,
) -> (usize, MatchCost, Vec<Option<usize>>) {
    let attributions = component.edges.len();
    let children = component.children;
    let source = attributions + children;
    let sink = source + 1;
    let mut matched_attribution: Vec<Option<usize>> = vec![None; attributions];
    let mut matched_child: Vec<Option<usize>> = vec![None; children];
    let mut cardinality = 0usize;

    loop {
        // Residual arcs: an unused pairing costs its distance and a used one
        // refunds it, so the cheapest source-to-sink walk is the cheapest way
        // to gain one pair. Refunds are negative, hence Bellman-Ford rather
        // than Dijkstra; a component is the handful of attributions sharing one
        // equal-usage bucket inside one lineage.
        let mut residual: Vec<(usize, usize, MatchCost)> = Vec::new();
        for (attribution, matched) in matched_attribution.iter().enumerate() {
            if blocked == Some(attribution) {
                continue;
            }
            if matched.is_none() {
                residual.push((source, attribution, 0));
            }
            for &(child, cost) in &component.edges[attribution] {
                if *matched == Some(child) {
                    residual.push((attributions + child, attribution, -cost));
                } else {
                    residual.push((attribution, attributions + child, cost));
                }
            }
        }
        for (child, matched) in matched_child.iter().enumerate() {
            if matched.is_none() {
                residual.push((attributions + child, sink, 0));
            }
        }

        let mut distance: Vec<Option<MatchCost>> = vec![None; sink + 1];
        let mut previous: Vec<Option<usize>> = vec![None; sink + 1];
        distance[source] = Some(0);
        for _ in 0..=sink {
            let mut improved = false;
            for &(from, to, cost) in &residual {
                let Some(reached) = distance[from] else {
                    continue;
                };
                let candidate = reached + cost;
                if distance[to].is_none_or(|current| candidate < current) {
                    distance[to] = Some(candidate);
                    previous[to] = Some(from);
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }
        if distance[sink].is_none() {
            break;
        }

        // Re-seat every pairing the augmenting path crosses. A shortest path is
        // simple, so each attribution and each child response appears at most
        // once and the rewrites are independent of the order applied.
        let mut node = sink;
        let mut steps = 0;
        while let Some(from) = previous[node] {
            if from < attributions && (attributions..source).contains(&node) {
                let child = node - attributions;
                matched_attribution[from] = Some(child);
                matched_child[child] = Some(from);
            }
            node = from;
            steps += 1;
            if steps > sink {
                break;
            }
        }
        cardinality += 1;
    }

    let mut cost = 0;
    for (attribution, matched) in matched_attribution.iter().enumerate() {
        if let Some(child) = matched {
            cost += component.edges[attribution]
                .iter()
                .find(|(candidate, _)| candidate == child)
                .map_or(0, |(_, cost)| *cost);
        }
    }
    (cardinality, cost, matched_attribution)
}

/// Which attributions appear in EVERY minimum-cost maximum matching of the
/// component, derived from a single matching instead of re-solving the whole
/// component once per matched attribution.
///
/// Read the matching as a unit-capacity min-cost flow of value `cardinality`:
/// `source -> attribution -> child -> sink`. Any other matching of the same
/// cardinality and the same cost is that flow plus a zero-cost circulation in
/// the residual graph, and every such circulation splits into simple residual
/// cycles that individually cost zero. Node potentials that make all residual
/// arcs non-negative exist because the matching is already cost-optimal, and
/// potentials cancel around a cycle, so a residual cycle costs zero exactly
/// when every arc on it has zero reduced cost.
///
/// A matched attribution drops out of the matching precisely when such a cycle
/// cancels its `source -> attribution` arc, i.e. when the residual arc
/// `attribution -> source` has zero reduced cost and lies on a zero-reduced-cost
/// cycle. Because that arc leads to `source`, the cycle exists exactly when
/// `source` reaches the attribution over zero-reduced-cost residual arcs. So one
/// Bellman-Ford pass for the potentials plus one traversal answers the question
/// for every attribution at once.
fn indispensable_attributions(
    component: &MatchingComponent,
    matched_attribution: &[Option<usize>],
) -> Vec<bool> {
    let attributions = component.edges.len();
    let children = component.children;
    let source = attributions + children;
    let sink = source + 1;
    let nodes = sink + 1;

    let mut matched_child: Vec<Option<usize>> = vec![None; children];
    for (attribution, matched) in matched_attribution.iter().enumerate() {
        if let Some(child) = matched {
            matched_child[*child] = Some(attribution);
        }
    }

    // The complete residual graph, unlike the augmenting-path search, which can
    // skip the arcs back to the source and out of the sink because a shortest
    // path never takes them. A cycle that re-seats which child feeds the sink
    // does take them, and that cycle can free an attribution, so they matter
    // here.
    let mut residual: Vec<(usize, usize, MatchCost)> = Vec::new();
    for (attribution, matched) in matched_attribution.iter().enumerate() {
        match matched {
            None => residual.push((source, attribution, 0)),
            Some(_) => residual.push((attribution, source, 0)),
        }
        for &(child, cost) in &component.edges[attribution] {
            if *matched == Some(child) {
                residual.push((attributions + child, attribution, -cost));
            } else {
                residual.push((attribution, attributions + child, cost));
            }
        }
    }
    for (child, matched) in matched_child.iter().enumerate() {
        match matched {
            None => residual.push((attributions + child, sink, 0)),
            Some(_) => residual.push((sink, attributions + child, 0)),
        }
    }

    // Potentials, as Bellman-Ford from a virtual node joined to every node at
    // zero cost: `potential[v] <= potential[u] + cost(u, v)` on every residual
    // arc is exactly the non-negative reduced cost the argument above needs.
    // The relaxation converges because a cost-optimal flow leaves no negative
    // residual cycle.
    let mut potential: Vec<MatchCost> = vec![0; nodes];
    for _ in 0..nodes {
        let mut improved = false;
        for &(from, to, cost) in &residual {
            let candidate = potential[from] + cost;
            if candidate < potential[to] {
                potential[to] = candidate;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
    // `<= 0` rather than `== 0`: with converged potentials the two agree, and if
    // they ever disagreed the extra arcs would only widen the set of
    // attributions treated as dispensable, which is the conservative direction
    // -- an unmatched attribution keeps its parent aggregate rather than
    // authorizing a subtraction.
    let reduced_cost =
        |from: usize, to: usize, cost: MatchCost| cost + potential[from] - potential[to];

    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes];
    for &(from, to, cost) in &residual {
        if reduced_cost(from, to, cost) <= 0 {
            adjacency[from].push(to);
        }
    }
    let mut reached = vec![false; nodes];
    reached[source] = true;
    let mut stack = vec![source];
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !reached[next] {
                reached[next] = true;
                stack.push(next);
            }
        }
    }

    matched_attribution
        .iter()
        .enumerate()
        .map(|(attribution, matched)| {
            matched.is_some()
                && !(reached[attribution] && reduced_cost(attribution, source, 0) <= 0)
        })
        .collect()
}

/// Subtract child usage only when a matching RLM transcript was actually
/// parsed, then collapse fork copies. Missing/pruned children remain represented
/// by Prime's aggregate parent usage instead of disappearing from the total.
pub(crate) fn reconcile_prime_agent_messages(
    messages: Vec<UnifiedMessage>,
    accounting: &[PrimeFileAccounting],
) -> Vec<UnifiedMessage> {
    const ATTRIBUTION_TIMESTAMP_TOLERANCE_MS: i64 = 1_000;

    let mut available_children: HashMap<LineageUsageKey, Vec<Option<i64>>> = HashMap::new();
    for file in accounting {
        if let Some(parent_path) = &file.child_parent_path {
            for child in &file.child_message_usages {
                available_children
                    .entry((parent_path.clone(), usage_key(&child.usage)))
                    .or_default()
                    .push(child.timestamp);
            }
        }
    }

    // Attribution ids survive fork serialization. Record every file that owns
    // a copy, but match only a child response whose header points back to that
    // parent session and whose completion timestamp is the same event (Prime
    // writes the two records within milliseconds). This disambiguates equal
    // token buckets produced by separate children in one parent.
    // Attribution ids are unique only inside one session, so they are keyed by
    // their lineage root as well: fork copies of one attribution still collapse,
    // while a colliding id minted in an unrelated lineage stays separate.
    let roots = lineage_roots(accounting);
    let mut unique_attributions: BTreeMap<
        AttributionKey,
        (TokenBreakdown, Option<i64>, BTreeSet<PathBuf>),
    > = BTreeMap::new();
    for file in accounting {
        let lineage = lineage_root(&roots, file);
        for attribution in &file.attributions {
            let (_, _, owners) = unique_attributions
                .entry((lineage.clone(), attribution.id.clone()))
                .or_insert_with(|| {
                    (
                        attribution.child_usage.clone(),
                        attribution.timestamp,
                        BTreeSet::new(),
                    )
                });
            owners.insert(file.source_path.clone());
            owners.insert(lineage.clone());
            if let Some(parent) = &file.fork_parent_path {
                owners.insert(parent.clone());
            }
        }
    }

    // The matching rule, in full. An attribution authorizes subtracting its
    // `childUsage` from the parent aggregate only when a parsed child response
    // is matched to it, and a child response is eligible only when all three
    // hold:
    //
    // 1. Lineage and size. The child's `parentSession` header must resolve to a
    //    file that owns the attribution, and the child's usage must equal the
    //    recorded `childUsage` bucket exactly.
    // 2. Provable completion identity. Either both records carry a timestamp
    //    within ATTRIBUTION_TIMESTAMP_TOLERANCE_MS -- Prime appends the
    //    attribution milliseconds after the child response it describes -- or
    //    neither record carries one, which only happens in transcripts written
    //    before Prime timestamped its entries and where lineage plus size is
    //    the only identity that exists. A half-timed pairing proves nothing, so
    //    an unrelated same-sized sibling can never stand in for a pruned child
    //    and shrink the parent.
    // 3. Exclusivity. Matching is one-to-one: every child response authorizes
    //    at most one attribution and every attribution consumes at most one
    //    child response. N children of equal size completing in the same
    //    millisecond pair off with their N attributions rather than being
    //    discarded as ambiguous, which would count both the children and the
    //    parent aggregate that already contains them.
    //
    // Rule 3 is settled by a minimum-cost maximum matching, the cost of a
    // pairing being its timestamp distance. Maximum cardinality alone fixes how
    // MANY attributions are matched but not WHICH ones, and that choice decides
    // which parent response gets its aggregate reduced. Every attribution
    // contending for one child response carries the same `childUsage`, so the
    // global token total is the same for every maximum matching -- but the
    // per-model rows are not, and pricing is applied per model after
    // reconciliation, so an arbitrary choice silently moves cost between models.
    // Minimum cost keeps an attribution that merely lands inside the tolerance
    // window from consuming a child response another attribution explains
    // exactly.
    //
    // Remaining ties are resolved conservatively rather than arbitrarily. An
    // attribution is represented only when EVERY minimum-cost maximum matching
    // contains it; if an equally cheap matching exists that leaves it out, the
    // transcripts do not say which aggregate spent that child, so the aggregate
    // is retained -- the same fallback used for a child that was never parsed.
    // That rule is deterministic and independent of attribution id ordering. It
    // cannot decide the residual case where two attributions belonging to
    // different parent responses describe equally sized children that completed
    // in the very same millisecond: nothing in the records distinguishes them,
    // and proving identity there would need an upstream child or response id on
    // the attribution record.
    let attribution_keys: Vec<AttributionKey> = unique_attributions.keys().cloned().collect();
    let eligible: Vec<Vec<(MatchCost, ChildResponseRef)>> = unique_attributions
        .values()
        .map(|(usage, attribution_timestamp, owners)| {
            let mut candidates: Vec<(i64, ChildResponseRef)> = Vec::new();
            for owner in owners {
                let key = (owner.clone(), usage_key(usage));
                let Some(children) = available_children.get(&key) else {
                    continue;
                };
                for (index, child_timestamp) in children.iter().enumerate() {
                    match (attribution_timestamp, *child_timestamp) {
                        (Some(attribution), Some(child)) => {
                            let distance = attribution.abs_diff(child) as i64;
                            if distance <= ATTRIBUTION_TIMESTAMP_TOLERANCE_MS {
                                candidates.push((distance, (key.clone(), index)));
                            }
                        }
                        // Untimed on both sides: legacy transcripts, matched on
                        // lineage and size alone and ranked after every timed
                        // pairing.
                        (None, None) => candidates
                            .push((ATTRIBUTION_TIMESTAMP_TOLERANCE_MS + 1, (key.clone(), index))),
                        _ => {}
                    }
                }
            }
            candidates.sort();
            candidates
        })
        .collect();

    let mut represented_attributions: HashSet<AttributionKey> = HashSet::new();
    for component in matching_components(&eligible) {
        let (_, _, assignment) = min_cost_max_matching(&component, None);
        for (local, indispensable) in indispensable_attributions(&component, &assignment)
            .into_iter()
            .enumerate()
        {
            if indispensable {
                represented_attributions
                    .insert(attribution_keys[component.attributions[local]].clone());
            }
        }
    }

    let mut adjustment_groups: HashMap<String, Vec<(PathBuf, &PrimeUsageAdjustment)>> =
        HashMap::new();
    let mut attribution_fallback_bases = HashSet::new();
    for file in accounting {
        let lineage = lineage_root(&roots, file);
        for adjustment in &file.adjustments {
            let identity = fallback_key_base(&adjustment.dedup_key)
                .inspect(|base| {
                    attribution_fallback_bases.insert((*base).to_string());
                })
                .unwrap_or(&adjustment.dedup_key)
                .to_string();
            adjustment_groups
                .entry(identity)
                .or_default()
                .push((lineage.clone(), adjustment));
        }
    }

    let mut grouped: HashMap<String, Vec<UnifiedMessage>> = HashMap::new();
    let mut group_order = Vec::new();
    for (ordinal, message) in messages.into_iter().enumerate() {
        let identity = message.dedup_key.as_deref().map_or_else(
            || format!("prime-agent:unkeyed:{ordinal}"),
            |key| {
                fallback_key_base(key)
                    .filter(|base| attribution_fallback_bases.contains(*base))
                    .unwrap_or(key)
                    .to_string()
            },
        );
        if !grouped.contains_key(&identity) {
            group_order.push(identity.clone());
        }
        grouped.entry(identity).or_default().push(message);
    }

    let mut deduped = Vec::with_capacity(group_order.len());
    for identity in group_order {
        let mut group = grouped.remove(&identity).unwrap_or_default();
        let Some(mut representative) = group.first().cloned() else {
            continue;
        };
        let Some(adjustments) = adjustment_groups.get(&identity) else {
            for duplicate in group.iter().skip(1) {
                maximize_usage(&mut representative.tokens, &duplicate.tokens);
            }
            deduped.push(representative);
            continue;
        };

        let mut base_usage = TokenBreakdown::default();
        let mut found_base = false;
        let mut all_attributions: BTreeMap<AttributionKey, TokenBreakdown> = BTreeMap::new();
        for (lineage, adjustment) in adjustments {
            let mut own_usage = adjustment.persisted_usage.clone();
            for attribution in &adjustment.attributions {
                subtract_usage(&mut own_usage, &attribution.child_usage);
                all_attributions
                    .entry((lineage.clone(), attribution.id.clone()))
                    .or_insert_with(|| attribution.child_usage.clone());
            }
            maximize_usage(&mut base_usage, &own_usage);
            found_base = true;
        }
        for message in &group {
            let is_aggregate_copy = adjustments.iter().any(|(_, adjustment)| {
                message.dedup_key.as_deref() == Some(&adjustment.dedup_key)
                    && message.tokens == adjustment.persisted_usage
            });
            if !is_aggregate_copy {
                maximize_usage(&mut base_usage, &message.tokens);
                found_base = true;
            }
        }
        if !found_base {
            for message in &group {
                maximize_usage(&mut base_usage, &message.tokens);
            }
        }
        for (attribution_key, usage) in all_attributions {
            if !represented_attributions.contains(&attribution_key) {
                add_usage(&mut base_usage, &usage);
            }
        }

        representative.tokens = base_usage;
        if let Some(key) = representative.dedup_key.as_deref() {
            representative.dedup_key = Some(rewrite_fallback_usage(key, &representative.tokens));
        }
        group.clear();
        deduped.push(representative);
    }
    deduped
}

#[cfg(test)]
mod tests;
