use std::time::Duration;

use smol::channel::{Receiver, Sender};

pub(super) struct LimitsRefreshSignal(Sender<()>);

impl Default for LimitsRefreshSignal {
    fn default() -> Self {
        let (sender, receiver) = smol::channel::bounded(1);
        drop(receiver);
        Self(sender)
    }
}

impl LimitsRefreshSignal {
    pub(super) fn request(&self) {
        let _ = self.0.try_send(());
    }
}

pub(super) fn channel() -> (LimitsRefreshSignal, Receiver<()>) {
    let (sender, receiver) = smol::channel::bounded(1);
    (LimitsRefreshSignal(sender), receiver)
}

pub(super) async fn wait(requests: &Receiver<()>, interval: Duration) {
    smol::future::race(
        async move {
            smol::Timer::after(interval).await;
        },
        async move {
            let _ = requests.recv().await;
        },
    )
    .await;
}
