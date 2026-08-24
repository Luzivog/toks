/// Keep only fields used by the parser before SQLite returns the JSON payload.
/// OpenCode stores message content beside accounting metadata, and tool output
/// can dwarf the token fields needed here.
const COMPACT_MESSAGE_SQL: &str = r#"json_object(
    'id', json_extract(data, '$.id'),
    'role', json_extract(data, '$.role'),
    'modelID', json_extract(data, '$.modelID'),
    'providerID', json_extract(data, '$.providerID'),
    'model', json_extract(data, '$.model'),
    'cost', json_extract(data, '$.cost'),
    'tokens', json_extract(data, '$.tokens'),
    'time', json_extract(data, '$.time'),
    'agent', json_extract(data, '$.agent'),
    'mode', json_extract(data, '$.mode'),
    'path', json_extract(data, '$.path')
)"#;

fn project(source: &str) -> String {
    format!(
        r#"
        SELECT id, session_id, {COMPACT_MESSAGE_SQL}, workspace_root, session_title
        FROM ({source}) rows
        ORDER BY id, session_id
    "#
    )
}

pub(super) fn v2_queries() -> (String, String) {
    let modern = project(
        r#"
            SELECT sm.id, sm.session_id, sm.data AS data,
                   NULLIF(s.directory, '') AS workspace_root, s.title AS session_title
            FROM session_message sm
            LEFT JOIN session s ON s.id = sm.session_id
            WHERE sm.type = 'assistant'
              AND json_extract(sm.data, '$.tokens') IS NOT NULL
        "#,
    );
    let without_title = project(
        r#"
            SELECT sm.id, sm.session_id, sm.data AS data,
                   NULLIF(s.directory, '') AS workspace_root, NULL AS session_title
            FROM session_message sm
            LEFT JOIN session s ON s.id = sm.session_id
            WHERE sm.type = 'assistant'
              AND json_extract(sm.data, '$.tokens') IS NOT NULL
        "#,
    );
    (modern, without_title)
}

pub(super) fn v1_queries() -> (String, String, String) {
    let modern = project(
        r#"
            SELECT m.id, m.session_id, m.data AS data,
                   NULLIF(s.directory, '') AS workspace_root, s.title AS session_title
            FROM message m
            LEFT JOIN session s ON s.id = m.session_id
            WHERE json_extract(m.data, '$.role') = 'assistant'
              AND json_extract(m.data, '$.tokens') IS NOT NULL
        "#,
    );
    let without_title = project(
        r#"
            SELECT m.id, m.session_id, m.data AS data,
                   NULLIF(s.directory, '') AS workspace_root, NULL AS session_title
            FROM message m
            LEFT JOIN session s ON s.id = m.session_id
            WHERE json_extract(m.data, '$.role') = 'assistant'
              AND json_extract(m.data, '$.tokens') IS NOT NULL
        "#,
    );
    let legacy = project(
        r#"
            SELECT m.id, m.session_id, m.data AS data,
                   NULL AS workspace_root, NULL AS session_title
            FROM message m
            WHERE json_extract(m.data, '$.role') = 'assistant'
              AND json_extract(m.data, '$.tokens') IS NOT NULL
        "#,
    );
    (modern, without_title, legacy)
}

#[cfg(test)]
mod sqlite_projection_tests;
