use toks_core::{
    accounts::AccountId,
    rotation::{ActiveTaskRow, TaskActivity, ThreadId, ThreadRequestSettings, UnixMillis},
};

use super::state::{ActiveTaskProjection, RotationUiState};

fn row(id: &str, account_id: &AccountId) -> ActiveTaskRow {
    ActiveTaskRow {
        thread_id: ThreadId::new(id),
        account_id: account_id.clone(),
        request_settings: ThreadRequestSettings::default(),
        started_at: UnixMillis::new(0),
    }
}

#[test]
fn unavailable_activity_does_not_invent_empty_rows_or_zero_counts() {
    let state = RotationUiState {
        activity: ActiveTaskProjection::from_activity_at(
            &TaskActivity::default(),
            UnixMillis::new(0),
        ),
        ..Default::default()
    };

    assert_eq!(state.active_task_rows(), None);
    assert_eq!(state.active_task_count(&AccountId::new("account")), None);
}

#[test]
fn rows_and_account_counts_share_one_exact_activity_projection() {
    let account = AccountId::new("account");
    let other = AccountId::new("other");
    let state = RotationUiState {
        activity: ActiveTaskProjection::Available(vec![
            row("parent", &account),
            row("child", &account),
            row("unrelated", &other),
        ]),
        ..Default::default()
    };

    assert_eq!(state.active_task_rows().unwrap().len(), 3);
    assert_eq!(state.active_task_count(&account), Some(2));
    assert_eq!(state.active_task_count(&other), Some(1));
    assert_eq!(state.active_task_count(&AccountId::new("missing")), Some(0));
}
