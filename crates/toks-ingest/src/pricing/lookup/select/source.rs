use super::provider_rank::{
    is_original_provider, is_reseller_provider, key_root_is_cross_provider_alias,
    key_root_matches_provider_hint,
};
use crate::pricing::lookup::LookupResult;
use crate::provider_identity;

pub(in crate::pricing::lookup) fn choose_best_source_result(
    litellm_result: Option<LookupResult>,
    openrouter_result: Option<LookupResult>,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    match (&litellm_result, &openrouter_result) {
        (Some(l), Some(o)) => {
            let l_matches_provider =
                provider_identity::matches_provider_hint(&l.matched_key, provider_id);
            let o_matches_provider =
                provider_identity::matches_provider_hint(&o.matched_key, provider_id);

            if l_matches_provider && !o_matches_provider {
                return litellm_result;
            }
            if o_matches_provider && !l_matches_provider {
                return openrouter_result;
            }

            let l_matches_root = provider_id
                .is_some_and(|hint| key_root_matches_provider_hint(&l.matched_key, hint));
            let o_matches_root = provider_id
                .is_some_and(|hint| key_root_matches_provider_hint(&o.matched_key, hint));
            if l_matches_root && !o_matches_root {
                return litellm_result;
            }
            if o_matches_root && !l_matches_root {
                return openrouter_result;
            }

            let l_is_original = is_original_provider(&l.matched_key);
            let o_is_original = is_original_provider(&o.matched_key);
            let l_is_reseller = is_reseller_provider(&l.matched_key);
            let o_is_reseller = is_reseller_provider(&o.matched_key);

            if o_is_original && !l_is_original {
                return openrouter_result;
            }
            if l_is_original && !o_is_original {
                return litellm_result;
            }
            if !l_is_reseller && o_is_reseller {
                return litellm_result;
            }
            if !o_is_reseller && l_is_reseller {
                return openrouter_result;
            }

            litellm_result
        }
        (Some(_), None) => litellm_result,
        (None, Some(_)) => openrouter_result,
        (None, None) => None,
    }
}

/// Run the normal LiteLLM/OpenRouter arbitration, but let a literal
/// provider-root match from Models.dev displace an alias-only winner. Models.dev
/// otherwise remains the long-tail fallback at its established precedence.
pub(in crate::pricing::lookup) fn choose_best_source_result_with_models_dev(
    litellm_result: Option<LookupResult>,
    openrouter_result: Option<LookupResult>,
    models_dev_result: Option<LookupResult>,
    provider_id: Option<&str>,
) -> Option<LookupResult> {
    let primary = choose_best_source_result(litellm_result, openrouter_result, provider_id);
    let models_dev_matches_root = models_dev_result.as_ref().is_some_and(|result| {
        provider_id.is_some_and(|hint| key_root_matches_provider_hint(&result.matched_key, hint))
    });
    let primary_is_cross_provider_alias = primary.as_ref().is_some_and(|result| {
        provider_id.is_some_and(|hint| key_root_is_cross_provider_alias(&result.matched_key, hint))
    });

    if models_dev_matches_root && primary_is_cross_provider_alias {
        models_dev_result
    } else {
        primary
    }
}
