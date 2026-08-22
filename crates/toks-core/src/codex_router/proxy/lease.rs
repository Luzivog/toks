use std::sync::Arc;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::engine::Engine;

pub(super) struct StreamLease {
    engine: Arc<Engine>,
    account: AccountId,
}

impl StreamLease {
    pub fn open(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
    ) -> anyhow::Result<Self> {
        engine.route(account, thread)?;
        Ok(Self {
            engine,
            account: account.clone(),
        })
    }
}

impl Drop for StreamLease {
    fn drop(&mut self) {
        let _ = self.engine.close(&self.account);
    }
}
