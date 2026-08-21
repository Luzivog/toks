pub(super) const LEGACY_IDENTITY: u32 = 6;
pub(super) const JSON_WIRE_COMPATIBLE: u32 = 7;

pub(super) fn current() -> u32 {
    crate::message_cache::parser_version(crate::ClientId::Codex)
}

pub(super) fn is_current(version: u32) -> bool {
    version == current() || version == JSON_WIRE_COMPATIBLE
}
