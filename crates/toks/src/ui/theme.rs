use gpui::{App, Hsla};
use gpui_component::ActiveTheme;
use toks_core::{limits::LimitWindow, Provider};

use crate::Page;

pub(crate) fn claude_accent() -> Hsla {
    gpui::rgb(0xd97757).into()
}

pub(crate) fn codex_accent() -> Hsla {
    gpui::rgb(0xe6e6e4).into()
}

pub(crate) fn opencode_accent() -> Hsla {
    gpui::rgb(0x4d_a3_ff).into()
}

pub(crate) fn page_accent(page: Page, cx: &App) -> Hsla {
    match page {
        Page::Overview => cx.theme().muted_foreground,
        Page::Hourly => gpui::rgb(0x5f_a8_d3).into(),
        Page::Daily => gpui::rgb(0xe4_a8_53).into(),
        Page::Monthly => gpui::rgb(0xa7_8b_fa).into(),
        Page::AllTime => gpui::rgb(0x72_c7_a5).into(),
        Page::Rotation => gpui::rgb(0x10_a3_7f).into(),
    }
}

pub(super) fn accent_for_provider(provider: Provider) -> Hsla {
    match provider {
        Provider::Claude => claude_accent(),
        Provider::Codex => codex_accent(),
    }
}

pub(super) fn accent_for_model_provider(provider: &str) -> Hsla {
    let provider = provider.to_ascii_lowercase();
    if contains_provider_marker(&provider, &["anthropic", "claude"]) {
        return accent_for_provider(Provider::Claude);
    }

    // The account-limit Provider enum has no OpenCode variant. History can
    // retain compound IDs such as `zenmux`, so these markers intentionally
    // preserve the existing blue grouping for those IDs too.
    if contains_provider_marker(
        &provider,
        &["opencode", "google", "gemini", "zen", "xai", "grok"],
    ) {
        return opencode_accent();
    }

    accent_for_provider(Provider::Codex)
}

fn contains_provider_marker(provider: &str, expected: &[&str]) -> bool {
    expected.iter().any(|marker| provider.contains(marker))
}

pub(super) fn gauge_color(w: &LimitWindow, accent: Hsla, cx: &App) -> Hsla {
    match w.severity.as_deref() {
        Some("warning") | Some("elevated") => cx.theme().warning,
        Some("critical") | Some("exceeded") => cx.theme().danger,
        _ if w.percent_used >= 95.0 => cx.theme().danger,
        _ if w.percent_used >= 80.0 => cx.theme().warning,
        _ => accent,
    }
}
