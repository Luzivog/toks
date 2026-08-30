//! Toks application library and deterministic test seam.

mod app;
#[cfg(test)]
mod app_tests;
mod history_refresh;
mod palette;
mod shell;
mod sidebar_motion;
mod table_state;
mod title_bar;
mod ui;
pub mod window;
mod window_controls;

pub use app::{Page, ToksApp};
pub(crate) use table_state::{
    ModelSortColumn, ModelTablesState, SortDirection, SortState, UsageSortColumn, UsageTablesState,
    USAGE_PAGE_SIZE,
};

use gpui::{
    px, size, AppContext, Application, Bounds, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowOptions,
};
use gpui_component::{Theme, ThemeMode, TitleBar};
use window::{ToksAssets, WindowFrame};

/// Start the desktop application.
pub fn run() {
    Application::new().with_assets(ToksAssets).run(|cx| {
        initialize_theme(cx);
        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_background: WindowBackgroundAppearance::Opaque,
                    window_decorations: Some(WindowDecorations::Client),
                    titlebar: Some(TitleBar::title_bar_options()),
                    app_id: Some("toks".into()),
                    window_min_size: Some(size(px(940.), px(620.))),
                    ..Default::default()
                },
                |_window, cx| {
                    let view = cx.new(ToksApp::new);
                    cx.new(|_| WindowFrame::new(view))
                },
            )?;
            Ok::<_, anyhow::Error>(())
        })
        .detach();
        cx.activate(true);
    });
}

fn initialize_theme(cx: &mut gpui::App) {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    palette::apply(cx);
}

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;
