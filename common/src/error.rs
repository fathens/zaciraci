use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    Authentication(String),
    Authorization(String),
    BadRequest(String),
    Internal(String),
    NotFound(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authentication(msg) => write!(f, "認証エラー: {}", msg),
            Self::Authorization(msg) => write!(f, "権限エラー: {}", msg),
            Self::BadRequest(msg) => write!(f, "リクエストエラー: {}", msg),
            Self::Internal(msg) => write!(f, "内部サーバーエラー: {}", msg),
            Self::NotFound(msg) => write!(f, "リソースが見つかりません: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}
