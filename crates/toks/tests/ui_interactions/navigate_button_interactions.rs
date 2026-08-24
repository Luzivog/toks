#![cfg(feature = "test-support")]

use chrono::{TimeZone, Utc};
use gpui::{point, px, size, Modifiers, MouseButton, NavigationDirection, TestAppContext};
use toks::test_support::{current_page, sidebar_open};
use toks::{Page, ToksApp};

use super::support::{navigation_history, Harness};

#[gpui::test]
fn mouse_back_and_forward_follow_visited_tabs(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, app(), size(px(1600.), px(900.)));
    assert!(harness.has("overview-usage-card"));

    harness.click("rotation");
    assert_eq!(page(&mut harness), Page::Rotation);
    press(&mut harness, NavigationDirection::Back);
    assert_eq!(page(&mut harness), Page::Overview);
    press(&mut harness, NavigationDirection::Forward);
    assert_eq!(page(&mut harness), Page::Rotation);

    harness.click("daily");
    assert_eq!(page(&mut harness), Page::Daily);
    press(&mut harness, NavigationDirection::Back);
    assert_eq!(page(&mut harness), Page::Rotation);
    press(&mut harness, NavigationDirection::Back);
    assert_eq!(page(&mut harness), Page::Overview);
    press(&mut harness, NavigationDirection::Forward);
    assert_eq!(page(&mut harness), Page::Rotation);

    harness.click("monthly");
    press(&mut harness, NavigationDirection::Back);
    assert_eq!(page(&mut harness), Page::Rotation);
    press(&mut harness, NavigationDirection::Forward);
    assert_eq!(page(&mut harness), Page::Monthly);
}

#[gpui::test]
fn navigate_buttons_dismiss_a_compact_overlay_sidebar(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, app(), size(px(900.), px(900.)));
    harness.click("toggle-sidebar");
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));

    press(&mut harness, NavigationDirection::Forward);
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
}

fn app() -> ToksApp {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    ToksApp::from_snapshots(
        Some(navigation_history(now.timestamp_millis())),
        vec![],
        now,
    )
}

fn press(harness: &mut Harness, direction: NavigationDirection) {
    let center = point(px(700.), px(450.));
    harness
        .cx
        .simulate_mouse_move(center, None::<MouseButton>, Modifiers::none());
    harness
        .cx
        .simulate_mouse_down(center, MouseButton::Navigate(direction), Modifiers::none());
    harness
        .cx
        .simulate_mouse_up(center, MouseButton::Navigate(direction), Modifiers::none());
    harness.cx.run_until_parked();
}

fn page(harness: &mut Harness) -> Page {
    harness
        .app
        .read_with(harness.cx, |app, _| current_page(app))
}
