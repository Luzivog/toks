use super::*;

/// The definition the fast derivation implements: re-solve the component
/// with one attribution removed and see whether an equally cheap maximum
/// matching survives without it.
fn indispensable_by_reprobe(component: &MatchingComponent) -> Vec<bool> {
    let (cardinality, cost, assignment) = min_cost_max_matching(component, None);
    let saturated = cardinality == component.edges.len();
    assignment
        .iter()
        .enumerate()
        .map(|(local, matched)| {
            if matched.is_none() {
                return false;
            }
            if saturated {
                return true;
            }
            let (alternate_cardinality, alternate_cost, _) =
                min_cost_max_matching(component, Some(local));
            !(alternate_cardinality == cardinality && alternate_cost == cost)
        })
        .collect()
}

/// Deterministic xorshift, so a failing case is reproducible from its seed
/// without pulling a random-number crate into the dependency tree.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn random_component(rng: &mut Rng, attributions: usize, children: usize) -> MatchingComponent {
    // Three cost shapes, because ties are what make the tie rule bite: all
    // pairings equally close, a mix of exact and merely-in-window hits, and
    // fully spread timestamp distances.
    let shape = rng.below(3);
    let edges = (0..attributions)
        .map(|_| {
            let mut candidates: Vec<(usize, MatchCost)> = Vec::new();
            for child in 0..children {
                // Partially pruned children: not every attribution reaches
                // every child response.
                if rng.below(3) == 0 {
                    continue;
                }
                let cost = match shape {
                    0 => [0, 0, 1001][rng.below(3)],
                    1 => [0, 1, 2, 1001][rng.below(4)],
                    _ => rng.below(1001) as MatchCost,
                };
                candidates.push((child, cost));
            }
            candidates.sort();
            candidates
        })
        .collect();
    MatchingComponent {
        attributions: (0..attributions).collect(),
        edges,
        children,
    }
}

#[test]
fn one_matching_decides_indispensability_the_same_way_as_re_solving() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0usize;
    for _ in 0..4_000 {
        let attributions = 1 + rng.below(8);
        let children = 1 + rng.below(8);
        let component = random_component(&mut rng, attributions, children);
        if component.edges.iter().all(|edges| edges.is_empty()) {
            continue;
        }
        let (_, _, assignment) = min_cost_max_matching(&component, None);
        assert_eq!(
            indispensable_attributions(&component, &assignment),
            indispensable_by_reprobe(&component),
            "component {:?} with {children} children",
            component.edges
        );
        checked += 1;
    }
    assert!(checked > 3_000, "only {checked} components were generated");
}

#[test]
fn a_large_equal_usage_component_with_pruned_children_resolves_quickly() {
    // 60 attributions in one equal-usage bucket, all landing on the same
    // completion timestamp, with only half the child responses still on
    // disk: the legacy shape where the per-attribution re-probe used to run
    // a full matching 30 times over.
    let attributions = 60usize;
    let children = attributions / 2;
    let component = MatchingComponent {
        attributions: (0..attributions).collect(),
        edges: (0..attributions)
            .map(|_| (0..children).map(|child| (child, 0)).collect())
            .collect(),
        children,
    };
    let start = std::time::Instant::now();
    let (cardinality, _, assignment) = min_cost_max_matching(&component, None);
    let indispensable = indispensable_attributions(&component, &assignment);
    let elapsed = start.elapsed();

    assert_eq!(cardinality, children);
    // Every attribution is interchangeable, so no matching is forced and
    // each parent aggregate is kept.
    assert!(indispensable.iter().all(|forced| !forced));
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "matching a 60-attribution component took {elapsed:?}"
    );
}

#[test]
fn rejects_the_rlm_subagent_catalog_as_a_session() {
    let file = session_file(
        r#"{"type":"rlm_subagent","childId":"sub-deadbeef","sessionName":"worker","sessionFile":"/tmp/child.jsonl"}"#,
    );

    assert!(parse_prime_agent_file(file.path()).is_empty());
}
