//! `NewPredictionRecord::try_new` の不変条件検証 (DB 不要のユニットテスト)。

use super::*;

/// 基準時刻を作成 (DB を使わないので任意の固定値で良い)
fn base_time() -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_700_000_000, 0)
        .unwrap()
        .naive_utc()
}

/// `created_at == data_cutoff_time` (= 同時刻) は不変条件を満たすため通る。
///
/// 既存の test helper / production caller の代表的な呼び出しパターン
/// (cutoff 時刻と created_at を等しく渡す) が引き続き許容されることを確認する。
#[test]
fn try_new_accepts_created_at_equal_to_data_cutoff() {
    let t = base_time();
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        t,
        t + chrono::TimeDelta::hours(24),
        t,
    );
    assert!(result.is_ok());
}

/// `created_at > data_cutoff_time` (= cutoff 後に予測を生成) は通る。
///
/// production の典型ケース: data 取得後に predict が走り、`Utc::now()` を
/// `created_at` として渡す経路。
#[test]
fn try_new_accepts_created_at_after_data_cutoff() {
    let t = base_time();
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        t,
        t + chrono::TimeDelta::hours(24),
        t + chrono::TimeDelta::minutes(5),
    );
    assert!(result.is_ok());
}

/// `created_at < data_cutoff_time` (= 「未来データを使った過去予測」) は
/// `CreatedAtBeforeCutoff` で `Err` を返す。
///
/// release ビルドでも確実に検出可能 (Layer 1 fail-soft 防御)。
#[test]
fn try_new_rejects_created_at_before_data_cutoff() {
    let t = base_time();
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        t,
        t + chrono::TimeDelta::hours(24),
        t - chrono::TimeDelta::seconds(1),
    );
    assert!(matches!(
        result,
        Err(NewPredictionRecordError::CreatedAtBeforeCutoff { .. })
    ));
}

/// `target_time == data_cutoff_time` (= horizon 0) は `NonPositiveHorizon` で
/// `Err` を返す。
///
/// horizon 0 以下の予測は「データカットオフと同時刻を予測」する壊れたレコードで
/// あり、caller-side で弾く。
#[test]
fn try_new_rejects_target_time_equal_to_data_cutoff() {
    let t = base_time();
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        t,
        t,
        t,
    );
    assert!(matches!(
        result,
        Err(NewPredictionRecordError::NonPositiveHorizon { .. })
    ));
}

/// `target_time < data_cutoff_time` (= horizon 負値) も `NonPositiveHorizon` で
/// `Err` を返す。
#[test]
fn try_new_rejects_target_time_before_data_cutoff() {
    let t = base_time();
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        t,
        t - chrono::TimeDelta::seconds(1),
        t,
    );
    assert!(matches!(
        result,
        Err(NewPredictionRecordError::NonPositiveHorizon { .. })
    ));
}

/// production の stale-data 経路 (`target_time < created_at` だが horizon > 0) は
/// 通る。
///
/// `data_cutoff = now - 3d` のようにデータが 24h 以上 stale なケース:
/// `target_time = data_cutoff + 24h = now - 2d`, `created_at = now` → target は
/// 過去だが horizon > 0。caller-side では horizon 正値のみ要求し、`target_time`
/// が created_at より過去かどうかの判定は SQL filter (`earliest_fresh_visible_in`)
/// に委ねる。
#[test]
fn try_new_accepts_stale_data_with_positive_horizon() {
    let t = base_time();
    // 3 日前のデータカットオフ → target_time = 2 日前 (now より過去)
    let data_cutoff = t - chrono::TimeDelta::days(3);
    let target = data_cutoff + chrono::TimeDelta::hours(24);
    let result = NewPredictionRecord::try_new(
        "tok.near".to_string(),
        "wrap.near".to_string(),
        BigDecimal::from(100),
        data_cutoff,
        target,
        t,
    );
    assert!(result.is_ok());
}

/// `Display` 出力に攻撃者制御の任意文字列が含まれないことを確認 (log forwarding
/// 経由の secret 漏洩防御)。フィールド値のみが構造化フォーマットで含まれる。
#[test]
fn error_display_uses_only_structured_fields() {
    let t = base_time();
    let err = NewPredictionRecordError::CreatedAtBeforeCutoff {
        created_at: t - chrono::TimeDelta::seconds(1),
        data_cutoff_time: t,
    };
    let s = format!("{err}");
    assert!(s.contains("created_at"));
    assert!(s.contains("data_cutoff_time"));
    assert!(s.contains("data-leakage"));
}
