use std::time::Duration;

use gpui::{AppContext, Context};
use toks_core::remote_control::{self, RemoteControlSnapshot};

use crate::ToksApp;

#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteControlUiState {
    pub snapshot: RemoteControlSnapshot,
}

pub(super) fn spawn(cx: &mut Context<ToksApp>) {
    cx.spawn(async move |this, cx| loop {
        let result = cx
            .background_spawn(async move { remote_control::status().await })
            .await;
        if this
            .update(cx, |app, cx| {
                if let Ok(snapshot) = result {
                    app.rotation.remote.snapshot = snapshot;
                    cx.notify();
                }
            })
            .is_err()
        {
            break;
        }
        smol::Timer::after(Duration::from_secs(10)).await;
    })
    .detach();
}
