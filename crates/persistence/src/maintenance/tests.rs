use super::*;

#[test]
fn validate_reindex_target_allows_all_tables() {
    for table in REINDEX_TARGETS {
        assert!(
            validate_reindex_target(table).is_ok(),
            "expected '{}' to be allowed",
            table
        );
    }
}

#[test]
fn validate_reindex_target_rejects_unknown_table() {
    let err = validate_reindex_target("malicious_table").unwrap_err();
    assert!(
        err.to_string()
            .contains("not in the allowed reindex targets"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn validate_reindex_target_rejects_sql_injection_attempt() {
    let err = validate_reindex_target("pool_info; DROP TABLE pool_info").unwrap_err();
    assert!(
        err.to_string()
            .contains("not in the allowed reindex targets"),
        "SQL injection attempt should be rejected: {}",
        err
    );
}

#[test]
fn default_cron_schedule_is_valid() {
    let parsed: std::result::Result<cron::Schedule, _> = DEFAULT_CRON_SCHEDULE.parse();
    assert!(
        parsed.is_ok(),
        "DEFAULT_CRON_SCHEDULE should be a valid cron expression: {:?}",
        parsed.err()
    );
}
