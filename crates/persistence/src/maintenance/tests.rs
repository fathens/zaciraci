use super::*;

#[test]
fn validate_reindex_target_allows_pool_info() {
    assert!(validate_reindex_target("pool_info").is_ok());
}

#[test]
fn validate_reindex_target_allows_token_rates() {
    assert!(validate_reindex_target("token_rates").is_ok());
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
