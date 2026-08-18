#![cfg(feature = "test-support")]

use std::{ops::Deref, time::Duration};

use chrono::Utc;
use gpui::{
    point, px, size, AppContext, Bounds, Entity, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use tokscope::test_support::{initialize, sidebar_open, WindowFrame};
use tokscope::TokscopeApp;

struct Harness {
    app: Entity<TokscopeApp>,
    cx: &'static mut VisualTestContext,
}

impl Harness {
    fn open(cx: &mut TestAppContext) -> Self {
        initialize(cx);
        let app = cx.new(|_| TokscopeApp::from_snapshots(None, Vec::new(), Utc::now()));
        let content = app.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1400.), px(800.)),
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

    fn sidebar_width(&mut self) -> gpui::Pixels {
        self.cx
            .debug_bounds("sidebar-rail")
            .expect("wide sidebar rail is rendered")
            .size
            .width
    }

    fn settle_motion(&mut self) {
        std::thread::sleep(Duration::from_millis(230));
        self.app.update(self.cx, |_, cx| cx.notify());
        self.cx.run_until_parked();
    }
}

#[gpui::test]
fn wide_sidebar_animates_closed_and_open_without_losing_its_toggle(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx);
    assert_eq!(harness.sidebar_width(), px(250.));

    harness.click("toggle-sidebar");
    harness.settle_motion();
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert_eq!(harness.sidebar_width(), px(0.));

    harness.click("toggle-sidebar");
    harness.settle_motion();
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert_eq!(harness.sidebar_width(), px(250.));
}
