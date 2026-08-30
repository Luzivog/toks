use super::status_from_observations;

#[test]
fn routing_and_automatic_recovery_report_independent_health() {
    let healthy = status_from_observations(true, true, true, true, true);
    assert!(healthy.service_active);
    assert!(healthy.resume_active);

    let recovery_down = status_from_observations(true, true, true, true, false);
    assert!(recovery_down.service_active);
    assert!(!recovery_down.resume_active);

    assert!(!status_from_observations(true, true, false, true, true).service_active);
    assert!(!status_from_observations(true, true, true, false, true).service_active);
}
