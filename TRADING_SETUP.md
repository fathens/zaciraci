# トレーディングシステム セットアップガイド

## 🎉 Phase 2 実装完了

自動トレードシステムは**完全実装済み**で、実際のトレードを実行する準備が整っています。

## 必要な環境変数

### 必須設定

```bash
# ネットワーク設定（mainnet/testnetの切り替え）
USE_MAINNET=false  # trueでmainnet、falseでtestnet

# Chronos API設定（価格予測）
CHRONOS_URL=http://localhost:8000

# Backend API設定
BACKEND_URL=http://localhost:3000

# データベース設定
PG_DSN=postgres://username:password@localhost:5432/zaciraci
PG_POOL_SIZE=16  # データベース接続プールサイズ（オプション）

# NEAR ウォレット設定
ROOT_ACCOUNT_ID=your-account.near
ROOT_MNEMONIC="your twelve word mnemonic phrase here"
ROOT_HDPATH="m/44'/397'/0'"
```

### トレード設定

```bash
# 初期投資額（NEAR単位）
TRADE_INITIAL_INVESTMENT=100

# 選定するトップボラティリティトークン数
TRADE_TOP_TOKENS=10

# 価格記録の初期値（ミリNEAR単位）
CRON_RECORD_RATES_INITIAL_VALUE=100

# トレード実行スケジュール（cron形式）
TRADE_CRON_SCHEDULE="0 0 0 * * *"  # デフォルト: 毎日午前0時
# 例:
# "0 0 * * * *"     - 毎時実行
# "0 0 */12 * * *"  - 12時間ごと
# "0 0 0 * * MON"   - 毎週月曜日

# ハーベスト設定
HARVEST_ACCOUNT_ID=harvest-account.near  # 利益送金先
HARVEST_MIN_AMOUNT=10                    # 最小ハーベスト額（NEAR）
HARVEST_RESERVE_AMOUNT=1                 # アカウント残高保護（NEAR）
```

## トレード実行の仕組み

### 自動実行スケジュール

- **価格記録**: 5分ごと（`0 */5 * * * *`）
- **トレード実行**: デフォルト毎日午前0時（環境変数`TRADE_CRON_SCHEDULE`で設定可能）

### トレード実行フロー

1. **資金準備**: NEAR → wrap.near 変換
2. **トークン選定**: ボラティリティTop10選択
3. **ポートフォリオ最適化**: Portfolioアルゴリズムで最適配分計算
4. **取引実行**: TradingActionに基づくswap実行
5. **ハーベスト**: 200%利益時に10%を収穫

## 動作確認

### ログ確認

```bash
# トレード実行ログ
tail -f logs/trade.log | grep "trade::start"

# ハーベスト実行ログ
tail -f logs/trade.log | grep "check_and_harvest"

# スワップ実行ログ
tail -f logs/trade.log | grep "execute_direct_swap"
```

### データベース確認

```sql
-- 最新の取引を確認
SELECT * FROM trade_transactions
ORDER BY executed_at DESC
LIMIT 10;

-- バッチごとの取引サマリー
SELECT
    batch_id,
    COUNT(*) as trade_count,
    SUM(CAST(amount_in AS DECIMAL)) as total_in,
    MIN(executed_at) as start_time,
    MAX(executed_at) as end_time
FROM trade_transactions
GROUP BY batch_id
ORDER BY start_time DESC;

-- ポートフォリオ価値の推移
SELECT
    batch_id,
    executed_at,
    SUM(CAST(amount_in AS DECIMAL) * CAST(price_yoctonear AS DECIMAL)) as portfolio_value
FROM trade_transactions
GROUP BY batch_id, executed_at
ORDER BY executed_at DESC;
```

## セキュリティ注意事項

⚠️ **重要**: 本番環境で使用する前に必ず以下を確認してください：

1. **テストネットで十分なテスト**を実施
2. **小額から開始**して動作を確認
3. **秘密鍵の安全な管理**（環境変数または秘密管理システムを使用）
4. **ハーベストアカウントの設定**を正しく行う
5. **データベースバックアップ**を定期的に実施

## トラブルシューティング

### トレードが実行されない場合

1. ウォレット残高を確認
2. Chronos APIの接続を確認
3. データベース接続を確認
4. ログでエラーを確認

### ハーベストが実行されない場合

1. ポートフォリオ価値が200%に達しているか確認
2. HARVEST_MIN_AMOUNTの設定を確認
3. 24時間の時間間隔制限を確認

## パフォーマンス監視

システムは以下の指標を自動記録します：

- 各取引のトランザクションハッシュ
- 取引ごとの入出力額と価格
- バッチ単位での取引グループ化
- ポートフォリオ価値の時系列データ
- ハーベスト実行履歴

## 次のステップ

1. **テストネットでの動作確認** ✅
2. **小額での本番テスト**
3. **パフォーマンス分析とアルゴリズム調整**
4. **追加の取引戦略の実装**（Momentum、TrendFollowingなど）

---

🚀 **システムは完全に実装済みで、実際のトレードを開始する準備ができています！**