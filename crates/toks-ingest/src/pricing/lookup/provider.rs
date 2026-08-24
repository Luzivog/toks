mod hints;
mod known_prefix;
mod prefixes;
mod routing;
mod scoped;

pub(in crate::pricing::lookup) use hints::{build_lookup_cache_key, normalize_provider_hint};
pub(in crate::pricing::lookup) use known_prefix::lookup_known_provider_prefix;
pub(in crate::pricing::lookup) use prefixes::{
    strip_generic_provider_prefix, strip_known_provider_prefix,
};
pub(crate) use routing::is_routing_label;
pub(in crate::pricing::lookup) use scoped::parse_provider_scoped_model_path;

pub(in crate::pricing::lookup) const PROVIDER_PREFIXES: &[&str] = &[
    "openai/",
    "anthropic/",
    "google/",
    "meta-llama/",
    "mistralai/",
    "minimax/",
    "deepseek/",
    "qwen/",
    "cohere/",
    "perplexity/",
    "x-ai/",
];

pub(in crate::pricing::lookup) const RESELLER_PROVIDER_PREFIXES: &[&str] = &[
    "azure/",
    "azure_ai/",
    "bedrock/",
    "vertex_ai/",
    "together/",
    "together_ai/",
    "fireworks_ai/",
    "groq/",
    "openrouter/",
    "orcarouter/",
];
