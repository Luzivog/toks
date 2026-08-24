use anyhow::Result;

pub(super) fn record(errors: &mut Vec<String>, label: &str, result: Result<()>) {
    if let Err(error) = result {
        errors.push(format!("{label}: {error:#}"));
    }
}

pub(super) fn finish(errors: Vec<String>) -> Result<()> {
    if errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "resume supervisor completed with errors: {}",
            errors.join("; ")
        )
    }
}
