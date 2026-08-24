use std::sync::Arc;

use super::{Catalogue, Credentials, Engine, SharedCredentials};
use crate::accounts::AccountId;
use crate::rotation::{RotationRuntimeStore, RotationSettings, RotationSettingsStore, ThreadId};

#[test]
fn only_known_root_or_unknown_threads_enter_the_external_resume_queue() {
    let directory = tempfile::tempdir().expect("temp dir");
    let account = AccountId::new("a");
    let settings_store = RotationSettingsStore::for_data_dir(directory.path());
    let mut settings = RotationSettings::default();
    settings.reconcile(std::slice::from_ref(&account));
    settings.set_enabled(true);
    settings_store.save(&settings).unwrap();
    let runtime_store = RotationRuntimeStore::for_data_dir(directory.path());
    let database = directory.path().join("state.sqlite");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, thread_source TEXT)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO threads (id, thread_source) VALUES \
                ('child', 'subagent'), ('root', 'cli')",
            [],
        )
        .unwrap();
    drop(connection);
    let credentials: SharedCredentials = Arc::new(Credentials {
        accounts: vec![account],
    });
    let engine = Engine::with_catalogue_and_thread_sources(
        credentials,
        settings_store,
        runtime_store.clone(),
        Catalogue::at(None),
        crate::codex_router::thread_source::ThreadSourceStore::for_database(database),
    )
    .unwrap();

    engine.waiting(&ThreadId::new("child")).unwrap();
    engine.waiting(&ThreadId::new("root")).unwrap();
    engine.waiting(&ThreadId::new("unknown")).unwrap();

    let runtime = runtime_store.load().unwrap();
    let waiting = runtime
        .waiting_threads()
        .iter()
        .map(|waiting| waiting.thread_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(waiting, ["root", "unknown"]);
}
