use chrono::{DateTime, Local, Utc};

pub(crate) fn fmt_tokens(n: i64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.2}B", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.2}M", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.1}k", n / 1e3)
    } else {
        format!("{n:.0}")
    }
}

/// Full-precision cost with thousands separators, for the headline number:
/// `$17,195.83`.
pub(crate) fn fmt_cost_full(c: f64) -> String {
    let cents = (c * 100.0).round() as i64;
    let dollars = cents / 100;
    let rem = (cents % 100).abs();
    let mut s = dollars.abs().to_string();
    let mut grouped = String::new();
    while s.len() > 3 {
        let split = s.len() - 3;
        grouped = format!(",{}{grouped}", &s[split..]);
        s.truncate(split);
    }
    format!("${s}{grouped}.{rem:02}")
}

pub(super) fn cost_per_million(cost: f64, tokens: i64) -> Option<f64> {
    (tokens > 0 && cost.is_finite() && cost >= 0.0).then(|| cost * 1_000_000.0 / tokens as f64)
}

pub(super) fn fmt_cost_per_million(cost: f64, tokens: i64) -> String {
    cost_per_million(cost, tokens)
        .map(fmt_cost_full)
        .unwrap_or_else(|| "—".into())
}

pub(super) fn fmt_reset(now: DateTime<Utc>, at: Option<DateTime<Utc>>) -> String {
    let Some(at) = at else {
        return "No scheduled reset".into();
    };
    let delta = at - now;
    if delta.num_seconds() <= 0 {
        return "Previous window".into();
    }
    let mins = delta.num_minutes();
    if mins >= 24 * 60 {
        format!(
            "resets in {}d {}h",
            mins / (24 * 60),
            (mins % (24 * 60)) / 60
        )
    } else if mins >= 60 {
        format!("resets in {}h {:02}m", mins / 60, mins % 60)
    } else {
        format!("resets in {}m", mins.max(1))
    }
}

pub(super) fn fmt_exact_local(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&Local);
    let zone = local.format("%Z").to_string();
    let offset = local.format("%:z").to_string();
    format!(
        "{} {}",
        local.format("%b %-d, %Y, %-I:%M %p"),
        zone_suffix(&zone, &offset)
    )
}

fn zone_suffix(zone: &str, offset: &str) -> String {
    if zone == offset {
        offset.to_owned()
    } else {
        format!("{zone} ({offset})")
    }
}

pub(super) fn fmt_age(now: DateTime<Utc>, at: DateTime<Utc>) -> String {
    let seconds = (now - at).num_seconds().max(0);
    if seconds < 60 {
        "just now".into()
    } else if seconds < 60 * 60 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h ago", seconds / (60 * 60))
    } else {
        format!("{}d ago", seconds / (24 * 60 * 60))
    }
}

pub(super) fn fmt_as_of(at: DateTime<Utc>) -> String {
    format!("as of {}", at.with_timezone(&Local).format("%b %-d, %H:%M"))
}

#[cfg(test)]
mod tests {
    use super::zone_suffix;

    #[test]
    fn numeric_timezone_is_not_repeated() {
        assert_eq!(zone_suffix("+02:00", "+02:00"), "+02:00");
    }

    #[test]
    fn named_timezone_keeps_its_numeric_offset() {
        assert_eq!(zone_suffix("CEST", "+02:00"), "CEST (+02:00)");
    }
}

// ---------------------------------------------------------------------------
// Accents
// ---------------------------------------------------------------------------
