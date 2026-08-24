use super::*;

#[test]
fn unset_and_blank_names_stay_unpinned() {
    assert_eq!(
        BucketTimezone::from_pinned_name(None),
        BucketTimezone::Local
    );
    assert_eq!(
        BucketTimezone::from_pinned_name(Some("")),
        BucketTimezone::Local
    );
    assert_eq!(
        BucketTimezone::from_pinned_name(Some("   ")),
        BucketTimezone::Local
    );
    assert!(!BucketTimezone::from_pinned_name(None).is_pinned());
}

#[test]
fn unknown_zone_name_degrades_to_local_instead_of_failing() {
    assert_eq!(
        BucketTimezone::from_pinned_name(Some("Mars/Olympus_Mons")),
        BucketTimezone::Local
    );
    // A fixed-offset string is not an IANA name and must not be accepted:
    // silently honoring it would pin a zone that cannot follow DST.
    assert_eq!(
        BucketTimezone::from_pinned_name(Some("+09:00")),
        BucketTimezone::Local
    );
}

#[test]
fn pinned_zone_keys_the_same_instant_the_same_way_regardless_of_host() {
    let tz = BucketTimezone::from_pinned_name(Some("Asia/Seoul")).clone();
    assert_eq!(tz.pinned_name(), Some("Asia/Seoul"));
    assert!(tz.is_pinned());

    // 2026-03-02T18:00:00Z — 2026-03-03 03:00 in Seoul, 2026-03-02 10:00 in
    // Los Angeles. The day key follows the pinned zone, not the host.
    let instant = 1_772_474_400_000;
    assert_eq!(tz.day_key(instant), "2026-03-03");
    assert_eq!(
        BucketTimezone::from_pinned_name(Some("America/Los_Angeles")).day_key(instant),
        "2026-03-02"
    );
}

/// The reason this module does not use `FixedOffset`. A zone that observes
/// DST changes its offset mid-year; an offset pinned before the transition
/// keys instants after it onto the wrong day near midnight.
#[test]
fn named_zone_follows_dst_where_a_fixed_offset_would_not() {
    let ny = BucketTimezone::from_pinned_name(Some("America/New_York"));

    // 2026-01-15T04:30:00Z — 23:30 on the 14th in EST (UTC-5).
    let winter = chrono::DateTime::parse_from_rfc3339("2026-01-15T04:30:00Z")
        .unwrap()
        .timestamp_millis();
    // 2026-07-15T03:30:00Z — 23:30 on the 14th in EDT (UTC-4).
    let summer = chrono::DateTime::parse_from_rfc3339("2026-07-15T03:30:00Z")
        .unwrap()
        .timestamp_millis();

    assert_eq!(ny.day_key(winter), "2026-01-14");
    assert_eq!(ny.day_key(summer), "2026-07-14");

    // The same instants under the winter offset frozen as a fixed value:
    // the summer one lands a day late.
    let frozen = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
    assert_eq!(format_day_key(winter, &frozen), "2026-01-14");
    assert_eq!(
        format_day_key(summer, &frozen),
        "2026-07-14",
        "sanity: 03:30Z is 22:30 EST, still the 14th"
    );

    // And where it actually bites: 00:30 EDT on the 15th is 23:30 EST on
    // the 14th under a frozen winter offset.
    let after_midnight_edt = chrono::DateTime::parse_from_rfc3339("2026-07-15T04:30:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(ny.day_key(after_midnight_edt), "2026-07-15");
    assert_eq!(
        format_day_key(after_midnight_edt, &frozen),
        "2026-07-14",
        "a frozen offset buckets an hour of every DST-shifted day onto the wrong date"
    );
}

/// Zones that produce the same offset at every instant produce the same day
/// keys, so they are interchangeable and pinning either is a no-op.
#[test]
fn observationally_identical_zones_agree() {
    let utc: chrono_tz::Tz = "Etc/UTC".parse().unwrap();
    assert!(zones_agree(&utc, &chrono::Utc));
    assert!(zones_agree(&utc, &"UTC".parse::<chrono_tz::Tz>().unwrap()));

    // Same rules, different names — a device may legitimately be detected
    // as either and neither moves a day boundary. These are tz database
    // *links*, so they are the same rules by construction rather than by
    // two zones happening to have matched recently.
    let new_york: chrono_tz::Tz = "America/New_York".parse().unwrap();
    let us_eastern: chrono_tz::Tz = "US/Eastern".parse().unwrap();
    assert!(zones_agree(&new_york, &us_eastern));

    let seoul: chrono_tz::Tz = "Asia/Seoul".parse().unwrap();
    let rok: chrono_tz::Tz = "ROK".parse().unwrap();
    assert!(zones_agree(&seoul, &rok));
}

/// The guard that keeps the first run from re-keying history.
#[test]
fn zones_with_different_offsets_are_rejected() {
    let seoul: chrono_tz::Tz = "Asia/Seoul".parse().unwrap();
    let utc: chrono_tz::Tz = "Etc/UTC".parse().unwrap();
    assert!(
        !zones_agree(&seoul, &utc),
        "a nine-hour difference must never be accepted as the same zone"
    );

    // The subtle case: identical offset for part of the year, different DST
    // rules. Sampling a single instant would let this through in winter.
    let london: chrono_tz::Tz = "Europe/London".parse().unwrap();
    assert!(
        !zones_agree(&london, &utc),
        "matching offsets in winter must not pass for a zone that observes DST"
    );

    // And against a fixed offset, which is what a `TZ=<+09>-9` host looks
    // like to `chrono::Local`: same offset now, no transitions ever.
    // Tokyo, not Seoul — Seoul observed DST in 1987-88, and the window now
    // reaches back far enough to see it.
    let tokyo: chrono_tz::Tz = "Asia/Tokyo".parse().unwrap();
    let plus_nine = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    assert!(
        zones_agree(&tokyo, &plus_nine),
        "Asia/Tokyo has had no DST since the epoch"
    );
    assert!(
        !zones_agree(&seoul, &plus_nine),
        "Asia/Seoul's 1987-88 DST is inside the window and must be seen"
    );
    assert!(
        !zones_agree(&london, &chrono::FixedOffset::east_opt(0).unwrap()),
        "a fixed offset cannot stand in for a zone that observes DST"
    );

    // The case that sets the sampling step. Lord Howe shifts by 30 minutes
    // where Sydney shifts by an hour, so for part of each DST season they
    // differ by only half an hour. A step coarser than that would step over
    // the divergence and accept two zones that bucket differently.
    //
    // Asserted over a recent window as well as the full one: across all of
    // history these two diverge in bigger ways too, and the point here is
    // that the *current* half-hour difference is still caught.
    let lord_howe: chrono_tz::Tz = "Australia/Lord_Howe".parse().unwrap();
    let sydney: chrono_tz::Tz = "Australia/Sydney".parse().unwrap();
    assert!(
        !zones_agree(&lord_howe, &sydney),
        "a half-hour DST difference must be detected"
    );
    let now = chrono::Utc::now().timestamp_millis();
    assert!(
        !zones_agree_between(&lord_howe, &sydney, now - 2 * YEAR_MS, now),
        "and detected from recent samples alone, not only from old history"
    );
}

/// The window has to cover everything an accepted pin can re-key, not a
/// recent slice of it.
///
/// `rebucket_days` applies the pin to *every* message, so a zone that
/// matches `chrono::Local` across the last decade but diverges in older
/// rules would still move day boundaries in older history. Seoul and Tokyo
/// are exactly that pair: both fixed at UTC+09:00 for decades, but Seoul
/// observed DST in 1987 and 1988 and Tokyo did not.
#[test]
fn zones_that_only_diverge_before_the_last_decade_are_still_rejected() {
    let seoul: chrono_tz::Tz = "Asia/Seoul".parse().unwrap();
    let tokyo: chrono_tz::Tz = "Asia/Tokyo".parse().unwrap();

    let now = chrono::Utc::now().timestamp_millis();

    // The premise: indistinguishable across the window this check used to
    // sample. If this ever stops holding the test below proves nothing.
    assert!(
        zones_agree_between(&seoul, &tokyo, now - 10 * YEAR_MS, now + YEAR_MS),
        "Seoul and Tokyo must be indistinguishable over the last decade for \
         this test to be about the window rather than the zones"
    );

    assert!(
        !zones_agree(&seoul, &tokyo),
        "a divergence older than the previous window must still be caught — \
         Seoul's 1987-88 DST moves day boundaries the pin would silently rewrite"
    );
}

#[test]
fn tz_env_is_read_as_a_zone_name_only_when_it_is_one() {
    // Not asserting on the live environment — exercising the parse rule the
    // TZ path uses, including the POSIX leading colon.
    assert_eq!(
        "Asia/Seoul".parse::<chrono_tz::Tz>().ok(),
        ":Asia/Seoul"
            .strip_prefix(':')
            .unwrap()
            .parse::<chrono_tz::Tz>()
            .ok()
    );
    // POSIX rule strings are honored by `chrono::Local` but are not names
    // that can be pinned, so they must fall through to the detector.
    assert!("<+09>-9".parse::<chrono_tz::Tz>().is_err());
    assert!("/etc/localtime".parse::<chrono_tz::Tz>().is_err());
}

/// A `TZ` the machine is not in must not make the device unpinnable.
///
/// Windows-only because it is the only platform where the two disagree:
/// `chrono::Local` reads the Win32 zone and never the environment, so
/// offering `TZ` as the candidate would fail [`zones_agree`] on every run
/// and leave the device bucketing by `chrono::Local` forever — carrying the
/// exact bug pinning removes, and saying nothing, because declining is the
/// safe branch. Mutating `TZ` here is harmless for the same reason nothing
/// on this platform reads it.
#[test]
#[cfg(not(unix))]
fn a_foreign_tz_does_not_make_a_windows_host_unpinnable() {
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TZ"]);
    env.set("TZ", "Asia/Seoul");

    assert!(
        tz_env_zone().is_none(),
        "TZ must not be offered as the pin candidate where chrono::Local \
         does not read it"
    );

    let with_foreign_tz = detect_local_iana_name();
    env.remove("TZ");
    let without_tz = detect_local_iana_name();
    assert_eq!(
        with_foreign_tz, without_tz,
        "detection must reach the same answer with and without TZ set"
    );
}

#[test]
fn detection_either_names_a_real_zone_or_declines() {
    // Host-dependent, so assert the contract rather than a value: whatever
    // comes back must round-trip through the tz database, because a name
    // that does not would pin to something later scans silently ignore.
    if let Some(name) = detect_local_iana_name() {
        assert!(
            BucketTimezone::from_pinned_name(Some(&name)).is_pinned(),
            "detected zone {name} must be re-resolvable"
        );
        // The contract that makes auto-pinning safe: whatever comes back
        // buckets identically to what the parsers already used.
        let pinned: chrono_tz::Tz = name.parse().unwrap();
        assert!(
            zones_agree(&pinned, &chrono::Local),
            "detected zone {name} must reproduce chrono::Local"
        );
    }
}
