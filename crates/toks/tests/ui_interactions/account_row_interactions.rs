use gpui::{point, px, Modifiers, MouseButton, TestAppContext};
use toks::Page;
use toks_core::Provider;

use super::support::{limit_snapshot, Harness, VIEWPORT};

#[gpui::test]
fn account_handle_drops_onto_another_account_row(cx: &mut TestAppContext) {
    let previous_data_home = std::env::var_os("XDG_DATA_HOME");
    let data_home =
        std::env::temp_dir().join(format!("toks-account-drag-test-{}", std::process::id()));
    std::fs::create_dir_all(&data_home).expect("temporary XDG data home");
    std::env::set_var("XDG_DATA_HOME", &data_home);
    let limits = vec![
        limit_snapshot(Provider::Codex, "first"),
        limit_snapshot(Provider::Claude, "second"),
    ];
    let mut harness = Harness::open_with_limits(cx, Page::Overview, VIEWPORT, limits);
    assert!(harness.above("account-drop-codex-first", "account-drop-claude-second"));
    let start = harness.bounds("account-drag-codex-first").center();
    let end = harness.bounds("account-drop-claude-second").center();
    harness
        .cx
        .simulate_mouse_move(start, None::<MouseButton>, Modifiers::none());
    harness
        .cx
        .simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    harness.cx.simulate_mouse_move(
        point(start.x + px(12.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    harness
        .cx
        .simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    harness
        .cx
        .simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    harness.cx.run_until_parked();

    assert!(harness.above("account-drop-claude-second", "account-drop-codex-first"));
    if let Some(previous) = previous_data_home {
        std::env::set_var("XDG_DATA_HOME", previous);
    } else {
        std::env::remove_var("XDG_DATA_HOME");
    }
    std::fs::remove_dir_all(data_home).expect("temporary account order is removable");
}

#[gpui::test]
fn plan_limits_render_as_distinct_account_and_quota_rows(cx: &mut TestAppContext) {
    let limits = vec![limit_snapshot(Provider::Codex, "readable")];
    let mut harness = Harness::open_with_limits(cx, Page::Overview, VIEWPORT, limits);

    assert!(harness.has("account-group-codex-readable"));
    assert!(harness.has("account-status-codex-readable"));
    let quota = harness.bounds("quota-row-weekly");
    assert!(quota.size.height >= px(40.));
}
