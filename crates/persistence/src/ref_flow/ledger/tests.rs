use super::*;
use crate::ref_flow::test_support::*;
use serial_test::serial;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn add_requests_creates_then_accumulates() -> Result<()> {
    clean_tables().await?;
    assert_eq!(requests_on(day(1)).await?, 0);
    assert_eq!(add_requests(day(1), 10).await?, 10);
    assert_eq!(add_requests(day(1), 5).await?, 15);
    assert_eq!(add_requests(day(1), 0).await?, 15);
    assert_eq!(requests_on(day(1)).await?, 15);
    assert_eq!(requests_on(day(2)).await?, 0);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn window_is_inclusive_and_ordered() -> Result<()> {
    clean_tables().await?;
    add_requests(day(1), 1).await?;
    add_requests(day(3), 3).await?;
    add_requests(day(5), 5).await?;
    add_requests(day(9), 9).await?;

    let w = window(day(3), day(5)).await?;
    assert_eq!(
        w.into_iter().collect::<Vec<_>>(),
        vec![(day(3), 3), (day(5), 5)]
    );
    assert!(window(day(6), day(8)).await?.is_empty());
    assert_eq!(window(day(1), day(9)).await?.len(), 4);
    Ok(())
}
