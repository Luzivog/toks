use anyhow::Result;
fn main() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(toks_core::codex_router::run_router())
}
