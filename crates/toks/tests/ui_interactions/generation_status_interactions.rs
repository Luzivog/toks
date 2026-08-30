use chrono::{TimeZone, Utc};
use gpui::{px, size, TestAppContext};
use toks::{
    test_support::{set_page, set_rotation_service_active, set_router_deployment},
    Page, ToksApp,
};
use toks_core::{
    codex_router::{RouterDeploymentStatus, RouterGenerationRole, RouterGenerationSummary},
    rotation::UnixMillis,
};

use super::support::Harness;

#[gpui::test]
fn only_pending_and_draining_builds_render_in_the_status_card(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 24, 12, 0, 0)
        .single()
        .unwrap();
    let mut app = ToksApp::from_snapshots(None, Vec::new(), now);
    set_rotation_service_active(&mut app);
    set_router_deployment(
        &mut app,
        RouterDeploymentStatus {
            generations: vec![
                RouterGenerationSummary {
                    generation: 3,
                    build: "new-build-0123456789".into(),
                    role: RouterGenerationRole::Active,
                    task_count: Some(1),
                    oldest_task_at: Some(UnixMillis::new(now.timestamp_millis() - 60_000)),
                },
                RouterGenerationSummary {
                    generation: 2,
                    build: "next-build-0123456789".into(),
                    role: RouterGenerationRole::Pending,
                    task_count: Some(0),
                    oldest_task_at: None,
                },
                RouterGenerationSummary {
                    generation: 1,
                    build: "old-build-0123456789".into(),
                    role: RouterGenerationRole::Draining,
                    task_count: Some(2),
                    oldest_task_at: Some(UnixMillis::new(now.timestamp_millis() - 600_000)),
                },
            ],
            update_waiting: true,
        },
    );
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1000.), px(800.)));

    let card = harness.bounds("rotation-status-card");
    let controls = harness.bounds("rotation-router-controls");
    let pending = harness.bounds("rotation-router-generation-2");
    let draining = harness.bounds("rotation-router-generation-1");
    let update_waiting = harness.bounds("rotation-router-update-waiting");
    let remote = harness.bounds("rotation-remote-control-row");
    assert!(!harness.has("rotation-router-generation-3"));
    assert!(card.contains(&pending.center()));
    assert!(card.contains(&draining.center()));
    assert!(controls.bottom() <= pending.top());
    assert!(pending.bottom() <= draining.top());
    assert!(draining.bottom() <= remote.top());
    assert!(pending.size.height <= px(40.));
    assert!(draining.size.height <= px(40.));
    assert!(draining.contains(&update_waiting.center()));
    for selector in ["rotation-router-build-3", "rotation-router-workload-3"] {
        assert!(!harness.has(selector), "rendered {selector}");
    }
    for selector in [
        "rotation-router-build-2",
        "rotation-router-workload-2",
        "rotation-router-build-1",
        "rotation-router-workload-1",
    ] {
        assert!(harness.has(selector), "missing {selector}");
    }
}

#[gpui::test]
fn active_build_alone_keeps_the_generation_section_hidden(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 24, 12, 0, 0)
        .single()
        .unwrap();
    let mut app = ToksApp::from_snapshots(None, Vec::new(), now);
    set_rotation_service_active(&mut app);
    set_router_deployment(
        &mut app,
        RouterDeploymentStatus {
            generations: vec![RouterGenerationSummary {
                generation: 7,
                build: "steady-build".into(),
                role: RouterGenerationRole::Active,
                task_count: Some(1),
                oldest_task_at: Some(UnixMillis::new(now.timestamp_millis() - 60_000)),
            }],
            update_waiting: false,
        },
    );
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1000.), px(800.)));

    for selector in [
        "rotation-router-generations",
        "rotation-router-generation-7",
        "rotation-router-build-7",
        "rotation-router-workload-7",
    ] {
        assert!(!harness.has(selector), "rendered {selector}");
    }
}
