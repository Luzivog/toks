//! Tokscope application library and deterministic test seam.

mod app;
mod history_refresh;
mod palette;
mod shell;
mod sidebar_motion;
mod table_state;
mod title_bar;
mod ui;
pub mod window;
mod window_controls;

pub use app::{Page, TokscopeApp};
pub(crate) use table_state::{
    ModelSortColumn, ModelTablesState, SortDirection, SortState, UsageSortColumn, UsageTablesState,
    USAGE_PAGE_SIZE,
};

use gpui::{
    px, size, AppContext, Application, Bounds, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowOptions,
};
use gpui_component::{Theme, ThemeMode, TitleBar};
use window::{TokscopeAssets, WindowFrame};

/// Start the desktop application.
pub fn run() {
    Application::new().with_assets(TokscopeAssets).run(|cx| {
        initialize_theme(cx);
        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_background: WindowBackgroundAppearance::Opaque,
                    window_decorations: Some(WindowDecorations::Client),
                    titlebar: Some(TitleBar::title_bar_options()),
                    app_id: Some("tokscope".into()),
                    window_min_size: Some(size(px(940.), px(620.))),
                    ..Default::default()
                },
                |_window, cx| {
                    let view = cx.new(TokscopeApp::new);
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
pub mod test_support {
    pub use crate::window::{WindowAction, WindowFrame};
    use crate::{Page, TokscopeApp};

    /// Initialize GPUI Component and Tokscope's theme in a headless test app.
    pub fn initialize(cx: &mut gpui::TestAppContext) {
        cx.update(super::initialize_theme);
    }

    pub fn set_page(app: &mut TokscopeApp, page: Page) {
        app.page = page;
    }

    pub fn sidebar_open(app: &TokscopeApp) -> bool {
        app.sidebar_open
    }
}

#[cfg(test)]
mod app_tests;
