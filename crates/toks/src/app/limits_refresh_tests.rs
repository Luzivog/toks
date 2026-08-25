use std::time::Duration;

use super::limits_refresh::{channel, wait};

#[test]
fn immediate_limit_refresh_requests_coalesce_for_the_single_collector() {
    let (signal, requests) = channel();

    signal.request();
    signal.request();

    assert_eq!(requests.len(), 1);
    smol::block_on(wait(&requests, Duration::from_secs(60)));
    assert!(requests.is_empty());
}
