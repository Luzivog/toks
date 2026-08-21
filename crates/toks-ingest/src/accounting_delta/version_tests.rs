use rusqlite::Connection;

use super::tests::{options, setup, write_initial};

#[test]
fn codex_v7_json_checkpoint_is_wire_compatible_without_a_reparse() {
    let (home, state, mut collector) = setup();
    write_initial(home.path());
    collector
        .advance_for_test(options(home.path()), None)
        .unwrap();
    drop(collector);

    let database = state.path().join("accounting-checkpoints-v2.sqlite");
    let connection = Connection::open(database).unwrap();
    connection
        .execute("UPDATE sources SET parser_version = 7", [])
        .unwrap();
    drop(connection);

    let mut reopened = super::AccountingDeltaCollector::open_at(state.path()).unwrap();
    let unchanged = reopened
        .advance_for_test(options(home.path()), None)
        .unwrap();
    assert!(unchanged.sources.is_empty());
    assert_eq!(unchanged.backlog.changed_sources, 0);
}
