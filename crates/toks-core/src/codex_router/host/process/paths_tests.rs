use super::paths::installed_build_id;

#[test]
fn baked_identity_does_not_depend_on_the_process_environment() {
    let from_installer = installed_build_id(Ok("baked-at-install".into())).unwrap();
    let from_manager = installed_build_id(Ok("baked-at-install".into())).unwrap();

    assert_eq!(from_installer, from_manager);
    assert_eq!(from_manager.as_str(), "baked-at-install");
}

#[test]
fn missing_or_blank_baked_identity_is_rejected() {
    assert!(installed_build_id(Err(std::env::VarError::NotPresent)).is_err());
    assert!(installed_build_id(Ok("  ".into())).is_err());
}
