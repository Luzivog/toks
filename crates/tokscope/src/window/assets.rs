use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const EYE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/eye.svg"
));
const EYE_OFF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/eye-off.svg"
));
const PANEL_LEFT_CLOSE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/panel-left-close.svg"
));
const PANEL_LEFT_OPEN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/panel-left-open.svg"
));
const PLUS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/icons/plus.svg"
));

/// Repository-owned assets embedded in the executable at compile time.
pub(crate) struct TokscopeAssets;

impl AssetSource for TokscopeAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes = match path {
            "icons/eye.svg" => EYE,
            "icons/eye-off.svg" => EYE_OFF,
            "icons/panel-left-close.svg" => PANEL_LEFT_CLOSE,
            "icons/panel-left-open.svg" => PANEL_LEFT_OPEN,
            "icons/plus.svg" => PLUS,
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.trim_end_matches('/') != "icons" {
            return Ok(Vec::new());
        }
        Ok([
            "eye.svg",
            "eye-off.svg",
            "panel-left-close.svg",
            "panel-left-open.svg",
            "plus.svg",
        ]
        .into_iter()
        .map(SharedString::from)
        .collect())
    }
}
