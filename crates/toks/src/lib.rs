//! Toks application library and deterministic test seam.

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
pub mod test_support {
    pub use crate::window::{WindowAction, WindowFrame};
    use crate::{Page, ToksApp};

    /// Initialize GPUI Component and Toks's theme in a headless test app.
    pub fn initialize(cx: &mut gpui::TestAppContext) {
        cx.update(super::initialize_theme);
    }

    pub fn set_page(app: &mut ToksApp, page: Page) {
        app.page = page;
    }

    pub fn current_page(app: &ToksApp) -> Page {
        app.page
    }

    pub fn sidebar_open(app: &ToksApp) -> bool {
        app.sidebar_open
    }

    pub fn emails_hidden(app: &ToksApp) -> bool {
        app.emails_hidden
    }

    pub fn prepare_rotation_accounts(app: &mut ToksApp) {
        let accounts: Vec<_> = app
            .limits
            .iter()
            .filter(|snapshot| snapshot.provider == toks_core::Provider::Codex)
            .map(|snapshot| snapshot.account.id.clone())
            .collect();
        app.rotation.settings.reconcile(&accounts);
        app.rotation.runtime.reconcile(
            &accounts,
            toks_core::rotation::UnixMillis::new(app.now.timestamp_millis()),
        );
    }

    pub fn set_rotation_active_streams(app: &mut ToksApp, account: &str, count: u32) {
        let account = toks_core::accounts::AccountId::new(account);
        let at = toks_core::rotation::UnixMillis::new(app.now.timestamp_millis());
        for index in 0..count {
            app.rotation.runtime.connection_opened(
                &account,
                &toks_core::rotation::ThreadId::new(format!("fixture-{index}")),
                at,
            );
        }
    }

    pub fn set_rotation_blocked(app: &mut ToksApp, account: &str) {
        let account = toks_core::accounts::AccountId::new(account);
        let at = toks_core::rotation::UnixMillis::new(app.now.timestamp_millis());
        app.rotation.runtime.block(
            &account,
            toks_core::rotation::UnixMillis::new(at.get() + 86_400_000),
            true,
            at,
        );
    }
}

#[cfg(test)]
mod app_tests;
