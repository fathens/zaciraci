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
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            fallback_multiplier: 1.05,
            quote_token: "wrap.near".parse().unwrap(),
            chart_width: 600,
            chart_height: 300,
            default_limit: 1000,
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

        Self {
            fallback_multiplier,
            quote_token,
            chart_width,
            chart_height,
            default_limit,
        }
    }

    /// チャートサイズをタプルで取得
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
