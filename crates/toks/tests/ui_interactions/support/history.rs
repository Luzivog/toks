use chrono::NaiveDate;
use toks_core::history::{
    DaySlice, HistorySnapshot, MinuteSlice, ModelUsage, SourceHistory, UsageBucket, UsageSeries,
};

use super::fixture_now;

pub(crate) fn navigation_history(generated_at_ms: i64) -> HistorySnapshot {
    let model = simple_model();
    let source = |client: &'static str| SourceHistory {
        client: client.into(),
        total_tokens: 70,
        total_cost: 0.7,
        models: vec![model.clone()],
        usage: UsageSeries {
            daily: vec![usage_bucket("2026-08-18", 50, model.clone())],
            ..Default::default()
        },
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source("codex"), source("opencode")],
        usage: UsageSeries {
            daily: vec![usage_bucket("2026-08-18", 100, model)],
            ..Default::default()
        },
        generated_at_ms,
        ..Default::default()
    }
}

pub(crate) fn usage_history(generated_at_ms: i64) -> HistorySnapshot {
    let model = simple_model();
    let monthly = vec![
        usage_bucket("2026-06", 20, model.clone()),
        usage_bucket("2026-08", 50, model.clone()),
    ];
    let daily = vec![usage_bucket("2026-08-18", 50, model.clone())];
    let source = SourceHistory {
        client: "codex".into(),
        total_tokens: 70,
        total_cost: 0.7,
        models: vec![model],
        usage: UsageSeries {
            daily: daily.clone(),
            monthly: monthly.clone(),
            ..Default::default()
        },
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source],
        usage: UsageSeries {
            daily,
            monthly,
            ..Default::default()
        },
        generated_at_ms,
        ..Default::default()
    }
}

fn simple_model() -> ModelUsage {
    ModelUsage {
        model: "gpt-test".into(),
        provider: "openai".into(),
        input: 40,
        output: 10,
        tokens: 50,
        messages: 2,
        turns: 1,
        cost: 0.5,
        ..Default::default()
    }
}

fn usage_bucket(key: &str, tokens: i64, model: ModelUsage) -> UsageBucket {
    UsageBucket {
        key: key.into(),
        tokens,
        cost: tokens as f64 / 100.0,
        models: vec![model],
        ..Default::default()
    }
}

pub(super) fn sortable_history() -> HistorySnapshot {
    let models = sortable_models();
    let hourly = (0..12)
        .map(|hour| {
            let value = match hour {
                2 => 1_000,
                11 => 1,
                _ => 20 + hour,
            };
            sortable_bucket(format!("2026-08-18 {hour:02}:00"), value, models.clone())
        })
        .collect::<Vec<_>>();
    let start = NaiveDate::from_ymd_opt(2026, 8, 7).expect("valid fixture date");
    let daily = (0..12)
        .map(|offset| {
            let date = start + chrono::Duration::days(offset);
            let value = match offset {
                1 => 1_000,
                11 => 1,
                _ => 20 + offset,
            };
            sortable_bucket(date.format("%Y-%m-%d").to_string(), value, models.clone())
        })
        .collect::<Vec<_>>();
    let month_keys = [
        "2025-09", "2025-10", "2025-11", "2025-12", "2026-01", "2026-02", "2026-03", "2026-04",
        "2026-05", "2026-06", "2026-07", "2026-08",
    ];
    let monthly = month_keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let value = match index {
                1 => 1_000,
                11 => 1,
                _ => 20 + index as i64,
            };
            sortable_bucket((*key).to_string(), value, models.clone())
        })
        .collect::<Vec<_>>();
    let usage = UsageSeries {
        daily,
        hourly,
        monthly,
    };
    let generated_at_ms = fixture_now().timestamp_millis();
    let source = SourceHistory {
        client: "codex".into(),
        minutes: vec![MinuteSlice {
            minute: generated_at_ms.div_euclid(60_000),
            tokens: 10_000,
            cost: 10.0,
            models: models.clone(),
        }],
        days: vec![DaySlice {
            date: "2026-08-18".into(),
            tokens: 10_000,
            cost: 10.0,
            messages: 100,
        }],
        usage: usage.clone(),
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source],
        usage,
        generated_at_ms,
        ..Default::default()
    }
}

fn sortable_models() -> Vec<ModelUsage> {
    vec![sortable_model("large", 1_000), sortable_model("small", 1)]
}

fn sortable_model(name: &str, value: i64) -> ModelUsage {
    ModelUsage {
        model: name.into(),
        provider: "openai".into(),
        input: value,
        output: value,
        cache_read: value,
        cache_write: value,
        reasoning: value,
        tokens: value.saturating_mul(5),
        messages: value,
        turns: value,
        cost: (value as f64).powi(2),
        ..Default::default()
    }
}

fn sortable_bucket(key: String, value: i64, models: Vec<ModelUsage>) -> UsageBucket {
    UsageBucket {
        key,
        input: value,
        output: value,
        cache_read: value,
        cache_write: value,
        reasoning: value,
        tokens: value.saturating_mul(5),
        messages: value,
        turns: value,
        cost: (value as f64).powi(2),
        models,
        ..Default::default()
    }
}
