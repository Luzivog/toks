use super::embedded::dataset;

#[test]
fn baseline_has_usable_openai_and_anthropic_rows() {
    let prices = dataset();

    for model in ["openai/gpt-5.6-sol", "anthropic/claude-fable-5"] {
        let row = prices.get(model).expect("baseline model must be present");
        assert!(row.has_any_usable_base_rate());
        assert!(row.cache_read_input_token_cost.is_some());
    }
}
