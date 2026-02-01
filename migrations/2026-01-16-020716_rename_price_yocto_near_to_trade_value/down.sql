-- price_yocto_near カラムを復元
ALTER TABLE trade_transactions ADD COLUMN price_yocto_near NUMERIC(39, 0) NOT NULL DEFAULT 0;
