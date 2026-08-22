use std::time::Duration;

use chrono::Utc;
use gpui::{AppContext, Context};
use toks_core::{history::UsagePeriod, HistorySnapshot, LimitSnapshot};

use crate::{
    history_refresh::HistoryRefreshState, sidebar_motion::SidebarMotion, ModelTablesState,
    UsageTablesState,
};

mod account_operations;
mod account_removals;
pub(crate) mod banked_reset_operations;
mod history_task;
mod rotation_operations;
pub(crate) use account_operations::AccountOperations;
pub(crate) use account_removals::{request_removal, AccountRemovals, RemovalStatus};
use banked_reset_operations::BankedResetOperations;
pub(crate) use rotation_operations::{RotationServiceAction, RotationUiState, SettingsAction};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Hourly,
    Daily,
    Monthly,
    AllTime,
    Rotation,
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::Overview,
        Page::Hourly,
        Page::Daily,
        Page::Monthly,
        Page::AllTime,
        Page::Rotation,
    ];

    pub fn usage_period(&self) -> Option<UsagePeriod> {
        match self {
            Page::Overview | Page::AllTime | Page::Rotation => None,
            Page::Hourly => Some(UsagePeriod::Hourly),
            Page::Daily => Some(UsagePeriod::Daily),
            Page::Monthly => Some(UsagePeriod::Monthly),
        }
    }

    /// The neighbor `delta` steps away in sidebar order, clamped at the ends.
    pub fn shifted(self, delta: isize) -> Page {
        let index = Page::ALL.iter().position(|page| *page == self).unwrap_or(0) as isize;
        let next = (index + delta).clamp(0, Page::ALL.len() as isize - 1);
        Page::ALL[next as usize]
    }
}

pub struct ToksApp {
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
    pub(crate) banked_resets: BankedResetOperations,
    pub(crate) emails_hidden: bool,
    pub(crate) usage_tables: UsageTablesState,
    pub(crate) model_tables: ModelTablesState,
    pub(crate) rotation: RotationUiState,
    pub(crate) now: chrono::DateTime<Utc>,
    pub(super) compact_layout: Option<bool>,
    pub(super) sidebar_motion: SidebarMotion,
}

impl ToksApp {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        // Paint last-good snapshots before starting provider requests.
        cx.spawn(async move |this, cx| {
            let mut hydrated = cx
                .background_spawn(async { toks_core::limits::hydrate_all() })
                .await;
            toks_core::accounts::apply_saved_order(&mut hydrated);
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
                    .background_spawn(async { toks_core::limits::collect_all() })
                    .await;
                toks_core::accounts::apply_saved_order(&mut limits);
                if this
                    .update(cx, |app, cx| {
                        app.account_removals.filter_refresh(&mut limits);
                        app.account_operations.reconcile(&mut limits, Utc::now());
                        app.banked_resets.reconcile(&limits);
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
        rotation_operations::spawn(cx);

        Self::from_snapshots(None, Vec::new(), Utc::now())
    }

    /// Construct deterministic render state without filesystem or network work.
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
            banked_resets: BankedResetOperations::default(),
            emails_hidden: false,
            usage_tables: UsageTablesState::new(),
            model_tables: ModelTablesState::new(),
            rotation: RotationUiState::default(),
            now,
            compact_layout: None,
            sidebar_motion: SidebarMotion::new(),
        }
    }
}
