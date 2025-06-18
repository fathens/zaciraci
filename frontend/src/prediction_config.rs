use zaciraci_common::config;
use zaciraci_common::types::TokenAccount;

/// 予測機能の設定値
#[derive(Debug, Clone)]
pub struct PredictionConfig {
    /// 代替価格計算の乗数（デフォルト: 1.05）
    pub fallback_multiplier: f64,
    /// デフォルトのquoteトークン（デフォルト: "wrap.near"）
    pub quote_token: TokenAccount,
    /// チャートサイズ幅（デフォルト: 600）
    pub chart_width: u32,
    /// チャートサイズ高さ（デフォルト: 300）
    pub chart_height: u32,
    /// デフォルト制限数（デフォルト: 1000）
    pub default_limit: u32,
    /// データ正規化：移動平均ウィンドウサイズ（デフォルト: 5）
    pub normalization_window: usize,
    /// データ正規化：異常値検出の閾値（デフォルト: 2.5）
    pub outlier_threshold: f64,
    /// データ正規化：最大変化率（デフォルト: 0.5）
    pub max_change_ratio: f64,
    /// データ正規化を有効にするかどうか（デフォルト: true）
    pub enable_normalization: bool,
    /// デフォルトの予測モデル名（デフォルト: "chronos_default"）
    pub default_model_name: String,
    /// モデル指定を省略するかどうか（デフォルト: true
    pub omit_model_name: bool,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            fallback_multiplier: 1.05,
            quote_token: "wrap.near".parse().unwrap(),
            chart_width: 600,
            chart_height: 300,
            default_limit: 1000,
            normalization_window: 5,
            outlier_threshold: 2.5,
            max_change_ratio: 0.5,
            enable_normalization: true,
            default_model_name: "chronos_default".to_string(),
            omit_model_name: true,
        }
    }
}

impl PredictionConfig {
    /// 環境変数から設定を読み込む
    pub fn from_env() -> Self {
        let default_config = Self::default();

        let fallback_multiplier = config::get("PREDICTION_FALLBACK_MULTIPLIER")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default_config.fallback_multiplier);

        let quote_token = config::get("PREDICTION_DEFAULT_QUOTE_TOKEN")
            .ok()
            .and_then(|s| s.parse::<TokenAccount>().ok())
            .unwrap_or(default_config.quote_token);

        let chart_width = config::get("PREDICTION_CHART_WIDTH")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(default_config.chart_width);

        let chart_height = config::get("PREDICTION_CHART_HEIGHT")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(default_config.chart_height);

        let default_limit = config::get("PREDICTION_DEFAULT_LIMIT")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(default_config.default_limit);

        let normalization_window = config::get("PREDICTION_NORMALIZATION_WINDOW")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default_config.normalization_window);

        let outlier_threshold = config::get("PREDICTION_OUTLIER_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default_config.outlier_threshold);

        let max_change_ratio = config::get("PREDICTION_MAX_CHANGE_RATIO")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default_config.max_change_ratio);

        let enable_normalization = config::get("PREDICTION_ENABLE_NORMALIZATION")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(default_config.enable_normalization);

        let default_model_name = config::get("PREDICTION_DEFAULT_MODEL")
            .ok()
            .unwrap_or(default_config.default_model_name);

        let omit_model_name = config::get("PREDICTION_OMIT_MODEL_NAME")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(default_config.omit_model_name);

        Self {
            fallback_multiplier,
            quote_token,
            chart_width,
            chart_height,
            default_limit,
            normalization_window,
            outlier_threshold,
            max_change_ratio,
            enable_normalization,
            default_model_name,
            omit_model_name,
        }
    }

    /// チャートサイズをタプルで取得
    #[allow(dead_code)]
    pub fn chart_size(&self) -> (u32, u32) {
        (self.chart_width, self.chart_height)
    }
}

/// グローバル設定インスタンス
static CONFIG: std::sync::OnceLock<PredictionConfig> = std::sync::OnceLock::new();

/// グローバル設定を取得
pub fn get_config() -> &'static PredictionConfig {
    CONFIG.get_or_init(PredictionConfig::from_env)
}
