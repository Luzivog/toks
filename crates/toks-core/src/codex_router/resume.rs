use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::rotation::{RotationSettings, RotationSettingsStore, ThreadId, WaitingThread};

use super::proxy::RouterRuntimeHandle;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
const CONTINUE_PROMPT: &str =
    "Continue the interrupted task from where it stopped. Do not repeat completed work.";

pub(super) async fn run(runtime: RouterRuntimeHandle) {
    let mut retry_after = BTreeMap::new();
    loop {
        let _ = try_resume(&runtime, &mut retry_after).await;
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn try_resume(
    runtime: &RouterRuntimeHandle,
    retry_after: &mut BTreeMap<ThreadId, Instant>,
) -> anyhow::Result<()> {
    let Some(account) = runtime.eligible_account()? else {
        return Ok(());
    };
    let waiting = runtime.waiting_threads();
    let settings = RotationSettingsStore::discover()?.load()?;
    let Some(thread) = next_waiting(&settings, &waiting, retry_after, Instant::now()) else {
        return Ok(());
    };
    if !runtime.claim_waiting(&thread, &account)? {
        return Ok(());
    }
    let result = run_codex(thread.clone()).await;
    if !result.is_ok_and(|status| status.success()) {
        runtime.waiting(&thread)?;
        retry_after.insert(thread, Instant::now() + RETRY_DELAY);
    } else {
        retry_after.remove(&thread);
    }
    Ok(())
}

fn next_waiting(
    settings: &RotationSettings,
    waiting: &[WaitingThread],
    retry_after: &BTreeMap<ThreadId, Instant>,
    now: Instant,
) -> Option<ThreadId> {
    let live = waiting
        .iter()
        .map(|waiting| waiting.thread_id.clone())
        .collect::<BTreeSet<_>>();
    settings
        .waiting_priority()
        .iter()
        .chain(waiting.iter().map(|waiting| &waiting.thread_id))
        .find(|thread| {
            live.contains(*thread)
                && !settings.cancelled_threads().contains(*thread)
                && retry_after.get(*thread).is_none_or(|after| *after <= now)
        })
        .cloned()
}

async fn run_codex(thread: ThreadId) -> anyhow::Result<std::process::ExitStatus> {
    let executable = super::codex_binary::discover()?;
    tokio::task::spawn_blocking(move || {
        resume_command(&executable, &thread)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    })
    .await?
    .map_err(Into::into)
}

fn resume_command(executable: &std::path::Path, thread: &ThreadId) -> Command {
    let mut command = Command::new(executable);
    command
        .args(["exec", "--skip-git-repo-check", "resume", "--all"])
        .arg(thread.as_str())
        .arg(CONTINUE_PROMPT);
    command
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use crate::rotation::{RotationSettings, ThreadId, UnixMillis, WaitingThread};

    use super::{next_waiting, resume_command, CONTINUE_PROMPT};

    #[test]
    fn queue_order_honors_user_priority_cancellation_and_runtime_fallback() {
        let first = ThreadId::new("first");
        let second = ThreadId::new("second");
        let third = ThreadId::new("third");
        let waiting = [&first, &second, &third]
            .into_iter()
            .enumerate()
            .map(|(index, thread)| WaitingThread {
                thread_id: thread.clone(),
                since: UnixMillis::new(index as i64),
            })
            .collect::<Vec<_>>();
        let mut settings = RotationSettings::default();
        settings.reconcile_waiting(&[first.clone(), second.clone()]);
        settings.move_waiting_to(&second, 0);
        settings.cancel_waiting(&second);

        assert_eq!(
            next_waiting(&settings, &waiting, &BTreeMap::new(), Instant::now()),
            Some(first)
        );
    }

    #[test]
    fn continuation_resumes_the_same_thread_without_a_shell() {
        let command = resume_command(std::path::Path::new("/opt/codex"), &ThreadId::new("thread"));
        assert_eq!(command.get_program(), "/opt/codex");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "exec",
                "--skip-git-repo-check",
                "resume",
                "--all",
                "thread",
                CONTINUE_PROMPT,
            ]
        );
    }
}
