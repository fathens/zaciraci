use crate::api_underlying::Underlying;
use anyhow::Result;
use chrono::{DateTime, Utc};
use gloo_timers::future::TimeoutFuture;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use web_sys;
use zaciraci_common::config;

fn chronos_base_url() -> String {
    let url =
        config::get("CHRONOS_BASE_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());

    // 初回接続時にURL情報をログ出力
    web_sys::console::log_1(
        &format!(
            "=== Chronos API 接続設定 ===\n\
         URL: {}\n\
         環境変数CHRONOS_BASE_URLの設定状況: {}",
            url,
            if config::get("CHRONOS_BASE_URL").is_ok() {
                "設定済み"
            } else {
                "未設定（デフォルト使用）"
            }
        )
        .into(),
    );

    url
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

// 非同期予測リクエスト
#[derive(Debug, Serialize, Deserialize)]
pub struct AsyncPredictionRequest {
    pub timestamp: Vec<DateTime<Utc>>,
    pub values: Vec<f64>,
    pub forecast_until: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_params: Option<HashMap<String, serde_json::Value>>,
}

// 非同期予測レスポンス
#[derive(Debug, Serialize, Deserialize)]
pub struct AsyncPredictionResponse {
    pub task_id: String,
    pub status: String,
}

// 予測ステータス
#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionStatus {
    pub task_id: String,
    pub status: String,
    pub progress: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<PredictionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ChronosApiClient {
    pub underlying: Arc<Underlying>,
}

impl ChronosApiClient {
    // 非同期ゼロショット予測を開始
    pub async fn predict_zero_shot_async(
        &self,
        request: &ZeroShotPredictionRequest,
    ) -> Result<AsyncPredictionResponse> {
        let async_request = AsyncPredictionRequest {
            timestamp: request.timestamp.clone(),
            values: request.values.clone(),
            forecast_until: request.forecast_until,
            model_name: request.model_name.clone(),
            model_params: request.model_params.clone(),
        };

        // リクエスト詳細をログ出力
        web_sys::console::log_1(
            &format!(
                "=== Chronos API リクエスト詳細 ===\n\
             URL: {}/api/v1/predict_zero_shot_async\n\
             タイムスタンプ数: {}\n\
             値の数: {}\n\
             予測終了時刻: {}\n\
             モデル名: {:?}\n\
             値のサンプル（先頭5個）: {:?}\n\
             タイムスタンプサンプル（先頭3個）: {:?}",
                chronos_base_url(),
                async_request.timestamp.len(),
                async_request.values.len(),
                async_request.forecast_until,
                async_request.model_name,
                async_request.values.iter().take(5).collect::<Vec<_>>(),
                async_request.timestamp.iter().take(3).collect::<Vec<_>>()
            )
            .into(),
        );

        let result: Result<AsyncPredictionResponse> = self
            .underlying
            .post("api/v1/predict_zero_shot_async", &async_request)
            .await;

        match &result {
            Ok(response) => {
                web_sys::console::log_1(
                    &format!(
                        "【成功】Chronos API リクエスト開始: task_id={}, status={}",
                        response.task_id, response.status
                    )
                    .into(),
                );
            }
            Err(e) => {
                web_sys::console::error_1(
                    &format!("【エラー】Chronos API リクエスト失敗: {}", e).into(),
                );

                // ネットワークエラーの詳細を出力
                web_sys::console::error_1(
                    &format!(
                        "接続先URL: {}/api/v1/predict_zero_shot_async",
                        chronos_base_url()
                    )
                    .into(),
                );
            }
        }

        result
    }

    // 予測ステータスを取得
    pub async fn get_prediction_status(&self, task_id: &str) -> Result<PredictionStatus> {
        self.underlying
            .get(&format!("api/v1/prediction_status/{}", task_id))
            .await
    }

    // ポーリングによる予測実行（共通化された処理）
    pub async fn predict_with_polling(
        &self,
        request: &ZeroShotPredictionRequest,
        progress_callback: Option<Box<dyn Fn(f64, String)>>,
    ) -> Result<PredictionResponse> {
        // 非同期予測を開始
        let async_response = self.predict_zero_shot_async(request).await?;

        if let Some(callback) = &progress_callback {
            callback(
                0.0,
                format!("予測タスクを開始しました: {}", async_response.task_id),
            );
        }

        // ポーリングループ（高品質予測のため65分に延長）
        for attempt in 0..3900 {
            // 65分間ポーリング（1時間学習＋5分余裕）
            TimeoutFuture::new(1000).await; // 1秒待機

            match self.get_prediction_status(&async_response.task_id).await {
                Ok(status) => {
                    if let Some(callback) = &progress_callback {
                        callback(
                            status.progress,
                            format!(
                                "ステータス: {} ({:.1}%)",
                                status.status,
                                status.progress * 100.0
                            ),
                        );
                    }

                    match status.status.to_uppercase().as_str() {
                        "COMPLETED" => {
                            if let Some(result) = status.result {
                                if let Some(callback) = &progress_callback {
                                    callback(1.0, "予測が完了しました".to_string());
                                }

                                // APIレスポンスの詳細ログを出力
                                web_sys::console::log_1(
                                    &format!(
                                        "=== Chronos API レスポンス詳細 ===\n\
                                     モデル名: {}\n\
                                     予測タイムスタンプ数: {}\n\
                                     予測値数: {}",
                                        result.model_name,
                                        result.forecast_timestamp.len(),
                                        result.forecast_values.len()
                                    )
                                    .into(),
                                );

                                // 予測値の raw data をログ出力（先頭10個と末尾10個）
                                if !result.forecast_values.is_empty() {
                                    let head_count = result.forecast_values.len().min(10);
                                    let tail_count = result.forecast_values.len().min(10);

                                    web_sys::console::log_1(
                                        &format!(
                                            "RAW予測値サンプル（先頭{}個）: {:?}",
                                            head_count,
                                            &result.forecast_values[..head_count]
                                        )
                                        .into(),
                                    );

                                    if result.forecast_values.len() > 10 {
                                        web_sys::console::log_1(
                                            &format!(
                                                "RAW予測値サンプル（末尾{}個）: {:?}",
                                                tail_count,
                                                &result.forecast_values
                                                    [result.forecast_values.len() - tail_count..]
                                            )
                                            .into(),
                                        );
                                    }

                                    // 統計情報
                                    let min_val = result
                                        .forecast_values
                                        .iter()
                                        .fold(f64::INFINITY, |a, &b| a.min(b));
                                    let max_val = result
                                        .forecast_values
                                        .iter()
                                        .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                                    let mean_val = result.forecast_values.iter().sum::<f64>()
                                        / result.forecast_values.len() as f64;

                                    // 同一値問題の検出
                                    let unique_values: std::collections::HashSet<_> = result
                                        .forecast_values
                                        .iter()
                                        .map(|&x| (x * 1000000.0) as i64)
                                        .collect();
                                    let is_same_values = unique_values.len() == 1;

                                    web_sys::console::log_1(
                                        &format!(
                                            "RAW予測値統計:\n\
                                         - 最小値: {}\n\
                                         - 最大値: {}\n\
                                         - 平均値: {}\n\
                                         - ユニークな値の数: {}\n\
                                         - 【問題】全て同じ値か?: {}",
                                            min_val,
                                            max_val,
                                            mean_val,
                                            unique_values.len(),
                                            is_same_values
                                        )
                                        .into(),
                                    );

                                    if is_same_values {
                                        web_sys::console::error_1(&format!(
                                            "【重大な問題】予測値が全て同一です！\n\
                                             値: {}\n\
                                             データ数: {}\n\
                                             これはAutoGluonが適切に予測を生成していないことを示しています。",
                                            result.forecast_values[0],
                                            result.forecast_values.len()
                                        ).into());
                                    }
                                }

                                return Ok(result);
                            } else {
                                return Err(anyhow::anyhow!("予測結果がありません"));
                            }
                        }
                        "FAILED" => {
                            let error_msg = status
                                .error
                                .unwrap_or_else(|| "予測が失敗しました".to_string());
                            return Err(anyhow::anyhow!("予測失敗: {}", error_msg));
                        }
                        _ => {
                            // PENDING または RUNNING の場合は継続
                            continue;
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::log_1(
                        &format!("ポーリングエラー (attempt {}): {}", attempt, e).into(),
                    );
                    if attempt < 5 {
                        // 最初の5回は再試行
                        continue;
                    } else {
                        // ポーリングに失敗した場合はエラーとして扱う
                        return Err(anyhow::anyhow!("ポーリングエラー: {}", e));
                    }
                }
            }
        }

        // タイムアウトした場合はエラーとして扱う
        Err(anyhow::anyhow!(
            "ポーリングタイムアウト: 65分を超えました（高品質予測には時間がかかります）"
        ))
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

        log::debug!("routes.pyのサンプル日時文字列のパースに成功しました");
    }

    #[test]
    fn test_request_without_model_name() {
        // モデル名を省略したリクエストのテスト
        let timestamp_strs = vec![
            "2023-01-01T00:00:00",
            "2023-01-01T01:00:00",
            "2023-01-01T02:00:00",
        ];
        let dts = parse_datetime_vec(&timestamp_strs).expect("タイムスタンプ配列のパースに失敗");

        // モデル名なしでリクエストを作成
        let request_without_model = ZeroShotPredictionRequest::new(
            dts.clone(),
            vec![10.5, 11.2, 10.8],
            parse_datetime("2023-01-04T02:00:00").expect("forecast_untilのパースに失敗"),
        );

        // モデル名ありでリクエストを作成
        let request_with_model = ZeroShotPredictionRequest::new(
            dts,
            vec![10.5, 11.2, 10.8],
            parse_datetime("2023-01-04T02:00:00").expect("forecast_untilのパースに失敗"),
        )
        .with_model_name("chronos-bolt-base");

        // シリアライズして比較
        let json_without_model =
            serde_json::to_string(&request_without_model).expect("シリアライズに失敗");
        let json_with_model =
            serde_json::to_string(&request_with_model).expect("シリアライズに失敗");

        log::debug!("モデル名なし: {}", json_without_model);
        log::debug!("モデル名あり: {}", json_with_model);

        // モデル名なしの場合、JSONに含まれないことを確認
        assert!(!json_without_model.contains("model_name"));
        assert!(json_with_model.contains("model_name"));
        assert!(json_with_model.contains("chronos-bolt-base"));

        // デシリアライズテスト
        let deserialized_without: ZeroShotPredictionRequest =
            serde_json::from_str(&json_without_model).expect("デシリアライズに失敗");
        let deserialized_with: ZeroShotPredictionRequest =
            serde_json::from_str(&json_with_model).expect("デシリアライズに失敗");

        assert!(deserialized_without.model_name.is_none());
        assert_eq!(
            deserialized_with.model_name,
            Some("chronos-bolt-base".to_string())
        );
    }

    #[test]
    fn test_request_json_compatibility() {
        // まず単一の日時文字列をパースしてみる
        let timestamp_str = "2023-01-01T00:00:00";
        let dt = parse_datetime(timestamp_str).expect("日時文字列のパースに失敗");
        log::debug!("パースした日時: {}", dt);

        // 次に日時文字列の配列をパースする
        let timestamp_strs = vec![
            "2023-01-01T00:00:00",
            "2023-01-01T01:00:00",
            "2023-01-01T02:00:00",
        ];
        let dts = parse_datetime_vec(&timestamp_strs).expect("タイムスタンプ配列のパースに失敗");
        log::debug!("パースした日時配列の長さ: {}", dts.len());

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
        log::debug!("シリアライズされたJSON: {}", serialized);

        let deserialized: ZeroShotPredictionRequest =
            serde_json::from_str(&serialized).expect("デシリアライズに失敗");

        // 検証
        assert_eq!(deserialized.timestamp.len(), request.timestamp.len());
        assert_eq!(deserialized.values, request.values);

        log::debug!("シリアライズ・デシリアライズのテストに成功しました");
    }
}
