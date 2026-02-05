-- swap_path カラムを追加（マルチホップの全プール情報を JSON で保存）
-- スリッページ補正に必要なプールサイズ情報を記録
ALTER TABLE token_rates ADD COLUMN swap_path JSONB;
