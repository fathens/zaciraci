-- ロールバック: token_count カラムを再追加
ALTER TABLE evaluation_periods ADD COLUMN token_count INTEGER NOT NULL DEFAULT 0;

-- 既存データの token_count を selected_tokens の長さで更新
UPDATE evaluation_periods
SET token_count = COALESCE(array_length(selected_tokens, 1), 0);
