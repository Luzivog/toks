use std::time::Duration;

use chrono::Utc;
use gpui::{AppContext, Context};
use tokscope_core::{history::UsagePeriod, HistorySnapshot, LimitSnapshot};

use crate::{sidebar_motion::SidebarMotion, ModelTablesState, UsageTablesState};

const LIMITS_REFRESH: Duration = Duration::from_secs(15);
const HISTORY_REFRESH: Duration = Duration::from_secs(60);
pub(super) fn sidebar_open_for_layout(
    currently_open: bool,
    previous_compact_layout: Option<bool>,
    compact_layout: bool,
) -> bool {
    if previous_compact_layout == Some(compact_layout) {
        currently_open
    } else {
        !compact_layout
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Hourly,
    Daily,
    Monthly,
}

impl Page {
    pub fn usage_period(&self) -> Option<UsagePeriod> {
        match self {
            Page::Overview => None,
            Page::Hourly => Some(UsagePeriod::Hourly),
            Page::Daily => Some(UsagePeriod::Daily),
            Page::Monthly => Some(UsagePeriod::Monthly),
        }
    }
}

pub struct TokscopeApp {
    pub(crate) page: Page,
    pub(crate) sidebar_open: bool,
    pub(crate) limits: Vec<LimitSnapshot>,
    pub(crate) limits_loaded: bool,
    pub(crate) history: Option<HistorySnapshot>,
    pub(crate) history_error: Option<String>,
    pub(crate) account_notice: Option<String>,
    pub(crate) emails_hidden: bool,
    pub(crate) usage_tables: UsageTablesState,
    pub(crate) model_tables: ModelTablesState,
    pub(crate) now: chrono::DateTime<Utc>,
    pub(super) compact_layout: Option<bool>,
    pub(super) sidebar_motion: SidebarMotion,
}

impl TokscopeApp {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        // Hydrate Tokscope's last-good snapshots before any provider request,
        // then refresh live data independently in the background.
        cx.spawn(async move |this, cx| {
            let mut hydrated = cx
                .background_spawn(async { tokscope_core::limits::hydrate_all() })
                .await;
            tokscope_core::accounts::apply_saved_order(&mut hydrated);
            if this
                .update(cx, |app, cx| {
                    app.limits = hydrated;
                    app.limits_loaded = true;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            loop {
                let mut limits = cx
                    .background_spawn(async { tokscope_core::limits::collect_all() })
                    .await;
                tokscope_core::accounts::apply_saved_order(&mut limits);
                if this
                    .update(cx, |app, cx| {
                        app.limits = limits;
                        app.limits_loaded = true;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                smol::Timer::after(LIMITS_REFRESH).await;
            }
        })
        .detach();

        // Countdown labels must keep advancing even while a provider request is
        // delayed or being backed off.
        cx.spawn(async move |this, cx| loop {
            smol::Timer::after(LIMITS_REFRESH).await;
            if this
                .update(cx, |app, cx| {
                    app.now = Utc::now();
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        })
        .detach();

        // Publish the last complete aggregate immediately, then refresh the
        // scanner in the background. A failed scan never replaces last-good
        // history with an empty page.
        cx.spawn(async move |this, cx| {
            let hydrated = cx
                .background_spawn(async { tokscope_core::history::hydrate() })
                .await;
            if this
                .update(cx, |app, cx| {
                    if hydrated.is_some() {
                        app.history = hydrated;
                    }
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            loop {
                let result = cx
                    .background_spawn(async { tokscope_core::history::collect() })
                    .await;
                let ok = this.update(cx, |app, cx| {
                    match result {
                        Ok(h) => {
                            app.history = Some(h);
                            app.history_error = None;
                        }
                        Err(e) => app.history_error = Some(e.to_string()),
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
                smol::Timer::after(HISTORY_REFRESH).await;
            }
        })
        .detach();

        Self::from_snapshots(None, Vec::new(), Utc::now())
    }

    /// Construct the render state without starting filesystem or network work.
    ///
    /// The production constructor owns refresh scheduling; tests and headless
    /// renderers cross this pure seam with deterministic snapshots instead.
    pub fn from_snapshots(
        history: Option<HistorySnapshot>,
        limits: Vec<LimitSnapshot>,
        now: chrono::DateTime<Utc>,
    ) -> Self {
        let limits_loaded = !limits.is_empty();
        Self {
            page: Page::Overview,
            sidebar_open: true,
            limits,
            limits_loaded,
            history,
            history_error: None,
            account_notice: None,
            emails_hidden: false,
            usage_tables: UsageTablesState::new(),
            model_tables: ModelTablesState::new(),
            now,
            compact_layout: None,
            sidebar_motion: SidebarMotion::new(),
        }
    }
}
