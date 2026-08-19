//! Headless verification tool: `toks-dump [limits|history|all]`.

fn main() -> anyhow::Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());

    if mode == "forget-history-range" {
        return forget_history_range();
    }

    if mode == "limits" || mode == "all" {
        let snapshots = toks_core::limits::collect_all();
        println!("{}", serde_json::to_string_pretty(&snapshots)?);
    }

    if mode == "history" || mode == "all" {
        let started = std::time::Instant::now();
        let history = toks_core::history::collect()?;
        eprintln!("history collected in {:?}", started.elapsed());
        // Full minute arrays are noisy; print a trimmed view.
        for s in &history.sources {
            let last_min_tokens: i64 = s.minutes.iter().rev().take(5).map(|m| m.tokens).sum();
            eprintln!(
                "{}: total ${:.2} / {} tok / {} msgs | today ${:.2} / {} tok | week ${:.2} | last-5min {} tok | models: {}",
                s.client,
                s.total_cost,
                s.total_tokens,
                s.total_messages,
                s.today_cost,
                s.today_tokens,
                s.week_cost,
                last_min_tokens,
                s.models.len()
            );
            for m in s.models.iter().take(5) {
                eprintln!(
                    "   {:<40} ${:>9.2}  in {:>12} out {:>10} cr {:>13} cw {:>12}",
                    m.model, m.cost, m.input, m.output, m.cache_read, m.cache_write
                );
            }
        }
        if history.unpriced {
            eprintln!("WARNING: no pricing data available — costs are zero");
        }
    }

    Ok(())
}

fn forget_history_range() -> anyhow::Result<()> {
    use chrono::{Local, NaiveDate, TimeZone};

    let mut arguments = std::env::args().skip(2);
    let start = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing start date"))?;
    let end = arguments
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing exclusive end date"))?;
    let local_midnight = |value: &str| -> anyhow::Result<i64> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
        let naive = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
        Local
            .from_local_datetime(&naive)
            .single()
            .map(|time| time.timestamp_millis())
            .ok_or_else(|| anyhow::anyhow!("date does not resolve to one local midnight"))
    };
    let removed = toks_core::history::forget_range(local_midnight(&start)?, local_midnight(&end)?)?;
    eprintln!("forgot {removed} retained usage events from {start} through {end} (exclusive)");
    Ok(())
}
