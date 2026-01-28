-- trade_value (price_yocto_near) カラムを削除
-- このカラムは不正確な値を記録しており、実際に使用されていなかった
ALTER TABLE trade_transactions DROP COLUMN price_yocto_near;
