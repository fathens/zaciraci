use super::*;

#[test]
fn json_round_trip_keeps_every_field() {
    let mut s = CollectorState::new(1_792_843_229_499_015_627);
    s.push_gap(Gap {
        cursor: Some("abc".to_owned()),
        until_ns: 10,
        oldest_ns: Some(500),
    })
    .unwrap();
    s.cooldown_until = Some(1_792_900_000);
    s.last_cycle_at = Some(
        chrono::DateTime::from_timestamp(1_792_900_000, 0)
            .unwrap()
            .naive_utc(),
    );
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["schema_version"], SCHEMA_VERSION);
    assert_eq!(json["since_ns"], 1_792_843_229_499_015_627u64);
    let back: CollectorState = serde_json::from_value(json).unwrap();
    assert_eq!(back, s);
    assert!(back.validate().is_ok());
}

#[test]
fn unknown_schema_version_and_overlong_gap_list_fail_validation() {
    let mut s = CollectorState::new(0);
    s.schema_version = SCHEMA_VERSION + 1;
    assert!(s.validate().is_err());

    let mut s = CollectorState::new(0);
    for i in 0..MAX_GAPS {
        s.push_gap(Gap {
            cursor: None,
            until_ns: i as u64,
            oldest_ns: None,
        })
        .unwrap();
    }
    assert!(
        s.push_gap(Gap {
            cursor: None,
            until_ns: 0,
            oldest_ns: None
        })
        .is_err()
    );
    s.gaps.push(Gap {
        cursor: None,
        until_ns: 0,
        oldest_ns: None,
    });
    assert!(s.validate().is_err());
}

#[test]
fn gap_span_is_zero_without_an_edge_and_never_negative() {
    let g = Gap {
        cursor: None,
        until_ns: 100,
        oldest_ns: None,
    };
    assert_eq!(g.span_ns(), 0);
    let g = Gap {
        cursor: None,
        until_ns: 100,
        oldest_ns: Some(250),
    };
    assert_eq!(g.span_ns(), 150);
    let g = Gap {
        cursor: None,
        until_ns: 300,
        oldest_ns: Some(250),
    };
    assert_eq!(g.span_ns(), 0);
}

#[test]
fn missing_optional_fields_deserialize_with_defaults_only_where_declared() {
    // Every field is required except the Option ones, which accept null.
    let json = serde_json::json!({
        "schema_version": SCHEMA_VERSION, "since_ns": 5, "gaps": [],
        "cooldown_until": null, "last_cycle_at": null, "last_success_at": null
    });
    let s: CollectorState = serde_json::from_value(json).unwrap();
    assert_eq!(s, CollectorState::new(5));
    assert!(serde_json::from_value::<CollectorState>(serde_json::json!({"since_ns": 5})).is_err());
}
