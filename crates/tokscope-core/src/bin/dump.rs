//! Headless verification tool: `tokscope-dump [limits|history|all]`.

fn main() -> anyhow::Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());

    if mode == "limits" || mode == "all" {
        let snapshots = tokscope_core::limits::collect_all();
        println!("{}", serde_json::to_string_pretty(&snapshots)?);
    }

    if mode == "history" || mode == "all" {
        let started = std::time::Instant::now();
        let history = tokscope_core::history::collect()?;
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
