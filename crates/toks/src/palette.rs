use gpui::rgb;
use gpui_component::Theme;

/// Near-black surfaces and hairline borders shared by every page.
pub(super) fn apply(cx: &mut gpui::App) {
    let theme = Theme::global_mut(cx);
    theme.background = rgb(0x0a0a0a).into();
    theme.foreground = rgb(0xededed).into();
    theme.muted_foreground = rgb(0x858585).into();
    theme.sidebar = rgb(0x0d0d0d).into();
    theme.sidebar_foreground = rgb(0xededed).into();
    theme.sidebar_border = rgb(0x1e1e1e).into();
    theme.sidebar_accent = rgb(0x1a1a1a).into();
    theme.sidebar_accent_foreground = rgb(0xededed).into();
    theme.skeleton = rgb(0x2a2a2a).into();
    theme.secondary = rgb(0x121212).into();
    theme.secondary_foreground = rgb(0xededed).into();
    theme.secondary_hover = rgb(0x1a1a1a).into();
    theme.secondary_active = rgb(0x212121).into();
    theme.popover = rgb(0x121212).into();
    theme.popover_foreground = rgb(0xededed).into();
    theme.border = rgb(0x212121).into();
    theme.window_border = rgb(0x212121).into();
    theme.title_bar = rgb(0x0a0a0a).into();
    theme.title_bar_border = rgb(0x0a0a0a).into();
}
