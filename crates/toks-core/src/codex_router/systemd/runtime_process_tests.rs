use super::runtime_process::exact_command;
use std::collections::BTreeMap;

#[test]
fn exact_command_removes_the_real_ambient_environment() {
    let expected = BTreeMap::from([
        ("PATH".into(), "/usr/bin$literal%value".into()),
        ("TOKS_CODEX_BIN".into(), "/opt/codex$literal%value".into()),
    ]);
    let output = exact_command(
        std::path::Path::new("/usr/bin/env"),
        [] as [&str; 0],
        &expected,
    )
    .output()
    .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let actual = output
        .lines()
        .map(|line| line.split_once('=').unwrap())
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual.len(), 2);
    assert_eq!(actual["PATH"], "/usr/bin$literal%value");
    assert_eq!(actual["TOKS_CODEX_BIN"], "/opt/codex$literal%value");
    assert!(!actual.contains_key("OPENAI_API_KEY"));
    assert!(!actual.contains_key("LD_PRELOAD"));
}
