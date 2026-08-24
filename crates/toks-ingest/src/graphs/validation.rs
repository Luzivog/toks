use super::exclusions::has_positive_token_usage;
use crate::{pricing, UnifiedMessage, UnpricedSubmissionExclusion};
use std::collections::HashMap;

const UNAVAILABLE_SUBMISSION_PRICING: &str = "pricing data is unavailable for submission";

// @keep: the two conditions are load-bearing together; either alone is wrong.
/// Refuse to act on exclusions that no pricing dataset backs.
///
/// `exclude_unpriced_submission_messages` drops what the pricing service cannot
/// cover, but a service with no dataset covers *nothing*, so "unpriced" and "we
/// have no prices" produce identical exclusions. Left alone, a cold cache with
/// no network excludes the entire batch, leaves `total_tokens == 0`, and lets
/// the CLI print "No usage data found to submit" and exit 0 — indistinguishable
/// from genuinely having no usage, and reported as success to autosubmit.
///
/// Both conditions matter:
///
/// - Only when something was excluded. A batch whose messages all carry
///   provider-reported costs never consults pricing, so a missing dataset is
///   irrelevant and must not block it.
/// - Only when no dataset loaded. A populated dataset that simply lacks a price
///   for some model is the case #1053 exists to handle; failing there would
///   break autosubmit for anyone whose usage is legitimately unpriceable, which
///   is the trap #1044 documents.
///
/// This runs after exclusion because the exclusion list is the signal. It
/// cannot move into `validate_priced_messages`, which sees only the survivors —
/// and when everything is excluded that slice is empty and validates trivially.
pub(super) fn require_trustworthy_exclusions(
    pricing: Option<&pricing::PricingService>,
    exclusions: &[UnpricedSubmissionExclusion],
) -> Result<(), String> {
    if exclusions.is_empty() {
        return Ok(());
    }

    match pricing {
        Some(pricing) if pricing.has_pricing_data() => Ok(()),
        _ => Err(UNAVAILABLE_SUBMISSION_PRICING.to_string()),
    }
}

pub(crate) fn validate_priced_messages(
    messages: &[UnifiedMessage],
    pricing: Option<&pricing::PricingService>,
) -> Result<(), String> {
    let Some(pricing) = pricing else {
        return Err(UNAVAILABLE_SUBMISSION_PRICING.to_string());
    };

    // Counted rather than listed per message: a real submission repeats the
    // same handful of ids thousands of times, and the raw list buried the
    // actionable model names under hundreds of kilobytes of output (#1013).
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for message in messages {
        let unpriced = has_positive_token_usage(&message.tokens)
            && !message.has_authoritative_cost()
            && !pricing.covers_usage_with_provider(
                &message.model_id,
                Some(&message.provider_id),
                &message.tokens,
            );
        if !unpriced {
            continue;
        }

        let id = if message.provider_id.is_empty() {
            message.model_id.clone()
        } else {
            format!("{}/{}", message.provider_id, message.model_id)
        };
        match counts.get_mut(&id) {
            Some(count) => *count += 1,
            None => {
                counts.insert(id.clone(), 1);
                order.push(id);
            }
        }
    }

    if order.is_empty() {
        return Ok(());
    }

    let summary = order
        .into_iter()
        .map(|id| match counts.get(&id).copied().unwrap_or(1) {
            1 => id,
            count => format!("{id} (x{count})"),
        })
        .collect::<Vec<String>>()
        .join(", ");

    Err(format!(
        "pricing is unavailable for submitted token usage: {summary}"
    ))
}
