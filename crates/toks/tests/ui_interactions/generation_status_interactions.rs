use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Pixels, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::{
    test_support::{
        initialize, set_page, set_rotation_service_active, set_router_deployment, WindowFrame,
    },
    Page, ToksApp,
};
use toks_core::{
    codex_router::{RouterDeploymentStatus, RouterGenerationRole, RouterGenerationSummary},
    rotation::UnixMillis,
};

#[gpui::test]
fn active_and_draining_builds_stay_compact_in_the_status_card(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 24, 12, 0, 0)
        .single()
        .unwrap();
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(None, Vec::new(), now);
        set_rotation_service_active(&mut app);
        set_router_deployment(
            &mut app,
            RouterDeploymentStatus {
                generations: vec![
                    RouterGenerationSummary {
                        generation: 2,
                        build: "new-build-0123456789".into(),
                        role: RouterGenerationRole::Active,
                        task_count: 1,
                        oldest_task_at: Some(UnixMillis::new(now.timestamp_millis() - 60_000)),
                    },
                    RouterGenerationSummary {
                        generation: 1,
                        build: "old-build-0123456789".into(),
                        role: RouterGenerationRole::Draining,
                        task_count: 2,
                        oldest_task_at: Some(UnixMillis::new(now.timestamp_millis() - 600_000)),
                    },
                ],
                update_waiting: true,
            },
        );
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app);

    let card = bounds(cx, "rotation-status-card");
    let controls = bounds(cx, "rotation-router-controls");
    let active = bounds(cx, "rotation-router-generation-2");
    let draining = bounds(cx, "rotation-router-generation-1");
    let remote = bounds(cx, "rotation-remote-control-row");
    assert!(card.contains(&active.center()));
    assert!(card.contains(&draining.center()));
    assert!(controls.bottom() <= active.top());
    assert!(active.bottom() <= draining.top());
    assert!(draining.bottom() <= remote.top());
    assert!(active.size.height <= px(40.));
    assert!(draining.size.height <= px(40.));
    assert!(cx.debug_bounds("rotation-router-update-waiting").is_some());
    for selector in [
        "rotation-router-build-2",
        "rotation-router-workload-2",
        "rotation-router-build-1",
        "rotation-router-workload-1",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
}

fn harness(cx: &mut TestAppContext, app: &gpui::Entity<ToksApp>) -> &'static mut VisualTestContext {
    initialize(cx);
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(800.)),
                ))),
                window_background: WindowBackgroundAppearance::Opaque,
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| WindowFrame::new(content)),
        )
        .unwrap()
    });
    let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    cx.run_until_parked();
    cx
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
}
