pub fn normalize_agent_name(agent: &str) -> String {
    let cleaned = strip_zero_width_chars(agent);
    let stripped = strip_agent_prefix(cleaned.trim());
    let canonical = canonicalize_agent_name(stripped);
    let agent_lower = canonical.to_lowercase();

    if agent_lower.contains("plan") {
        if agent_lower.contains("omo") || agent_lower.contains("sisyphus") {
            return "Planner-Sisyphus".to_string();
        }
        return titlecase_agent(&canonical);
    }
    if agent_lower == "omo" || agent_lower == "sisyphus" {
        return "Sisyphus".to_string();
    }
    if agent_lower == "orchestrator-sisyphus" {
        return "Atlas".to_string();
    }
    titlecase_agent(&canonical)
}

pub fn normalize_opencode_agent_name(agent: &str) -> String {
    let cleaned = strip_zero_width_chars(agent);
    let stripped = strip_agent_prefix(cleaned.trim());
    let canonical = canonicalize_agent_name(stripped);
    normalize_oh_my_opencode_agent_name(&canonical.to_lowercase())
        .unwrap_or_else(|| normalize_agent_name(&canonical))
}

pub fn normalize_copilot_agent_name(agent: &str) -> String {
    if agent.eq_ignore_ascii_case("github.copilot.default") {
        return "GitHub Copilot".to_string();
    }

    const GITHUB_COPILOT_PREFIX: &str = "github.copilot.";
    if agent
        .get(..GITHUB_COPILOT_PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(GITHUB_COPILOT_PREFIX))
    {
        return titlecase_agent(&agent[GITHUB_COPILOT_PREFIX.len()..].replace('.', "-"));
    }

    const PLUGIN_PREFIX: &str = "Plugin:";
    if agent
        .get(..PLUGIN_PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(PLUGIN_PREFIX))
    {
        let rest = &agent[PLUGIN_PREFIX.len()..];
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        return if parts.len() == 2 {
            format!(
                "{}: {}",
                titlecase_agent(parts[0]),
                titlecase_agent(parts[1])
            )
        } else {
            titlecase_agent(rest)
        };
    }
    normalize_agent_name(agent)
}

fn normalize_oh_my_opencode_agent_name(agent_lower: &str) -> Option<String> {
    let normalized = match agent_lower {
        "sisyphus (ultraworker)"
        | "sisyphus - ultraworker"
        | "sisyphus ultraworker"
        | "sisyphus" => "Sisyphus",
        "hephaestus (deep agent)"
        | "hephaestus - deep agent"
        | "hephaestus deep agent"
        | "hephaestus" => "Hephaestus",
        "prometheus (plan builder)"
        | "prometheus - plan builder"
        | "prometheus plan builder"
        | "prometheus (planner)"
        | "prometheus" => "Prometheus",
        "atlas (plan executor)" | "atlas - plan executor" | "atlas plan executor" | "atlas" => {
            "Atlas"
        }
        "metis (plan consultant)"
        | "metis - plan consultant"
        | "metis plan consultant"
        | "metis" => "Metis",
        "momus (plan critic)"
        | "momus - plan critic"
        | "momus plan critic"
        | "momus (plan reviewer)"
        | "momus" => "Momus",
        "orchestrator-sisyphus" => "Atlas",
        "sisyphus-junior" => "Sisyphus-Junior",
        "planner-sisyphus" => "Planner-Sisyphus",
        _ => return None,
    };
    Some(normalized.to_string())
}

/// Remove invisible sort-order prefixes used by oh-my-openagent.
pub(super) fn strip_zero_width_chars(s: &str) -> String {
    if !s.contains(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}']) {
        return s.to_string();
    }
    s.chars()
        .filter(|c| !matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}'))
        .collect()
}

fn strip_agent_prefix(name: &str) -> &str {
    for prefix in &["astrape:", "oh-my-claudecode:", "oh-my-codex:"] {
        if name
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return &name[prefix.len()..];
        }
    }
    name
}

fn canonicalize_agent_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn titlecase_word(word: &str) -> String {
    match word.to_lowercase().as_str() {
        "ui" => "UI".to_string(),
        "ux" => "UX".to_string(),
        "api" => "API".to_string(),
        _ => {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + &chars.collect::<String>()
            })
        }
    }
}

fn titlecase_agent(name: &str) -> String {
    name.split('-')
        .flat_map(|part| part.split_whitespace())
        .map(titlecase_word)
        .collect::<Vec<_>>()
        .join(" ")
}
