use super::*;

fn model_pricing(input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        input_cost_per_token: Some(input),
        output_cost_per_token: Some(output),
        ..Default::default()
    }
}

fn custom_service(
    custom: HashMap<String, ModelPricing>,
    litellm: HashMap<String, ModelPricing>,
    openrouter: HashMap<String, ModelPricing>,
) -> PricingService {
    PricingService::new_with_custom(CustomPricing::from_models(custom), litellm, openrouter)
}

fn fixture_models_dev() -> HashMap<String, ModelPricing> {
    models_dev::parse_dataset(
        r#"{
            "openai": {"models": {
                "gpt-fixture-model": {"id": "gpt-fixture-model", "cost": {
                    "input": 1.25, "output": 10.0,
                    "cache_read": 0.125, "cache_write": 1.875
                }},
                "missing-output-price": {"cost": {"input": 1.0}}
            }},
            "anthropic": {"models": {
                "claude-fixture-sonnet": {"cost": {"input": 3.0, "output": 15.0}}
            }}
        }"#,
    )
    .unwrap()
}

fn custom_service_with_models_dev(
    custom: HashMap<String, ModelPricing>,
    litellm: HashMap<String, ModelPricing>,
    openrouter: HashMap<String, ModelPricing>,
    models_dev: HashMap<String, ModelPricing>,
) -> PricingService {
    PricingService::new_with_custom_and_models_dev(
        CustomPricing::from_models(custom),
        litellm,
        openrouter,
        models_dev,
    )
}

fn cache_read_usage() -> TokenBreakdown {
    TokenBreakdown {
        input: 1_000_000,
        output: 0,
        cache_read: 1_000_000,
        cache_write: 0,
        reasoning: 0,
    }
}

mod catalog;
mod coverage;
mod cursor;
mod custom;
mod sources;
