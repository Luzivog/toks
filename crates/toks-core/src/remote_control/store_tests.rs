use super::store::{environment_at, remember_at};
use crate::accounts::AccountId;

#[test]
fn environments_survive_restart_and_stay_scoped_to_the_control_account() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("remote-control.json");
    let first = AccountId::new("first");
    let second = AccountId::new("second");
    remember_at(&path, &first, "environment-a").unwrap();
    remember_at(&path, &second, "environment-b").unwrap();
    assert_eq!(
        environment_at(&path, &first).unwrap().as_deref(),
        Some("environment-a")
    );
    assert_eq!(
        environment_at(&path, &second).unwrap().as_deref(),
        Some("environment-b")
    );
}
