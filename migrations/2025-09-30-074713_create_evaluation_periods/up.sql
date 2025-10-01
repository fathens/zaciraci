-- 評価期間テーブルの作成
CREATE TABLE evaluation_periods (
    id SERIAL PRIMARY KEY,

    -- 評価期間の識別
    period_id VARCHAR NOT NULL UNIQUE,          -- UUID

    -- 期間開始時刻
    start_time TIMESTAMP NOT NULL,              -- 評価期間開始日時

    -- 期間開始時の資金状況
    initial_value NUMERIC(39, 0) NOT NULL,      -- 期間開始時の総価値（yoctoNEAR）

    -- トークン選定情報
    selected_tokens TEXT[],                     -- 選定されたトークンリスト
    token_count INTEGER NOT NULL,               -- 選定トークン数

    -- メタデータ
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- インデックス
CREATE INDEX idx_evaluation_periods_start_time ON evaluation_periods(start_time DESC);
CREATE INDEX idx_evaluation_periods_period_id ON evaluation_periods(period_id);