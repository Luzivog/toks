use gpui::{div, prelude::*, Hsla, SharedString};
use gpui_component::tooltip::Tooltip;
use toks_core::{LimitSnapshot, Provider};

const TOOLTIP: &str = "Redeeming one reset resets both the Codex 5-hour and weekly usage windows.";

pub(super) fn banked_reset_badge(
    snapshot: &LimitSnapshot,
    accent: Hsla,
) -> Option<impl IntoElement> {
    let label = banked_reset_label(snapshot.provider, snapshot.banked_resets)?;
    let selector = format!(
        "account-resets-{}-{}",
        snapshot.provider.slug(),
        snapshot.account.id
    );
    let id = selector.clone();
    Some(
        div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.clone())
            .px_1p5()
            .rounded_sm()
            .text_xs()
            .bg(accent.opacity(0.1))
            .text_color(accent.opacity(0.82))
            .child(label)
            .tooltip(|window, cx| {
                Tooltip::element(|_, _| {
                    div()
                        .debug_selector(|| "banked-reset-tooltip".to_string())
                        .child(TOOLTIP)
                })
                .build(window, cx)
            }),
    )
}

fn banked_reset_label(provider: Provider, count: u64) -> Option<String> {
    (provider == Provider::Codex && count > 0).then(|| match count {
        1 => "1 reset".to_string(),
        count => format!("{count} resets"),
    })
}

#[cfg(test)]
mod tests {
    use super::banked_reset_label;
    use toks_core::Provider;

    #[test]
    fn labels_positive_codex_resets_only() {
        assert_eq!(
            banked_reset_label(Provider::Codex, 1).as_deref(),
            Some("1 reset")
        );
        assert_eq!(
            banked_reset_label(Provider::Codex, 3).as_deref(),
            Some("3 resets")
        );
        assert_eq!(banked_reset_label(Provider::Codex, 0), None);
        assert_eq!(banked_reset_label(Provider::Claude, 3), None);
    }
}
