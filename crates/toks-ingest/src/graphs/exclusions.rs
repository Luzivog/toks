use crate::{pricing, TokenBreakdown, UnifiedMessage, UnpricedSubmissionExclusion};
use std::collections::BTreeMap;

pub(crate) const ROUTING_LABEL_UNPRICED_REASON: &str =
    "generic routing label has no authoritative model-to-price mapping";
pub(crate) const MISSING_MODEL_PRICING_REASON: &str = "no authoritative model-to-price mapping";
pub(crate) const INCOMPLETE_MODEL_PRICING_REASON: &str =
    "pricing does not cover every populated token bucket";

/// Routing labels name the router that served the request, never the model
/// that answered it, so they have no authoritative model-to-price mapping.
/// This defers to `lookup::is_routing_label` (lookup.rs) rather than restating
/// its `ROUTING_LABELS` list: the reason a row is excluded has to name the same
/// labels the resolver refuses at its top, and a second copy of the list would
/// drift the moment a label is added to one side. Trimming matches for the same
/// reason — the resolver trims, so ` auto ` must not read as a routing label
/// here while being refused there. The historical `gemini-default` pair is
/// provider-scoped and lives only in this reason, not in the resolver gate.
pub(crate) fn is_generic_routing_label(provider_id: &str, model_id: &str) -> bool {
    (provider_id.eq_ignore_ascii_case("google")
        && model_id.trim().eq_ignore_ascii_case("gemini-default"))
        || pricing::lookup::is_routing_label(model_id)
}

pub(super) fn has_positive_token_usage(tokens: &TokenBreakdown) -> bool {
    tokens.input > 0
        || tokens.output > 0
        || tokens.cache_read > 0
        || tokens.cache_write > 0
        || tokens.reasoning > 0
}

pub(super) fn exclude_unpriced_submission_messages(
    messages: Vec<UnifiedMessage>,
    pricing: Option<&pricing::PricingService>,
) -> (Vec<UnifiedMessage>, Vec<UnpricedSubmissionExclusion>) {
    let Some(pricing) = pricing else {
        return (messages, Vec::new());
    };

    let mut submitted = Vec::with_capacity(messages.len());
    let mut exclusions: BTreeMap<(String, String), (usize, i64, &'static str)> = BTreeMap::new();

    for message in messages {
        let is_unpriced = has_positive_token_usage(&message.tokens)
            && !message.has_authoritative_cost()
            && !pricing.covers_usage_with_provider(
                &message.model_id,
                Some(&message.provider_id),
                &message.tokens,
            );

        if is_unpriced {
            // Resolution is consulted before the routing-label reason, not
            // after. `custom-pricing.json` is read first by
            // `lookup_with_source_and_provider`, and stating a rate for `auto`
            // there is the user asserting the label does name something for
            // them — the escape hatch `lookup::ROUTING_LABELS` documents.
            // Checking the label first told that user their label "has no
            // authoritative model-to-price mapping" while their own file held
            // one, hiding the fixable gap (a bucket their entry omits).
            // Nothing regresses for unpriced labels: the resolver refuses
            // routing labels outright, so with no custom entry this returns
            // None and the routing-label reason still applies.
            let reason = if pricing
                .lookup_with_source_and_provider(
                    &message.model_id,
                    None,
                    Some(&message.provider_id),
                )
                .is_some()
            {
                INCOMPLETE_MODEL_PRICING_REASON
            } else if is_generic_routing_label(&message.provider_id, &message.model_id) {
                ROUTING_LABEL_UNPRICED_REASON
            } else {
                MISSING_MODEL_PRICING_REASON
            };
            let entry = exclusions
                .entry((message.provider_id.clone(), message.model_id.clone()))
                .or_insert((0, 0, reason));
            entry.0 = entry
                .0
                .saturating_add(message.message_count.max(0) as usize);
            entry.1 = entry.1.saturating_add(message.tokens.total());
        } else {
            submitted.push(message);
        }
    }

    let exclusions = exclusions
        .into_iter()
        .map(
            |((provider_id, model_id), (message_count, total_tokens, reason))| {
                UnpricedSubmissionExclusion {
                    provider_id,
                    model_id,
                    message_count,
                    total_tokens,
                    reason,
                }
            },
        )
        .collect();
    (submitted, exclusions)
}
