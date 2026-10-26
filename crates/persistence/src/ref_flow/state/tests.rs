use super::*;
use crate::ref_flow::test_support::*;
use serde::Deserialize;
use serial_test::serial;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Demo {
    since_ns: u64,
    gaps: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Incompatible {
    #[allow(dead_code)]
    cursor: String,
}

fn demo(n: u64) -> Demo {
    Demo {
        since_ns: n,
        gaps: vec![format!("g{n}")],
    }
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn initialize_once_then_load() -> Result<()> {
    clean_tables().await?;
    assert!(load::<Demo>().await?.is_none());

    assert_eq!(
        initialize("fly-1", &demo(1), ts(10)).await?,
        InitOutcome::Created
    );
    assert_eq!(
        initialize("fly-2", &demo(2), ts(20)).await?,
        InitOutcome::AlreadyInitialized
    );

    let loaded = load::<Demo>().await?.unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.writer_id, "fly-1");
    assert_eq!(loaded.state, demo(1));
    assert_eq!(loaded.updated_at, ts(10));
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn save_requires_the_loaded_version() -> Result<()> {
    clean_tables().await?;
    initialize("a", &demo(1), ts(0)).await?;

    assert_eq!(
        save(1, "a", &demo(2), ts(1)).await?,
        SaveOutcome::Saved { version: 2 }
    );
    // Stale writer still holding version 1.
    assert_eq!(save(1, "b", &demo(99), ts(2)).await?, SaveOutcome::Conflict);
    let loaded = load::<Demo>().await?.unwrap();
    assert_eq!(loaded.version, 2);
    assert_eq!(loaded.writer_id, "a");
    assert_eq!(loaded.state, demo(2));
    assert_eq!(loaded.updated_at, ts(1));

    assert_eq!(
        save(2, "b", &demo(3), ts(3)).await?,
        SaveOutcome::Saved { version: 3 }
    );
    assert_eq!(load::<Demo>().await?.unwrap().writer_id, "b");
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn save_without_initialization_is_an_error() -> Result<()> {
    clean_tables().await?;
    assert!(save(1, "a", &demo(1), ts(0)).await.is_err());
    assert!(load::<Demo>().await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn incompatible_payload_is_an_error_not_a_reset() -> Result<()> {
    clean_tables().await?;
    initialize("a", &demo(1), ts(0)).await?;
    let err = load::<Incompatible>().await.unwrap_err();
    assert!(
        format!("{err:#}").contains("does not match the expected schema"),
        "{err:#}"
    );
    // The row is untouched and still loads with the right type.
    assert_eq!(load::<Demo>().await?.unwrap().state, demo(1));
    Ok(())
}
