use super::*;
use crate::ref_flow::test_support::*;
use serial_test::serial;

fn run(secs: i64, stream: RunStream, credits: u32) -> RunRecord {
    RunRecord {
        run_at: ts(secs),
        stream,
        pages: 3,
        receipts: 250,
        trades: 40,
        from_time: Some(ts(secs - 1800)),
        to_time: Some(ts(secs)),
        credits,
        http429: 1,
        failures: 0,
        note: "complete=1".to_owned(),
        parse_stats: Some(serde_json::json!({"rejected": 2})),
    }
}

#[test]
fn run_stream_round_trips_and_is_exhaustive() {
    for stream in RunStream::ALL {
        assert_eq!(stream.to_string().parse::<RunStream>().unwrap(), stream);
    }
    fn _exhaustive(s: RunStream) {
        match s {
            RunStream::Fwd | RunStream::Gap | RunStream::Enrich => {}
        }
    }
    assert!("FWD".parse::<RunStream>().is_err());
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn insert_and_latest_round_trip() -> Result<()> {
    clean_tables().await?;
    assert!(latest(None).await?.is_none());
    let first = run(100, RunStream::Fwd, 7);
    let second = run(200, RunStream::Enrich, 2000);
    insert(&first).await?;
    insert(&second).await?;

    assert_eq!(latest(None).await?, Some(second.clone()));
    assert_eq!(latest(Some(RunStream::Fwd)).await?, Some(first));
    assert!(latest(Some(RunStream::Gap)).await?.is_none());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn check_rejects_unknown_stream() -> Result<()> {
    clean_tables().await?;
    let conn = connection_pool::get_test_only().await?;
    let result = conn
        .interact(|conn| {
            diesel::insert_into(ref_flow_runs::table)
                .values((
                    ref_flow_runs::run_at.eq(ts(0)),
                    ref_flow_runs::stream.eq("history"),
                    ref_flow_runs::pages.eq(0),
                    ref_flow_runs::receipts.eq(0),
                    ref_flow_runs::trades.eq(0),
                    ref_flow_runs::credits.eq(0),
                ))
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))?;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn credits_by_day_groups_per_stream() -> Result<()> {
    clean_tables().await?;
    // ts(0) is 2026-10-24 UTC; 86_400 s later is the next day.
    insert(&run(0, RunStream::Fwd, 10)).await?;
    insert(&run(1800, RunStream::Fwd, 5)).await?;
    insert(&run(3600, RunStream::Enrich, 100)).await?;
    insert(&run(86_400 + 60, RunStream::Fwd, 7)).await?;

    let rows = credits_by_day(ts(0)).await?;
    let summary: Vec<(NaiveDate, RunStream, i64)> = rows
        .iter()
        .map(|r| (r.day, r.stream().unwrap(), r.credits))
        .collect();
    let d0 = ts(0).date();
    let d1 = ts(86_400).date();
    assert_eq!(
        summary,
        vec![
            (d0, RunStream::Enrich, 100),
            (d0, RunStream::Fwd, 15),
            (d1, RunStream::Fwd, 7),
        ]
    );
    assert_eq!(credits_by_day(ts(86_400)).await?.len(), 1);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn delete_before_removes_old_rows_only() -> Result<()> {
    clean_tables().await?;
    insert(&run(0, RunStream::Fwd, 1)).await?;
    insert(&run(5000, RunStream::Fwd, 1)).await?;
    assert_eq!(delete_before(ts(1000)).await?, 1);
    assert_eq!(latest(None).await?.unwrap().run_at, ts(5000));
    Ok(())
}
