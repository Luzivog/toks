use gpui::{App, Hsla};
use gpui_component::ActiveTheme;
use tokscope_core::{limits::LimitWindow, Provider};

use crate::Page;

pub(crate) fn claude_accent() -> Hsla {
    gpui::rgb(0xd97757).into()
}

pub(crate) fn codex_accent() -> Hsla {
    gpui::rgb(0xe6e6e4).into()
}

pub(crate) fn page_accent(page: Page, cx: &App) -> Hsla {
    match page {
        Page::Overview => cx.theme().muted_foreground,
        Page::Hourly => gpui::rgb(0x5f_a8_d3).into(),
        Page::Daily => gpui::rgb(0xe4_a8_53).into(),
        Page::Monthly => gpui::rgb(0xa7_8b_fa).into(),
        Page::AllTime => gpui::rgb(0x72_c7_a5).into(),
    }
}

pub(super) fn accent_for_provider(provider: Provider) -> Hsla {
    match provider {
        Provider::Claude => claude_accent(),
        Provider::Codex => codex_accent(),
    }
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

// ---------------------------------------------------------------------------
// Sidebar
