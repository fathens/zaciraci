# 自動トレード記録システム

## 概要
バックエンドの自動トレード機能において、成功した取引のみをシンプルにデータベースに記録する。

## 記録対象データ
- トランザクションID（primary key）
- 変換元トークン数量
- 変換先トークン数量
- yoctoNEAR建ての価格
- 取引時刻

## データベース設計

### trade_transactions テーブル
成功したトランザクションのみを記録するシンプルなテーブル

```sql
CREATE TABLE trade_transactions (
    tx_id VARCHAR PRIMARY KEY,                  -- NEARトランザクションハッシュ（一意）
    trade_batch_id VARCHAR NOT NULL,           -- 同時実行された取引群のID（UUID）
    from_token VARCHAR NOT NULL,               -- 変換元トークン識別子
    from_amount NUMERIC(39, 0) NOT NULL,       -- 変換元トークン数量
    to_token VARCHAR NOT NULL,                 -- 変換先トークン識別子
    to_amount NUMERIC(39, 0) NOT NULL,         -- 変換先トークン数量
    price_yocto_near NUMERIC(39, 0) NOT NULL,  -- yoctoNEAR建て価格
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

## インデックス設計

```sql
-- 検索性能向上のためのインデックス
CREATE INDEX idx_trade_transactions_timestamp ON trade_transactions(timestamp);
CREATE INDEX idx_trade_transactions_batch_id ON trade_transactions(trade_batch_id);
CREATE INDEX idx_trade_transactions_tokens ON trade_transactions(from_token, to_token);
```

## 記録フロー

### バッチ単位での取引記録
```rust
// 同時実行される複数取引のバッチID生成
let batch_id = Uuid::new_v4().to_string();

// 各トランザクションを同じbatch_idで記録
for transaction_result in completed_transactions {
    let trade_transaction = TradeTransaction {
        tx_id: transaction_result.hash,           // NEARトランザクションハッシュ
        trade_batch_id: batch_id.clone(),        // UUID
        from_token: "wrap.near".to_string(),
        from_amount: 1000000000000000000000000,  // 1 NEAR in yoctoNEAR
        to_token: "akaia.tkn.near".to_string(),
        to_amount: 50000000000000000000000,      // 受信したトークン数
        price_yocto_near: 20000000000000000000,  // yoctoNEAR建て単価
        timestamp: Utc::now().naive_utc(),
    };
}
```

## 活用方法

### 分析クエリ例
```sql
-- バッチ別の総残高確認
SELECT
    trade_batch_id,
    SUM(price_yocto_near) as total_portfolio_value,
    COUNT(*) as transaction_count,
    MIN(timestamp) as batch_timestamp
FROM trade_transactions
GROUP BY trade_batch_id
ORDER BY batch_timestamp DESC
LIMIT 10;

-- 時系列での残高推移
SELECT
    trade_batch_id,
    SUM(price_yocto_near) as portfolio_value,
    MIN(timestamp) as timestamp
FROM trade_transactions
GROUP BY trade_batch_id
ORDER BY timestamp ASC;

-- 特定バッチの取引詳細
SELECT
    tx_id,
    from_token,
    from_amount,
    to_token,
    to_amount,
    price_yocto_near
FROM trade_transactions
WHERE trade_batch_id = 'specific-uuid-here'
ORDER BY timestamp;
```

## 実装優先順位

### Phase 1: 基本記録機能
- [x] データベーススキーマ設計
- [ ] Diesel migration ファイル作成
- [ ] Rust struct 定義
- [ ] 基本的な記録機能実装

### Phase 2: 取引連携
- [ ] 実際の取引実行時の記録
- [ ] エラーハンドリング