use super::{support::*, *};
/// An explicit `--home` outranks every environment lookup. Pinned so the
/// reordering that routed the fallback through `paths::home_dir` cannot
/// quietly promote the resolver above the caller's own argument.
#[test]
#[serial]
fn get_home_dir_string_prefers_the_explicit_option() {
    let mut env = home_guard();
    env.set("HOME", "/tmp/tokscope-env-home");
    assert_eq!(
        get_home_dir_string(&Some("/tmp/tokscope-explicit-home".to_string())),
        Ok("/tmp/tokscope-explicit-home".to_string())
    );
}

/// The bypass this test exists for: reading `$HOME` directly meant an
/// exported-but-blank value won outright and produced `Ok("")`. Every
/// consumer builds scan roots with `format!("{home}/...")`, so an empty
/// home turns each of them into an absolute path from the filesystem root
/// — `/.codex/sessions` rather than `~/.codex/sessions`.
///
/// `paths::home_dir` delegates to `dirs`, which treats a blank `HOME` as
/// unset and falls back to the passwd entry, so the empty string can no
/// longer escape. Asserting "not `Ok("")`" rather than a concrete path
/// keeps this honest on a runner with no passwd home, where the correct
/// answer is the `Err` arm.
#[test]
#[serial]
fn get_home_dir_string_never_returns_an_empty_home() {
    let mut env = home_guard();
    env.set("HOME", "");
    let resolved = get_home_dir_string(&None);
    assert_ne!(
        resolved,
        Ok(String::new()),
        "a blank HOME must not resolve to the empty string; \
             every caller joins it into a scan root"
    );
}

/// MSYS2, Cygwin and Git Bash export `HOME=/home/<user>` on Windows.
/// Returning that verbatim points the model, monthly, hourly and local
/// parsers at `C:\home\<user>` — `Path` reads the leading `/` as the root
/// of the current drive — so a Git Bash user sees none of their own usage.
/// `paths::home_dir` rejects the shape; this test pins that
/// `get_home_dir_string` actually goes through it rather than around it.
///
/// Windows-only by construction: `/home/runner` is a legitimate absolute
/// path on macOS and the resolver rightly honors it there. It does run —
/// on the `windows-latest` leg this PR adds.
#[test]
#[serial]
#[cfg(windows)]
fn get_home_dir_string_ignores_a_posix_shaped_home_on_windows() {
    let mut env = home_guard();
    env.set("HOME", "/home/runner");
    let resolved = get_home_dir_string(&None);
    assert_ne!(
        resolved,
        Ok("/home/runner".to_string()),
        "a POSIX-shaped HOME must not reach the scanners on Windows"
    );
}

#[test]
fn test_select_local_parse_pricing_prefers_fresh_service_for_new_models() {
    let mut fresh_litellm = HashMap::new();
    fresh_litellm.insert(
        "gpt-5.4".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.000002),
            output_cost_per_token: Some(0.00001),
            ..Default::default()
        },
    );
    let fresh = Arc::new(pricing::PricingService::new(fresh_litellm, HashMap::new()));
    let stale = pricing::PricingService::new(HashMap::new(), HashMap::new());
    let selected = select_local_parse_pricing(Ok(Arc::clone(&fresh)), || Some(stale)).unwrap();

    let mut msg = UnifiedMessage::new(
        "opencode",
        "gpt-5.4",
        "openai",
        "session-1",
        1_733_011_200_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(selected.as_ref()));

    assert!(msg.cost > 0.0);
}

#[test]
fn test_select_local_parse_pricing_falls_back_to_stale_cache_on_fetch_error() {
    let mut stale_litellm = HashMap::new();
    stale_litellm.insert(
        "gpt-5.2".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.00000175),
            output_cost_per_token: Some(0.000014),
            ..Default::default()
        },
    );
    let stale = pricing::PricingService::new(stale_litellm, HashMap::new());

    let selected =
        select_local_parse_pricing(Err("network failed".to_string()), || Some(stale)).unwrap();

    assert!(selected.lookup_with_source("gpt-5.2", None).is_some());
}

#[test]
fn test_select_local_parse_pricing_does_not_evaluate_stale_fallback_on_fresh_success() {
    let fresh = Arc::new(pricing::PricingService::new(HashMap::new(), HashMap::new()));
    let mut stale_called = false;

    let selected = select_local_parse_pricing(Ok(Arc::clone(&fresh)), || {
        stale_called = true;
        None
    })
    .unwrap();

    assert!(Arc::ptr_eq(&selected, &fresh));
    assert!(!stale_called);
}
