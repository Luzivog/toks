use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::codex_router::host::BuildId;

use super::command::{
    coordinator_matches_until, health_check_until, is_unit_active_until, resume_matches_until,
};
use super::{receipt, socket_contract, RESUME_NAME, ROUTER_PORT, UNIT_NAME};

pub(super) fn wait(
    executable: &Path,
    candidate: &BuildId,
    process_environment: &BTreeMap<String, Option<String>>,
    deadline: Instant,
) -> Result<()> {
    let state = receipt::deployment_state_path()?;
    let address = SocketAddr::from(([127, 0, 0, 1], ROUTER_PORT));
    while Instant::now() < deadline {
        let ready = iteration(
            || socket_contract::ensure_active_candidate_until(deadline),
            || {
                let worker = match receipt::active_candidate_generation(&state, candidate)? {
                    Some(generation) => is_unit_active_until(
                        &format!("toks-router-worker@{generation}.service"),
                        deadline,
                    )?,
                    None => false,
                };
                Ok(is_unit_active_until(UNIT_NAME, deadline)?
                    && is_unit_active_until(RESUME_NAME, deadline)?
                    && coordinator_matches_until(executable, process_environment, deadline)?
                    && resume_matches_until(executable, process_environment, deadline)?
                    && worker
                    && health_check_until(address, deadline).is_ok())
            },
        )?;
        if ready {
            return Ok(());
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100)),
        );
    }
    anyhow::bail!("router candidate did not become the active deployment")
}

fn iteration(
    mut socket_check: impl FnMut() -> Result<()>,
    mut candidate_check: impl FnMut() -> Result<bool>,
) -> Result<bool> {
    socket_check()?;
    let ready = candidate_check()?;
    if ready {
        socket_check()?;
    }
    Ok(ready)
}

#[cfg(test)]
mod tests {
    use super::iteration;

    #[test]
    fn every_iteration_checks_the_socket_and_ready_state_checks_it_again() {
        let mut calls = 0;
        for _ in 0..2 {
            assert!(!iteration(
                || {
                    calls += 1;
                    Ok(())
                },
                || Ok(false),
            )
            .unwrap());
        }
        assert_eq!(calls, 2);

        let error = iteration(
            || {
                calls += 1;
                (calls < 4)
                    .then_some(())
                    .ok_or_else(|| anyhow::anyhow!("socket deactivated"))
            },
            || Ok(true),
        )
        .unwrap_err();
        assert!(error.to_string().contains("deactivated"));
        assert_eq!(calls, 4);
    }
}
