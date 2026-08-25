use std::time::Duration;

use super::engine::SnapshotApplication;
use super::Engine;
use super::RouterRuntimeHandle;
use crate::accounts::ProviderLimitCollection;

pub(crate) async fn heartbeat(runtime: RouterRuntimeHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        if let Some((snapshots, observed_at)) = refresh_quota(&runtime.engine, || {
            crate::accounts::collect_provider_limits(crate::limits::Provider::Codex)
        })
        .await
        {
            let _ = crate::codex_router::account_activation::observe_and_launch(
                &snapshots,
                observed_at,
            );
        }
    }
}

pub(crate) async fn refresh_quota<F>(
    engine: &Engine,
    collect: F,
) -> Option<(ProviderLimitCollection, chrono::DateTime<chrono::Utc>)>
where
    F: Fn() -> ProviderLimitCollection + Clone + Send + 'static,
{
    for attempt in 0..2 {
        let epoch = engine.begin_snapshot_refresh().ok()?;
        let collect = collect.clone();
        let snapshots = tokio::task::spawn_blocking(collect).await.ok()?;
        let observed_at = chrono::Utc::now();
        match engine.apply_snapshots(&snapshots, &epoch, observed_at) {
            Ok(SnapshotApplication::Applied) => return Some((snapshots, observed_at)),
            Ok(SnapshotApplication::Refetch) if attempt == 0 => {}
            Ok(SnapshotApplication::Refetch) | Err(_) => return None,
        }
    }
    None
}
