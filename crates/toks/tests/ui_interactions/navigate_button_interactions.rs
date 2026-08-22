#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Entity, Modifiers, MouseButton, NavigationDirection,
    TestAppContext, VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions,
};
use gpui_component::TitleBar;
use toks::test_support::{current_page, initialize, sidebar_open, WindowFrame};
use toks::{Page, ToksApp};
use toks_core::history::{HistorySnapshot, ModelUsage, SourceHistory, UsageBucket, UsageSeries};

struct Harness {
    app: Entity<ToksApp>,
    cx: &'static mut VisualTestContext,
}

impl Harness {
    fn open(cx: &mut TestAppContext, viewport_width: f32) -> Self {
        initialize(cx);
        let now = Utc
            .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
            .single()
            .expect("valid fixture timestamp");
        let app =
            cx.new(|_| ToksApp::from_snapshots(Some(history(now.timestamp_millis())), vec![], now));
        let content = app.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(viewport_width), px(900.)),
                    ))),
                    window_background: WindowBackgroundAppearance::Opaque,
                    window_decorations: Some(WindowDecorations::Client),
                    titlebar: Some(TitleBar::title_bar_options()),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| WindowFrame::new(content)),
            )
            .expect("headless window opens")
        });
        let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
        cx.run_until_parked();
        Self { app, cx }
    }

    fn press(&mut self, direction: NavigationDirection) {
        let center = point(px(700.), px(450.));
        self.cx
            .simulate_mouse_move(center, None::<MouseButton>, Modifiers::none());
        self.cx
            .simulate_mouse_down(center, MouseButton::Navigate(direction), Modifiers::none());
        self.cx
            .simulate_mouse_up(center, MouseButton::Navigate(direction), Modifiers::none());
        self.cx.run_until_parked();
    }

    fn click(&mut self, selector: &'static str) {
        let position = self
            .cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
            .center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
        self.cx.simulate_click(position, Modifiers::none());
        self.cx.run_until_parked();
    }

    fn page(&mut self) -> Page {
        self.app.read_with(self.cx, |app, _| current_page(app))
    }

    fn has(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }
}

#[gpui::test]
fn mouse_back_and_forward_follow_visited_tabs(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, 1600.);
    assert!(harness.has("overview-usage-card"));

    harness.click("rotation");
    assert_eq!(harness.page(), Page::Rotation);
    harness.press(NavigationDirection::Back);
    assert_eq!(harness.page(), Page::Overview);
    harness.press(NavigationDirection::Forward);
    assert_eq!(harness.page(), Page::Rotation);

    harness.click("daily");
    assert_eq!(harness.page(), Page::Daily);
    harness.press(NavigationDirection::Back);
    assert_eq!(harness.page(), Page::Rotation);
    harness.press(NavigationDirection::Back);
    assert_eq!(harness.page(), Page::Overview);
    harness.press(NavigationDirection::Forward);
    assert_eq!(harness.page(), Page::Rotation);

    harness.click("monthly");
    harness.press(NavigationDirection::Back);
    assert_eq!(harness.page(), Page::Rotation);
    harness.press(NavigationDirection::Forward);
    assert_eq!(harness.page(), Page::Monthly);
}

#[gpui::test]
fn navigate_buttons_dismiss_a_compact_overlay_sidebar(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, 900.);
    harness.click("toggle-sidebar");
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));

    harness.press(NavigationDirection::Forward);
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
}

fn history(generated_at_ms: i64) -> HistorySnapshot {
    let model = ModelUsage {
        model: "gpt-test".into(),
        provider: "openai".into(),
        input: 40,
        output: 10,
        tokens: 50,
        messages: 2,
        turns: 1,
        cost: 0.5,
        ..Default::default()
    };
    let source = |client: &'static str| SourceHistory {
        client: client.into(),
        total_tokens: 70,
        total_cost: 0.7,
        models: vec![model.clone()],
        usage: UsageSeries {
            daily: vec![bucket("2026-08-18", 50, model.clone())],
            ..Default::default()
        },
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source("codex"), source("opencode")],
        usage: UsageSeries {
            daily: vec![bucket("2026-08-18", 100, model)],
            ..Default::default()
        },
        generated_at_ms,
        ..Default::default()
    }
}

fn bucket(key: &str, tokens: i64, model: ModelUsage) -> UsageBucket {
    UsageBucket {
        key: key.into(),
        tokens,
        cost: tokens as f64 / 100.0,
        models: vec![model],
        ..Default::default()
    }
}
