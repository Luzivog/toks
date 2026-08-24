use std::ops::Deref;

use chrono::{DateTime, TimeZone, Utc};
use gpui::{
    point, px, AppContext, Bounds, Entity, Modifiers, MouseButton, Pixels, Size, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::test_support::{initialize, set_page, WindowFrame};
use toks::{Page, ToksApp};
use toks_core::LimitSnapshot;

#[path = "support/history.rs"]
mod history;
#[path = "support/limits.rs"]
mod limits;

use history::sortable_history;
pub(super) use history::{navigation_history, usage_history};
pub(super) use limits::{
    account_removal_snapshot, banked_reset_snapshot, failed_snapshot, limit_snapshot,
    privacy_snapshot, remote_control_snapshot, rotation_limit_snapshot,
};

pub(super) const VIEWPORT: Size<Pixels> = gpui::size(px(1600.0), px(1800.0));

pub(super) struct Harness {
    pub(super) app: Entity<ToksApp>,
    pub(super) frame: Entity<WindowFrame>,
    pub(super) cx: &'static mut VisualTestContext,
}

impl Harness {
    pub(super) fn open(cx: &mut TestAppContext, app: ToksApp, viewport: Size<Pixels>) -> Self {
        initialize(cx);
        let app = cx.new(|_| app);
        let content = app.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.0), px(0.0)),
                        viewport,
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
        let frame = window.root(cx).expect("window has a root frame");
        let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
        cx.run_until_parked();
        Self { app, frame, cx }
    }

    pub(super) fn open_page(cx: &mut TestAppContext, page: Page, viewport: Size<Pixels>) -> Self {
        Self::open_with_limits(cx, page, viewport, Vec::new())
    }

    pub(super) fn open_with_limits(
        cx: &mut TestAppContext,
        page: Page,
        viewport: Size<Pixels>,
        limits: Vec<LimitSnapshot>,
    ) -> Self {
        let now = fixture_now();
        let mut app = ToksApp::from_snapshots(Some(sortable_history()), limits, now);
        set_page(&mut app, page);
        Self::open(cx, app, viewport)
    }

    pub(super) fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
        self.cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
    }

    pub(super) fn has(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }

    pub(super) fn move_to(&mut self, selector: &'static str) {
        let position = self.bounds(selector).center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
    }

    pub(super) fn click(&mut self, selector: &'static str) {
        let position = self.bounds(selector).center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
        self.cx.simulate_click(position, Modifiers::none());
        self.cx.run_until_parked();
    }

    pub(super) fn click_after_resize_edge(&mut self, selector: &'static str) {
        self.move_to("resize-right");
        self.click(selector);
    }

    pub(super) fn above(&mut self, first: &'static str, second: &'static str) -> bool {
        self.bounds(first).center().y < self.bounds(second).center().y
    }
}

fn fixture_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}
