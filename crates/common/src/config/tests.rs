use super::store::*;
use std::collections::HashMap;

use serial_test::serial;

#[test]
#[serial]
fn test_config_store_priority() {
    // CONFIG_STOREの値が最優先
    const TEST_KEY: &str = "TRADE_CRON_SCHEDULE";
    let _env_guard = EnvGuard::set(TEST_KEY, "env-value");
    let _config_guard = ConfigGuard::new(TEST_KEY, "store-value");
    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "store-value");
}

#[test]
#[serial]
fn test_priority_order() {
    // 優先順位の完全検証: CONFIG_STORE > DB > 環境変数 > Err
    const TEST_KEY: &str = "TEST_PRIORITY_ORDER_KEY";

    // Guard で全レイヤーの状態を保存。Drop 時に復元。
    let _db_guard = DbStoreGuard::new();
    let _env_guard = EnvGuard::remove(TEST_KEY);
    remove(TEST_KEY);

    // Step 1: 何もない場合は Err
    assert!(get(TEST_KEY).is_err());

    // Step 2: 環境変数追加
    unsafe {
        std::env::set_var(TEST_KEY, "99");
    }
    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "99");

    // Step 3: DB_STORE 追加 (環境変数より優先)
    load_db_config(HashMap::from([(TEST_KEY.to_string(), "77".to_string())]));
    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "77");

    // Step 4: CONFIG_STORE 追加 (DB_STORE より優先)
    let _config_guard = ConfigGuard::new(TEST_KEY, "42");
    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "42");
}

#[test]
#[serial]
fn test_trade_min_pool_liquidity_from_env() {
    let _env_guard = EnvGuard::set("TRADE_MIN_POOL_LIQUIDITY", "200");
    let result = get("TRADE_MIN_POOL_LIQUIDITY").unwrap();
    assert_eq!(result, "200");
}

#[test]
#[serial]
fn test_new_config_keys_config_store_override() {
    // CONFIG_STORE で上書きした場合に新規キーも優先されることを確認
    let _guard1 = ConfigGuard::new("PORTFOLIO_REBALANCE_THRESHOLD", "0.05");
    let result = get("PORTFOLIO_REBALANCE_THRESHOLD").unwrap();
    assert_eq!(result, "0.05");

    let _guard2 = ConfigGuard::new("HARVEST_BALANCE_MULTIPLIER", "256");
    let result = get("HARVEST_BALANCE_MULTIPLIER").unwrap();
    assert_eq!(result, "256");
}

#[test]
#[serial]
fn test_db_store_overrides_env() {
    // DB_STORE が環境変数より優先されること
    const TEST_KEY: &str = "TRADE_TOP_TOKENS";
    let _env_guard = EnvGuard::set(TEST_KEY, "env_val");
    let _db_guard = DbStoreGuard::new();
    remove(TEST_KEY);

    load_db_config(HashMap::from([(
        TEST_KEY.to_string(),
        "db_val".to_string(),
    )]));
    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "db_val");
}

#[test]
#[serial]
fn test_config_store_overrides_db_store() {
    // CONFIG_STORE が DB_STORE より優先されること
    const TEST_KEY: &str = "TRADE_TOP_TOKENS";
    let _db_guard = DbStoreGuard::new();
    load_db_config(HashMap::from([(
        TEST_KEY.to_string(),
        "db_val".to_string(),
    )]));
    let _config_guard = ConfigGuard::new(TEST_KEY, "store_val");

    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "store_val");
}

#[test]
#[serial]
fn test_load_db_config_replaces_previous() {
    // load_db_config を再度呼ぶと前の値が置き換えられること
    let _db_guard = DbStoreGuard::new();

    load_db_config(HashMap::from([
        ("KEY_A".to_string(), "val_a".to_string()),
        ("KEY_B".to_string(), "val_b".to_string()),
    ]));
    assert_eq!(get("KEY_A").unwrap(), "val_a");
    assert_eq!(get("KEY_B").unwrap(), "val_b");

    // 再ロード: KEY_A は更新、KEY_B は消える
    load_db_config(HashMap::from([(
        "KEY_A".to_string(),
        "new_val_a".to_string(),
    )]));
    assert_eq!(get("KEY_A").unwrap(), "new_val_a");
    assert!(get("KEY_B").is_err());
}

#[test]
#[serial]
fn test_db_store_empty_falls_through() {
    // DB_STORE が空の場合は環境変数にフォールスルー
    const TEST_KEY: &str = "TRADE_TOP_TOKENS";
    let _db_guard = DbStoreGuard::new();
    let _env_guard = EnvGuard::set(TEST_KEY, "env_val");
    remove(TEST_KEY);

    if let Ok(mut store) = super::store::DB_STORE.lock() {
        store.clear();
    }

    let result = get(TEST_KEY).unwrap();
    assert_eq!(result, "env_val");
}

// =========================================================================
// remove / ConfigGuard / DbStoreGuard / EnvGuard のユニットテスト
// =========================================================================

#[test]
#[serial]
fn test_remove_existing_key() {
    set("TEST_REMOVE_KEY", "value");
    assert_eq!(get_from_store("TEST_REMOVE_KEY"), Some("value".to_string()));

    remove("TEST_REMOVE_KEY");
    assert_eq!(get_from_store("TEST_REMOVE_KEY"), None);
}

#[test]
#[serial]
fn test_remove_nonexistent_key() {
    // 存在しないキーの remove はパニックしない
    remove("TEST_REMOVE_NONEXISTENT_KEY_12345");
}

#[test]
#[serial]
fn test_config_guard_restores_previous_value() {
    set("TEST_GUARD_KEY", "original");

    {
        let _guard = ConfigGuard::new("TEST_GUARD_KEY", "temporary");
        assert_eq!(
            get_from_store("TEST_GUARD_KEY"),
            Some("temporary".to_string())
        );
    }
    // guard が drop → 元の値に復元
    assert_eq!(
        get_from_store("TEST_GUARD_KEY"),
        Some("original".to_string())
    );

    // Cleanup
    remove("TEST_GUARD_KEY");
}

#[test]
#[serial]
fn test_config_guard_removes_when_no_previous() {
    // キーが存在しない状態で guard を作成
    remove("TEST_GUARD_NEW_KEY");
    assert_eq!(get_from_store("TEST_GUARD_NEW_KEY"), None);

    {
        let _guard = ConfigGuard::new("TEST_GUARD_NEW_KEY", "temporary");
        assert_eq!(
            get_from_store("TEST_GUARD_NEW_KEY"),
            Some("temporary".to_string())
        );
    }
    // guard が drop → キー自体が削除される
    assert_eq!(get_from_store("TEST_GUARD_NEW_KEY"), None);
}

#[test]
#[serial]
fn test_config_guard_nested() {
    remove("TEST_GUARD_NEST");

    {
        let _g1 = ConfigGuard::new("TEST_GUARD_NEST", "first");
        {
            let _g2 = ConfigGuard::new("TEST_GUARD_NEST", "second");
            assert_eq!(
                get_from_store("TEST_GUARD_NEST"),
                Some("second".to_string())
            );
        }
        // g2 drop → "first" に復元
        assert_eq!(get_from_store("TEST_GUARD_NEST"), Some("first".to_string()));
    }
    // g1 drop → キー削除
    assert_eq!(get_from_store("TEST_GUARD_NEST"), None);
}

#[test]
#[serial]
fn test_db_store_guard_restores_state() {
    // DB_STORE に初期データを入れる
    load_db_config(HashMap::from([
        ("DB_KEY_A".to_string(), "a".to_string()),
        ("DB_KEY_B".to_string(), "b".to_string()),
    ]));

    {
        let _guard = DbStoreGuard::new();
        // guard 作成後に DB_STORE を変更
        load_db_config(HashMap::from([("DB_KEY_C".to_string(), "c".to_string())]));
        assert_eq!(get_from_db_store("DB_KEY_C"), Some("c".to_string()));
        assert_eq!(get_from_db_store("DB_KEY_A"), None);
    }
    // guard drop → 元の状態に復元
    assert_eq!(get_from_db_store("DB_KEY_A"), Some("a".to_string()));
    assert_eq!(get_from_db_store("DB_KEY_B"), Some("b".to_string()));
    assert_eq!(get_from_db_store("DB_KEY_C"), None);

    // Cleanup
    if let Ok(mut store) = super::store::DB_STORE.lock() {
        store.clear();
    }
}

#[test]
#[serial]
fn test_db_store_guard_restores_empty() {
    // DB_STORE が空の状態から開始
    if let Ok(mut store) = super::store::DB_STORE.lock() {
        store.clear();
    }

    {
        let _guard = DbStoreGuard::new();
        load_db_config(HashMap::from([("DB_TEMP".to_string(), "temp".to_string())]));
        assert_eq!(get_from_db_store("DB_TEMP"), Some("temp".to_string()));
    }
    // guard drop → 空に復元
    assert_eq!(get_from_db_store("DB_TEMP"), None);
}

#[test]
#[serial]
fn test_env_guard_set_restores_previous() {
    const KEY: &str = "TEST_ENV_GUARD_SET";
    unsafe {
        std::env::set_var(KEY, "original");
    }

    {
        let _guard = EnvGuard::set(KEY, "temporary");
        assert_eq!(std::env::var(KEY).unwrap(), "temporary");
    }
    // guard drop → 元の値に復元
    assert_eq!(std::env::var(KEY).unwrap(), "original");

    // Cleanup
    unsafe {
        std::env::remove_var(KEY);
    }
}

#[test]
#[serial]
fn test_env_guard_set_removes_when_no_previous() {
    const KEY: &str = "TEST_ENV_GUARD_NEW";
    unsafe {
        std::env::remove_var(KEY);
    }

    {
        let _guard = EnvGuard::set(KEY, "temporary");
        assert_eq!(std::env::var(KEY).unwrap(), "temporary");
    }
    // guard drop → 環境変数が削除される
    assert!(std::env::var(KEY).is_err());
}

#[test]
#[serial]
fn test_env_guard_remove_restores_previous() {
    const KEY: &str = "TEST_ENV_GUARD_REMOVE";
    unsafe {
        std::env::set_var(KEY, "original");
    }

    {
        let _guard = EnvGuard::remove(KEY);
        assert!(std::env::var(KEY).is_err());
    }
    // guard drop → 元の値に復元
    assert_eq!(std::env::var(KEY).unwrap(), "original");

    // Cleanup
    unsafe {
        std::env::remove_var(KEY);
    }
}

#[test]
#[serial]
fn test_env_guard_remove_noop_when_no_previous() {
    const KEY: &str = "TEST_ENV_GUARD_REMOVE_NOOP";
    unsafe {
        std::env::remove_var(KEY);
    }

    {
        let _guard = EnvGuard::remove(KEY);
        assert!(std::env::var(KEY).is_err());
    }
    // guard drop → 何もしない（元から無かった）
    assert!(std::env::var(KEY).is_err());
}

// =========================================================================
// get_excluding_db のユニットテスト
// =========================================================================

#[test]
#[serial]
fn test_get_excluding_db_returns_err_when_nothing_set() {
    const KEY: &str = "TEST_EXCL_DB_NOTHING";
    let _env = EnvGuard::remove(KEY);
    remove(KEY);
    assert!(get_excluding_db(KEY).is_err());
}

#[test]
#[serial]
fn test_get_excluding_db_returns_env_var() {
    const KEY: &str = "TEST_EXCL_DB_ENV";
    let _env = EnvGuard::set(KEY, "from_env");
    remove(KEY);
    assert_eq!(get_excluding_db(KEY).unwrap(), "from_env");
}

#[test]
#[serial]
fn test_get_excluding_db_config_store_overrides_env() {
    const KEY: &str = "TEST_EXCL_DB_STORE";
    let _env = EnvGuard::set(KEY, "from_env");
    let _config = ConfigGuard::new(KEY, "from_store");
    assert_eq!(get_excluding_db(KEY).unwrap(), "from_store");
}

#[test]
#[serial]
fn test_get_excluding_db_skips_db_store() {
    const KEY: &str = "TEST_EXCL_DB_SKIP";
    let _db_guard = DbStoreGuard::new();
    let _env = EnvGuard::remove(KEY);
    remove(KEY);

    load_db_config(HashMap::from([(KEY.to_string(), "from_db".to_string())]));

    // get() は DB_STORE の値を返す
    assert_eq!(get(KEY).unwrap(), "from_db");
    // get_excluding_db() は DB_STORE をスキップするので Err
    assert!(get_excluding_db(KEY).is_err());
}

#[test]
#[serial]
fn test_get_excluding_db_skips_db_falls_through_to_env() {
    const KEY: &str = "TEST_EXCL_DB_FALLTHROUGH";
    let _db_guard = DbStoreGuard::new();
    let _env = EnvGuard::set(KEY, "from_env");
    remove(KEY);

    load_db_config(HashMap::from([(KEY.to_string(), "from_db".to_string())]));

    // get() は DB_STORE の値（優先度が高い）を返す
    assert_eq!(get(KEY).unwrap(), "from_db");
    // get_excluding_db() は DB_STORE をスキップして env を返す
    assert_eq!(get_excluding_db(KEY).unwrap(), "from_env");
}
