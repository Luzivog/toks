use std::collections::HashMap;

use crate::LimitSnapshot;

use super::AccountOrderKey;

/// Match both new logical-account ranks and legacy credential-profile ranks.
pub(super) fn snapshot_rank(
    snapshot: &LimitSnapshot,
    logical_key: &AccountOrderKey,
    ranks: &HashMap<&AccountOrderKey, usize>,
) -> usize {
    ranks.get(logical_key).copied().unwrap_or_else(|| {
        snapshot
            .account
            .sources
            .iter()
            .filter_map(|source| {
                ranks.get(&AccountOrderKey::new(
                    snapshot.provider,
                    source.profile_id.as_str(),
                ))
            })
            .copied()
            .min()
            .unwrap_or(usize::MAX)
    })
}
