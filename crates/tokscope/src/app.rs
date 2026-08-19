use std::time::Duration;

use chrono::Utc;
use gpui::{AppContext, Context};
use tokscope_core::{history::UsagePeriod, HistorySnapshot, LimitSnapshot};

use crate::{
    history_refresh::HistoryRefreshState, sidebar_motion::SidebarMotion, ModelTablesState,
    UsageTablesState,
};

mod account_operations;
mod account_removals;
mod history_task;
pub(crate) use account_operations::AccountOperations;
pub(crate) use account_removals::{request_removal, AccountRemovals, RemovalStatus};

const LIMITS_REFRESH: Duration = Duration::from_secs(15);
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
    AllTime,
}

impl Page {
    pub fn usage_period(&self) -> Option<UsagePeriod> {
        match self {
            Page::Overview => None,
            Page::Hourly => Some(UsagePeriod::Hourly),
            Page::Daily => Some(UsagePeriod::Daily),
            Page::Monthly => Some(UsagePeriod::Monthly),
            Page::AllTime => None,
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
    pub(crate) history_refresh: HistoryRefreshState,
    pub(crate) account_notice: Option<String>,
    pub(crate) account_operations: AccountOperations,
    pub(crate) account_removals: AccountRemovals,
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
                if this
                    .update(cx, |app, _| {
                        for key in app.account_operations.authenticated_accounts() {
                            app.account_removals.allow(&key);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                let mut limits = cx
                    .background_spawn(async { tokscope_core::limits::collect_all() })
                    .await;
                tokscope_core::accounts::apply_saved_order(&mut limits);
                if this
                    .update(cx, |app, cx| {
                        app.account_removals.filter_refresh(&mut limits);
                        app.account_operations.reconcile(&mut limits, Utc::now());
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

        history_task::spawn(cx);

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
        let mut history_refresh = HistoryRefreshState::ready();
        history_refresh.complete(
            history
                .as_ref()
                .and_then(|snapshot| snapshot.captured_through_ms),
        );
        Self {
            page: Page::Overview,
            sidebar_open: true,
            limits,
            limits_loaded,
            history,
            history_error: None,
            history_refresh,
            account_notice: None,
            account_operations: AccountOperations::default(),
            account_removals: AccountRemovals::default(),
            emails_hidden: false,
            usage_tables: UsageTablesState::new(),
            model_tables: ModelTablesState::new(),
            now,
            compact_layout: None,
            sidebar_motion: SidebarMotion::new(),
        }
    }
}
