#![cfg(feature = "test-support")]

use std::time::Duration;

use chrono::Utc;
use gpui::{px, size, TestAppContext};
use toks::test_support::sidebar_open;
use toks::{Page, ToksApp};

use super::support::Harness;

#[gpui::test]
fn wide_sidebar_animates_closed_and_open_without_losing_its_toggle(cx: &mut TestAppContext) {
    let app = ToksApp::from_snapshots(None, Vec::new(), Utc::now());
    let mut harness = Harness::open(cx, app, size(px(1400.), px(800.)));
    assert_eq!(sidebar_width(&mut harness), px(250.));

    harness.click("toggle-sidebar");
    settle_motion(&mut harness);
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert_eq!(sidebar_width(&mut harness), px(0.));

    harness.click("toggle-sidebar");
    settle_motion(&mut harness);
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert_eq!(sidebar_width(&mut harness), px(250.));
}

#[gpui::test]
fn compact_sidebar_backdrop_closes_the_overlay(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Overview, size(px(1000.0), px(900.0)));
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));

    harness.click_after_resize_edge("toggle-sidebar");
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert!(harness.has("sidebar-dismiss"));

    harness.click_after_resize_edge("sidebar-dismiss");
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
}

fn sidebar_width(harness: &mut Harness) -> gpui::Pixels {
    harness.bounds("sidebar-rail").size.width
}

fn settle_motion(harness: &mut Harness) {
    std::thread::sleep(Duration::from_millis(230));
    harness.app.update(harness.cx, |_, cx| cx.notify());
    harness.cx.run_until_parked();
}
