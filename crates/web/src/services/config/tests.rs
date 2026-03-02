use super::*;
use crate::proto::config_service_server::ConfigService;
use serial_test::serial;
use tonic::Request;

const TEST_INSTANCE: &str = "web-test";
const TEST_KEY_PREFIX: &str = "WEB_TEST_";

fn test_key(suffix: &str) -> String {
    format!("{TEST_KEY_PREFIX}{suffix}")
}

async fn cleanup(key: &str) {
    let _ = persistence::config_store::delete(TEST_INSTANCE, key).await;
    let _ = persistence::config_store::delete("*", key).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_and_get_one() {
    let svc = ConfigServiceImpl;
    let key = test_key("UPSERT_GET");

    // Upsert
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
        value: "test_value".to_string(),
        description: Some("test description".to_string()),
    }))
    .await
    .unwrap();

    // GetOne
    let response = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: key.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(response.into_inner().value, Some("test_value".to_string()));

    cleanup(&key).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_overwrites_existing() {
    let svc = ConfigServiceImpl;
    let key = test_key("OVERWRITE");

    // Insert initial value
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
        value: "initial".to_string(),
        description: None,
    }))
    .await
    .unwrap();

    // Overwrite with new value
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
        value: "updated".to_string(),
        description: None,
    }))
    .await
    .unwrap();

    // Verify updated
    let response = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: key.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(response.into_inner().value, Some("updated".to_string()));

    cleanup(&key).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_and_get_all() {
    let svc = ConfigServiceImpl;
    let key = test_key("GET_ALL");

    // Upsert
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
        value: "all_value".to_string(),
        description: None,
    }))
    .await
    .unwrap();

    // GetAll
    let response = svc
        .get_all(Request::new(GetAllConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
        }))
        .await
        .unwrap();
    let entries = response.into_inner().entries;
    let found = entries.iter().find(|e| e.key == key);
    assert!(found.is_some(), "Expected key {key} in entries");
    assert_eq!(found.unwrap().value, "all_value");

    cleanup(&key).await;
}

#[tokio::test]
#[serial]
async fn test_delete() {
    let svc = ConfigServiceImpl;
    let key = test_key("DELETE");

    // Upsert then delete
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
        value: "to_delete".to_string(),
        description: None,
    }))
    .await
    .unwrap();

    svc.delete(Request::new(DeleteConfigRequest {
        instance_id: TEST_INSTANCE.to_string(),
        key: key.clone(),
    }))
    .await
    .unwrap();

    // Verify deleted
    let response = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: key.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(response.into_inner().value, None);
}

#[tokio::test]
#[serial]
async fn test_delete_nonexistent_succeeds() {
    let svc = ConfigServiceImpl;

    let result = svc
        .delete(Request::new(DeleteConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: test_key("NEVER_EXISTED"),
        }))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
#[serial]
async fn test_get_one_not_found() {
    let svc = ConfigServiceImpl;

    let response = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: test_key("NONEXISTENT"),
        }))
        .await
        .unwrap();
    assert_eq!(response.into_inner().value, None);
}

#[tokio::test]
#[serial]
async fn test_empty_key_rejected() {
    let svc = ConfigServiceImpl;

    let result = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: String::new(),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    let result = svc
        .upsert(Request::new(UpsertConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: String::new(),
            value: "v".to_string(),
            description: None,
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    let result = svc
        .delete(Request::new(DeleteConfigRequest {
            instance_id: TEST_INSTANCE.to_string(),
            key: String::new(),
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
#[serial]
async fn test_empty_instance_id_defaults_to_global() {
    let svc = ConfigServiceImpl;
    let key = test_key("GLOBAL_DEFAULT");

    // Upsert with empty instance_id (should default to "*")
    svc.upsert(Request::new(UpsertConfigRequest {
        instance_id: String::new(),
        key: key.clone(),
        value: "global_value".to_string(),
        description: None,
    }))
    .await
    .unwrap();

    // GetOne with explicit "*" should find it
    let response = svc
        .get_one(Request::new(GetOneConfigRequest {
            instance_id: "*".to_string(),
            key: key.clone(),
        }))
        .await
        .unwrap();
    assert_eq!(
        response.into_inner().value,
        Some("global_value".to_string())
    );

    cleanup(&key).await;
}
