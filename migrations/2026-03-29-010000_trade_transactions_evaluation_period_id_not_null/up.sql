-- evaluation_period_id は 2025-09-30 に ALTER TABLE ADD COLUMN で追加（NOT NULL なし）。
-- 現在のプロダクションコードは常に evaluation_period_id を設定するため、NULL レコードは
-- マイグレーション前の古いデータにのみ存在しうる。
DELETE FROM trade_transactions WHERE evaluation_period_id IS NULL;

ALTER TABLE trade_transactions ALTER COLUMN evaluation_period_id SET NOT NULL;
