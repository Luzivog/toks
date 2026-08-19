use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use super::{load_order, save_order, AccountOrderKey};

/// Remove exact logical or legacy profile keys from persisted display order.
/// Missing order state is already clean and therefore succeeds.
pub(in crate::accounts) fn remove_accounts_at(
    path: &Path,
    keys: &[AccountOrderKey],
) -> Result<bool> {
    let mut order = match load_order(path) {
        Ok(order) => order,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(false)
        }
        Err(error) => return Err(error),
    };
    let before = order.len();
    let removed: HashSet<_> = keys.iter().collect();
    order.retain(|key| !removed.contains(key));
    if order.len() == before {
        return Ok(false);
    }
    save_order(path, &order)?;
    Ok(true)
}
