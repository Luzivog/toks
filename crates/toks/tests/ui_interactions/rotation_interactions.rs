use std::ops::Deref;

use chrono::Utc;
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::{
    test_support::{current_page, initialize, WindowFrame},
    Page, ToksApp,
};

#[gpui::test]
fn rotation_sidebar_entry_opens_the_private_dashboard(cx: &mut TestAppContext) {
    initialize(cx);
    let app = cx.new(|_| ToksApp::from_snapshots(None, Vec::new(), Utc::now()));
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

    let rotation = cx
        .debug_bounds("rotation")
        .expect("rotation sidebar entry is rendered")
        .center();
    cx.simulate_mouse_move(rotation, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(rotation, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(
        app.read_with(cx, |app, _| current_page(app)),
        Page::Rotation
    );
    assert!(cx.debug_bounds("rotation-page").is_some());
}
