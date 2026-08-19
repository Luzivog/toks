pub(super) fn cache_only() -> bool {
    crate::paths::renamed_env_var("TOKS_PRICING_CACHE_ONLY", "TOKSCOPE_PRICING_CACHE_ONLY")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}
