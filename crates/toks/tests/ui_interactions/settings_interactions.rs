#![cfg(feature = "test-support")]

use gpui::{px, size, TestAppContext};
use toks::test_support::current_page;
use toks::Page;

use super::support::Harness;

#[gpui::test]
fn settings_is_pinned_to_the_sidebar_bottom_and_opens_its_page(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Overview, size(px(1400.), px(900.)));
    let rail = harness.bounds("sidebar-rail");
    let rotation = harness.bounds("rotation");
    let settings = harness.bounds("settings");

    assert!(settings.top() > rotation.bottom());
    assert!(rail.bottom() - settings.bottom() <= px(16.));

    harness.click("settings");
    assert_eq!(
        harness
            .app
            .read_with(harness.cx, |app, _| current_page(app)),
        Page::Settings
    );
    harness.bounds("settings-page");
    harness.bounds("settings-provider-codex");
    harness.bounds("settings-provider-claude");
    harness.bounds("settings-provider-opencode");
}
