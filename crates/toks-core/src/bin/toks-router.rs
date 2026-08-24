use anyhow::Result;
fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (None, None) => runtime()?.block_on(toks_core::codex_router::run_router()),
        (Some("install-service"), None) => toks_core::codex_router::install_router_service(),
        (Some("install-service"), Some(installed_link)) if arguments.next().is_none() => {
            toks_core::codex_router::install_router_service_for(installed_link.as_ref())
        }
        (Some("launch-host"), None) => toks_core::codex_router::launch_router_host(),
        (Some("host"), None) => runtime()?.block_on(toks_core::codex_router::run_router_host()),
        (Some("launch-resume-supervisor"), None) => {
            toks_core::codex_router::launch_router_resume_supervisor()
        }
        (Some("resume-supervisor"), None) => {
            runtime()?.block_on(toks_core::codex_router::run_resume_supervisor())
        }
        (Some("launch-resume-task"), Some(encoded)) if arguments.next().is_none() => {
            toks_core::codex_router::launch_router_resume_task(&encoded)
        }
        (Some("resume-task"), Some(attempt)) => {
            let thread = arguments.next().ok_or_else(|| anyhow::anyhow!("missing thread"))?;
            let cwd = arguments
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing workspace"))?;
            anyhow::ensure!(arguments.next().is_none(), "unexpected resume-task argument");
            runtime()?.block_on(toks_core::codex_router::run_resume_task(
                &attempt,
                &thread,
                cwd.into(),
            ))
        }
        (Some("worker"), Some(generation)) if arguments.next().is_none() => {
            let generation = generation.parse::<u64>()?;
            runtime()?.block_on(toks_core::codex_router::run_router_worker(generation))
        }
        (Some("launch-worker"), Some(generation)) => {
            let contract = arguments
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing launch contract"))?;
            anyhow::ensure!(arguments.next().is_none(), "unexpected launch-worker argument");
            toks_core::codex_router::launch_router_worker(generation.parse()?, contract.as_ref())
        }
        _ => anyhow::bail!(
            "usage: toks-router [install-service [installed-link] | launch-host | host | launch-resume-supervisor | resume-supervisor | launch-resume-task <payload> | resume-task <attempt> <thread> <workspace> | worker <generation> | launch-worker <generation> <contract>]"
        ),
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Into::into)
}
