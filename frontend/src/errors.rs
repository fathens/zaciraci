/// エラーメッセージの統一管理
use std::fmt::Display;

/// 予測とボラティリティトークン関連のエラー種別
#[derive(Debug, Clone, PartialEq)]
pub enum PredictionError {
    // データ関連
    DataNotFound,
    VolatilityTokensNotFound,
    InsufficientData,
    InsufficientDataAfterSplit,

    // パース関連
    QuoteTokenParseError(String),
    BaseTokenParseError(String),
    StartDateParseError(String),
    EndDateParseError(String),

    // API関連
    PredictionApiError(String),
    VolatilityTokensApiError(String),
    RequestError(String),
    ApiError(String),

    // ネットワーク関連
    NetworkError,

    // システム関連
    QuoteTokenSetupError,
    ChartGenerationError(String),
    SvgGenerationError(String),

    // 予測特化
    PredictionFailed(String),
    ChartGenerationFailed(String),

    // テスト関連（予約）
    EmptyPredictionDataError(String),
    EmptyTestDataError(String),
}

impl Display for PredictionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            // データ関連
            PredictionError::DataNotFound => "データが見つかりませんでした",
            PredictionError::VolatilityTokensNotFound => {
                "ボラティリティトークンが見つかりませんでした"
            }
            PredictionError::InsufficientData => "予測用のデータが不足しています",
            PredictionError::InsufficientDataAfterSplit => "データ分割後のデータが不足しています",

            // パース関連
            PredictionError::QuoteTokenParseError(detail) => {
                return write!(f, "Quote tokenのパースエラー: {}", detail);
            }
            PredictionError::BaseTokenParseError(detail) => {
                return write!(f, "Base tokenのパースエラー: {}", detail);
            }
            PredictionError::StartDateParseError(detail) => {
                return write!(f, "開始日時のパースエラー: {}", detail);
            }
            PredictionError::EndDateParseError(detail) => {
                return write!(f, "終了日時のパースエラー: {}", detail);
            }

            // API関連
            PredictionError::PredictionApiError(detail) => {
                return write!(f, "予測実行エラー: {}", detail);
            }
            PredictionError::VolatilityTokensApiError(detail) => {
                return write!(f, "ボラティリティトークン取得エラー: {}", detail);
            }
            PredictionError::RequestError(detail) => {
                return write!(f, "リクエストエラー: {}", detail);
            }
            PredictionError::ApiError(detail) => return write!(f, "APIエラー: {}", detail),

            // ネットワーク関連
            PredictionError::NetworkError => "ネットワークエラー",

            // システム関連
            PredictionError::QuoteTokenSetupError => "quote_tokenの設定に失敗しました",
            PredictionError::ChartGenerationError(detail) => {
                return write!(f, "チャート生成エラー: {}", detail);
            }
            PredictionError::SvgGenerationError(detail) => {
                return write!(f, "SVG生成エラー: {}", detail);
            }

            // 予測特化
            PredictionError::PredictionFailed(detail) => return write!(f, "{}", detail),
            PredictionError::ChartGenerationFailed(detail) => return write!(f, "{}", detail),

            // テスト関連（予約）
            PredictionError::EmptyPredictionDataError(detail) => {
                return write!(f, "空の予測データでエラー: {}", detail);
            }
            PredictionError::EmptyTestDataError(detail) => {
                return write!(f, "空のテストデータでエラー: {}", detail);
            }
        };
        write!(f, "{}", message)
    }
}

// 特定のケース用のメッセージ
impl PredictionError {
    /// ボラティリティトークンが見つからない場合の特定メッセージ
    pub fn volatility_tokens_not_found() -> String {
        "ボラティリティトークンが見つかりませんでした".to_string()
    }
}

/// エラーから文字列への簡易変換
impl From<PredictionError> for String {
    fn from(error: PredictionError) -> Self {
        error.to_string()
    }
}
