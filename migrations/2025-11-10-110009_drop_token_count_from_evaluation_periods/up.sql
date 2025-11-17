-- カラム削除: token_count は selected_tokens の長さから計算可能なため冗長
ALTER TABLE evaluation_periods DROP COLUMN token_count;
