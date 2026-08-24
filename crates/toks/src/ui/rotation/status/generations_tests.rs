use super::generations::short_build;

#[test]
fn build_label_is_compact_without_losing_short_names() {
    assert_eq!(short_build("build-a"), "build-a");
    assert_eq!(short_build("0123456789abcdef"), "0123456789ab…");
}
