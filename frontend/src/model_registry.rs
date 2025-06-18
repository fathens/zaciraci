/// 利用可能な予測モデルの定義とメタデータ

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: u32, // パラメータ数（百万単位）
    pub speed: ModelSpeed,
    pub accuracy: ModelAccuracy,
    pub recommended_for: &'static str,
}

#[derive(Debug, Clone)]
pub enum ModelSpeed {
    Fast,
    Medium,
    Slow,
}

#[derive(Debug, Clone)]
pub enum ModelAccuracy {
    High,
    Medium,
    Low,
}

impl ModelSpeed {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelSpeed::Fast => "高速",
            ModelSpeed::Medium => "中速",
            ModelSpeed::Slow => "低速",
        }
    }
}

impl ModelAccuracy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelAccuracy::High => "高精度",
            ModelAccuracy::Medium => "中精度",
            ModelAccuracy::Low => "低精度",
        }
    }
}

/// Chronos-Boltモデルファミリー（推奨）
pub const CHRONOS_BOLT_TINY: ModelInfo = ModelInfo {
    id: "chronos-bolt-tiny",
    name: "Chronos Bolt Tiny",
    description: "最軽量モデル、リアルタイム予測に最適",
    parameters: 9,
    speed: ModelSpeed::Fast,
    accuracy: ModelAccuracy::Medium,
    recommended_for: "リアルタイム取引、高頻度予測",
};

pub const CHRONOS_BOLT_MINI: ModelInfo = ModelInfo {
    id: "chronos-bolt-mini",
    name: "Chronos Bolt Mini",
    description: "軽量で効率的、バランスの取れた性能",
    parameters: 21,
    speed: ModelSpeed::Fast,
    accuracy: ModelAccuracy::Medium,
    recommended_for: "一般的な価格予測、分析ダッシュボード",
};

pub const CHRONOS_BOLT_SMALL: ModelInfo = ModelInfo {
    id: "chronos-bolt-small",
    name: "Chronos Bolt Small",
    description: "高性能と効率のバランス",
    parameters: 48,
    speed: ModelSpeed::Medium,
    accuracy: ModelAccuracy::High,
    recommended_for: "中期予測、ポートフォリオ分析",
};

pub const CHRONOS_BOLT_BASE: ModelInfo = ModelInfo {
    id: "chronos-bolt-base",
    name: "Chronos Bolt Base",
    description: "最高精度、複雑なパターン認識",
    parameters: 205,
    speed: ModelSpeed::Medium,
    accuracy: ModelAccuracy::High,
    recommended_for: "長期予測、機関投資家向け分析",
};

/// レガシーChronos-T5モデル（互換性のため）
pub const CHRONOS_T5_TINY: ModelInfo = ModelInfo {
    id: "chronos-t5-tiny",
    name: "Chronos T5 Tiny",
    description: "レガシーモデル、基本的な予測",
    parameters: 8,
    speed: ModelSpeed::Medium,
    accuracy: ModelAccuracy::Low,
    recommended_for: "テスト、実験用途",
};

pub const CHRONOS_T5_SMALL: ModelInfo = ModelInfo {
    id: "chronos-t5-small",
    name: "Chronos T5 Small",
    description: "レガシーモデル、従来のChronos実装",
    parameters: 46,
    speed: ModelSpeed::Slow,
    accuracy: ModelAccuracy::Medium,
    recommended_for: "既存システムとの互換性",
};

pub const CHRONOS_T5_BASE: ModelInfo = ModelInfo {
    id: "chronos-t5-base",
    name: "Chronos T5 Base",
    description: "レガシーモデル、高精度だが低速",
    parameters: 200,
    speed: ModelSpeed::Slow,
    accuracy: ModelAccuracy::High,
    recommended_for: "バッチ処理、オフライン分析",
};

/// 従来の統計モデル
pub const PROPHET: ModelInfo = ModelInfo {
    id: "prophet",
    name: "Prophet",
    description: "Facebook開発の時系列予測ライブラリ",
    parameters: 0, // 統計モデルのためパラメータ数は適用外
    speed: ModelSpeed::Fast,
    accuracy: ModelAccuracy::Medium,
    recommended_for: "トレンドと季節性のある時系列",
};

pub const ARIMA: ModelInfo = ModelInfo {
    id: "arima",
    name: "ARIMA",
    description: "自己回帰移動平均モデル、古典的手法",
    parameters: 0,
    speed: ModelSpeed::Fast,
    accuracy: ModelAccuracy::Low,
    recommended_for: "短期予測、ベースライン比較",
};

/// サーバーデフォルトモデル（省略時に使用）
pub const SERVER_DEFAULT: ModelInfo = ModelInfo {
    id: "chronos_default",
    name: "Server Default (DeepAR)",
    description: "サーバー側で自動選択されるAutoGluon TimeSeries DeepAR",
    parameters: 0, // AutoGluonが動的に決定
    speed: ModelSpeed::Medium,
    accuracy: ModelAccuracy::High,
    recommended_for: "自動最適化、開発・実験用途",
};

/// 利用可能な全モデルのリスト
pub const ALL_MODELS: &[ModelInfo] = &[
    // サーバーデフォルト
    SERVER_DEFAULT,
    // 推奨Chronos-Boltモデル
    CHRONOS_BOLT_BASE,
    CHRONOS_BOLT_SMALL,
    CHRONOS_BOLT_MINI,
    CHRONOS_BOLT_TINY,
    // レガシーChronos-T5モデル
    CHRONOS_T5_BASE,
    CHRONOS_T5_SMALL,
    CHRONOS_T5_TINY,
    // 統計モデル
    PROPHET,
    ARIMA,
];

/// 推奨モデル（パフォーマンスとコスト効率のバランス）
pub const RECOMMENDED_MODELS: &[ModelInfo] =
    &[CHRONOS_BOLT_BASE, CHRONOS_BOLT_SMALL, CHRONOS_BOLT_MINI];

/// モデルIDから情報を取得
pub fn get_model_info(model_id: &str) -> Option<&ModelInfo> {
    ALL_MODELS.iter().find(|model| model.id == model_id)
}

/// 使用ケース別の推奨モデル
#[allow(dead_code)]
pub struct ModelRecommendations;

#[allow(dead_code)]
impl ModelRecommendations {
    /// リアルタイム取引向け
    pub fn for_realtime_trading() -> &'static ModelInfo {
        &CHRONOS_BOLT_TINY
    }

    /// 一般的な価格分析向け
    pub fn for_general_analysis() -> &'static ModelInfo {
        &CHRONOS_BOLT_SMALL
    }

    /// 高精度分析向け
    pub fn for_high_accuracy() -> &'static ModelInfo {
        &CHRONOS_BOLT_BASE
    }

    /// 互換性重視
    pub fn for_compatibility() -> &'static ModelInfo {
        &CHRONOS_T5_SMALL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_lookup() {
        assert!(get_model_info("chronos-bolt-base").is_some());
        assert!(get_model_info("invalid-model").is_none());
    }

    #[test]
    fn test_recommendations() {
        let realtime = ModelRecommendations::for_realtime_trading();
        assert_eq!(realtime.id, "chronos-bolt-tiny");

        let general = ModelRecommendations::for_general_analysis();
        assert_eq!(general.id, "chronos-bolt-small");
    }
}
