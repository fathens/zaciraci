use crate::api_underlying::Underlying;
use anyhow::Result;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use zaciraci_common::config;

fn chronos_base_url() -> String {
    config::get("CHRONOS_BASE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}

// ゼロショット予測リクエスト
#[derive(Debug, Serialize, Deserialize)]
pub struct ZeroShotPredictionRequest {
    pub timestamp: Vec<DateTime<Utc>>,
    pub values: Vec<f64>,
    pub forecast_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_params: Option<HashMap<String, serde_json::Value>>,
}

impl ZeroShotPredictionRequest {
    /// 新しいZeroShotPredictionRequestを作成する
    pub fn new(
        timestamp: Vec<DateTime<Utc>>,
        values: Vec<f64>,
        forecast_until: DateTime<Utc>,
    ) -> Self {
        Self {
            timestamp,
            values,
            forecast_until,
            model_name: None,
            model_params: None,
        }
    }

    /// モデル名を設定する
    pub fn with_model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    /// モデルパラメータを設定する
    #[allow(dead_code)]
    pub fn with_model_params(mut self, model_params: HashMap<String, serde_json::Value>) -> Self {
        self.model_params = Some(model_params);
        self
    }

    /// モデルパラメータに単一のキーと値を追加する
    #[allow(dead_code)]
    pub fn add_model_param(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        let params = self.model_params.get_or_insert_with(HashMap::new);
        params.insert(key.into(), value.into());
        self
    }
}

// 予測レスポンス
#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub forecast_timestamp: Vec<DateTime<Utc>>,
    pub forecast_values: Vec<f64>,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_intervals: Option<HashMap<String, Vec<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<HashMap<String, f64>>,
}

pub struct ChronosApiClient {
    pub underlying: Arc<Underlying>,
}

impl ChronosApiClient {
    // ゼロショット予測APIを呼び出す
    pub async fn predict_zero_shot(
        &self,
        request: &ZeroShotPredictionRequest,
    ) -> Result<PredictionResponse> {
        self.underlying
            .post("api/v1/predict_zero_shot", request)
            .await
    }
}

static CHRONOS_API_CLIENT: Lazy<Arc<ChronosApiClient>> = Lazy::new(|| {
    Arc::new(ChronosApiClient {
        underlying: Underlying::new_shared(chronos_base_url()),
    })
});

pub fn get_client() -> Arc<ChronosApiClient> {
    CHRONOS_API_CLIENT.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    /// 日時文字列をDateTime<Utc>型に変換する
    fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
        // 最初にISO 8601形式（タイムゾーン情報あり）でパースを試みる
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(dt.with_timezone(&Utc));
        }

        // タイムゾーン情報なしの基本フォーマットを試す
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(dt, Utc));
        }

        // マイクロ秒を含むフォーマットを試す
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
            return Ok(DateTime::from_naive_utc_and_offset(dt, Utc));
        }

        // どのフォーマットにも合致しない場合はエラー
        Err(anyhow::anyhow!("Unsupported datetime format: {}", s))
    }

    /// タイムゾーン情報なしの日時文字列ベクターをDateTime<Utc>ベクターとしてパースする
    fn parse_datetime_vec(strs: &[&str]) -> Result<Vec<DateTime<Utc>>> {
        strs.iter().map(|s| parse_datetime(s)).collect()
    }

    #[test]
    fn test_parse_datetime_samples() {
        // routes.pyのサンプル日時文字列
        let sample_timestamps = vec![
            "2023-01-01T00:00:00",
            "2023-01-01T01:00:00",
            "2023-01-01T02:00:00",
        ];
        let sample_forecast_until = "2023-01-04T02:00:00";

        // タイムスタンプのパースをテスト
        let parsed_timestamps =
            parse_datetime_vec(&sample_timestamps).expect("タイムスタンプのパースに失敗");
        assert_eq!(parsed_timestamps.len(), 3);

        // 予測時点のパースをテスト
        let parsed_forecast_until =
            parse_datetime(sample_forecast_until).expect("予測時点のパースに失敗");

        // 期待値の確認
        assert_eq!(
            parsed_timestamps[0].format("%Y-%m-%d %H:%M:%S").to_string(),
            "2023-01-01 00:00:00"
        );
        assert_eq!(
            parsed_timestamps[1].format("%Y-%m-%d %H:%M:%S").to_string(),
            "2023-01-01 01:00:00"
        );
        assert_eq!(
            parsed_timestamps[2].format("%Y-%m-%d %H:%M:%S").to_string(),
            "2023-01-01 02:00:00"
        );
        assert_eq!(
            parsed_forecast_until
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            "2023-01-04 02:00:00"
        );

        println!("routes.pyのサンプル日時文字列のパースに成功しました");
    }

    #[test]
    fn test_request_json_compatibility() {
        // まず単一の日時文字列をパースしてみる
        let timestamp_str = "2023-01-01T00:00:00";
        let dt = parse_datetime(timestamp_str).expect("日時文字列のパースに失敗");
        println!("パースした日時: {}", dt);

        // 次に日時文字列の配列をパースする
        let timestamp_strs = vec![
            "2023-01-01T00:00:00",
            "2023-01-01T01:00:00",
            "2023-01-01T02:00:00",
        ];
        let dts = parse_datetime_vec(&timestamp_strs).expect("タイムスタンプ配列のパースに失敗");
        println!("パースした日時配列の長さ: {}", dts.len());

        // 手動でオブジェクトを構築
        let request = ZeroShotPredictionRequest {
            timestamp: dts,
            values: vec![10.5, 11.2, 10.8],
            forecast_until: parse_datetime("2023-01-04T02:00:00")
                .expect("forecast_untilのパースに失敗"),
            model_name: Some("chronos_default".to_string()),
            model_params: {
                let mut params = HashMap::new();
                params.insert(
                    "seasonality_mode".to_string(),
                    serde_json::json!("multiplicative"),
                );
                Some(params)
            },
        };

        // シリアライズ・デシリアライズのテスト
        let serialized = serde_json::to_string(&request).expect("シリアライズに失敗");
        println!("シリアライズされたJSON: {}", serialized);

        let deserialized: ZeroShotPredictionRequest =
            serde_json::from_str(&serialized).expect("デシリアライズに失敗");

        // 検証
        assert_eq!(deserialized.timestamp.len(), request.timestamp.len());
        assert_eq!(deserialized.values, request.values);

        println!("シリアライズ・デシリアライズのテストに成功しました");
    }
}
