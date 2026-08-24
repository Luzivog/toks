use super::format::datetime;
use toks_core::rotation::UnixMillis;

#[test]
fn invalid_unix_millis_does_not_invent_a_date() {
    assert!(datetime(UnixMillis::new(i64::MAX)).is_none());
}
