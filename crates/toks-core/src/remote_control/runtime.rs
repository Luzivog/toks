use std::{future::Future, sync::LazyLock};

use anyhow::{Context, Result};
use tokio::runtime::{Builder, Runtime};

static RUNTIME: LazyLock<std::result::Result<Runtime, String>> = LazyLock::new(|| {
    Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("toks-remote-control")
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
});

struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) async fn run<T, F>(future: F) -> Result<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    let runtime = RUNTIME
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))?;
    let task = runtime.spawn(future);
    let _abort_on_drop = AbortOnDrop(task.abort_handle());
    task.await.context("Remote Control task stopped")?
}
