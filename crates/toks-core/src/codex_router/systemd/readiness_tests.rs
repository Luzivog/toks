use super::readiness::iteration;

#[test]
fn every_iteration_checks_the_socket_and_ready_state_checks_it_again() {
    let mut calls = 0;
    for _ in 0..2 {
        assert!(!iteration(
            || {
                calls += 1;
                Ok(())
            },
            || Ok(false),
        )
        .unwrap());
    }
    assert_eq!(calls, 2);

    let error = iteration(
        || {
            calls += 1;
            (calls < 4)
                .then_some(())
                .ok_or_else(|| anyhow::anyhow!("socket deactivated"))
        },
        || Ok(true),
    )
    .unwrap_err();
    assert!(error.to_string().contains("deactivated"));
    assert_eq!(calls, 4);
}
