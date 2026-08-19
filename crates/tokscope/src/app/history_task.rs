use std::{sync::Arc, time::Instant};

use anyhow::Result;
use gpui::{AppContext, Context};
use tokscope_core::history::{HistoryStatus, HistoryView, LocalHistory};

use super::TokscopeApp;

/// Hydrate last-good history first, then own the only refresh worker.
pub(super) fn spawn(cx: &mut Context<TokscopeApp>) {
    cx.spawn(async move |this, cx| {
        let history = Arc::new(LocalHistory::open_default());
        let hydrated = cx
            .background_spawn({
                let history = Arc::clone(&history);
                async move { history.hydrate() }
            })
            .await;
        if publish(&this, hydrated, cx).is_err() {
            return;
        }

        loop {
            let cycle_started = Instant::now();
            if this
                .update(cx, |app, cx| {
                    app.history_refresh.begin_cycle();
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
            let result = cx
                .background_spawn({
                    let history = Arc::clone(&history);
                    async move { history.refresh() }
                })
                .await;
            let updated = match result {
                Ok(view) => publish(&this, view, cx),
                Err(error) => this.update(cx, |app, cx| {
                    app.history_error = Some(error.to_string());
                    app.history_refresh.busy_using_last_good(None);
                    cx.notify();
                }),
            };
            if updated.is_err() {
                break;
            }
            let delay = this
                .update(cx, |app, _| {
                    app.history_refresh.next_delay(cycle_started.elapsed())
                })
                .unwrap_or_default();
            if !delay.is_zero() {
                smol::Timer::after(delay).await;
            }
        }
    })
    .detach();
}

fn publish(
    this: &gpui::WeakEntity<TokscopeApp>,
    view: HistoryView,
    cx: &mut gpui::AsyncApp,
) -> Result<()> {
    this.update(cx, |app, cx| {
        let snapshot_capture = view
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.captured_through_ms);
        if view
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| should_publish(app.history.as_ref(), snapshot))
        {
            app.history = view.snapshot;
        }
        let has_warning = view.warning.is_some();
        app.history_error = view.warning;
        match view.status {
            HistoryStatus::Ready if has_warning => {
                app.history_refresh.fresh_save_delayed(snapshot_capture)
            }
            HistoryStatus::Ready => app.history_refresh.complete(snapshot_capture),
            HistoryStatus::CatchingUp {
                pending_sources,
                captured_through_ms,
                retry,
            } => app
                .history_refresh
                .catching_up(pending_sources, captured_through_ms, retry),
            HistoryStatus::BusyUsingLastGood {
                captured_through_ms,
                ..
            } => app
                .history_refresh
                .busy_using_last_good(captured_through_ms),
        }
        cx.notify();
    })
}

fn should_publish(
    current: Option<&tokscope_core::HistorySnapshot>,
    candidate: &tokscope_core::HistorySnapshot,
) -> bool {
    current.is_none_or(|current| candidate.generated_at_ms >= current.generated_at_ms)
}

#[cfg(test)]
mod tests;
