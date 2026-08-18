use gpui::{prelude::*, svg, Svg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokscopeIcon {
    Eye,
    EyeOff,
    PanelLeftClose,
    PanelLeftOpen,
    Plus,
}

impl TokscopeIcon {
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::Eye => "icons/eye.svg",
            Self::EyeOff => "icons/eye-off.svg",
            Self::PanelLeftClose => "icons/panel-left-close.svg",
            Self::PanelLeftOpen => "icons/panel-left-open.svg",
            Self::Plus => "icons/plus.svg",
        }
    }
}

pub(crate) fn icon_element(icon: TokscopeIcon) -> Svg {
    // GPUI renders SVG assets as alpha masks, so the color must live on the
    // SVG element itself; parent button text colors are not inherited.
    svg().path(icon.path()).text_color(gpui::white())
}
