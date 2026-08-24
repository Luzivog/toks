use std::time::Duration;

use super::RouterRuntimeHandle;

pub(crate) async fn heartbeat(runtime: RouterRuntimeHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let Ok(epoch) = runtime.engine.begin_snapshot_refresh() else {
            continue;
        };
        let snapshots = tokio::task::spawn_blocking(|| {
            crate::accounts::collect_provider_limits(crate::limits::Provider::Codex)
        })
        .await;
        if let Ok(snapshots) = snapshots {
            let observed_at = chrono::Utc::now();
            if runtime
                .engine
                .apply_snapshots(&snapshots, &epoch, observed_at)
                .is_ok()
            {
                let _ =
                    super::super::account_activation::observe_and_launch(&snapshots, observed_at);
            }
        }
    }
}
