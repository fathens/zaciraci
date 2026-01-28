# ログレベル方針

## 最終更新日
2026-01-27

## 概要

本プロジェクトでは slog + slog-envlogger を使用し、環境変数 `RUST_LOG` でログレベルを制御する。

### 環境別設定

| 環境 | RUST_LOG | 備考 |
|------|----------|------|
| ローカル開発 | `trace` | 全ログ出力 |
| 本番通常 | `info` | `release_max_level_info` で compile-time 制限 |
| 本番調査 | `debug` | 問題調査時に一時的に有効化 |

## ログレベル判断基準

| レベル | 基準 | 本番での利用シーン |
|--------|------|-----|
| **ERROR** | 要調査の障害 | 常時出力 |
| **WARN** | リカバリ可能な問題、リトライ上限到達 | 常時出力 |
| **INFO** | ビジネスイベント・結果・重要な状態変化 | 常時出力 |
| **DEBUG** | 処理フロー把握。フェーズ区切り、意思決定、アクション実行 | 問題調査時 |
| **TRACE** | ループ内の個別アイテム、関数入口/出口、中間計算値 | 深い調査時のみ |

### INFO にすべきもの

- サイクルの開始・完了
- 最適化結果のサマリー
- ビジネスアクションの成功・失敗
- 評価期間の遷移（作成、終了）
- 清算の開始・完了
- 取引不可能な状態（資金なし、トークン選定ゼロ）

### DEBUG にすべきもの

- フェーズの区切り（Phase 1 開始、Phase 2 開始）
- 個別アクションの実行と完了（sell, switch, add, reduce）
- フィルタリングパイプラインの中間結果
- 資金源の意思決定
- RPC レベルのエラー詳細、リトライ
- DB 更新確認

### TRACE にすべきもの

- ループ内の個別トークン処理
- 個別トークンの予測値、ウェイト
- 関数の入口/出口
- 中間計算値、データダンプ
- RPC 呼び出しの成功応答
- エンドポイント選択

## RPC ログの層分け

```
ビジネス層 (near_client.rs)
  INFO  : "transferring", "executing contract", "broadcasted"
  DEBUG : "asking for tx status", "Transaction status"

Tx監視層 (sent_tx.rs)
  INFO  : "completed", "failed"
  WARN  : "polling timeout"
  DEBUG : "starting polling"

トランスポート層 (rpc.rs)
  INFO  : "calling" (outer loop - エンドポイント選択後)
  DEBUG : "calling" (inner - 個別リクエスト), エラー詳細, リトライ
  TRACE : "success"
  WARN  : "global retry limit reached"

エンドポイント層 (endpoint_pool.rs)
  TRACE : "endpoint selected"
  WARN  : "no available", "marked as failed"
```

## ポートフォリオ処理の各レベルでの見え方

### INFO (本番)

```
starting portfolio-based trading strategy
evaluation period status; period_id=abc, is_new_period=true
portfolio optimization completed; action_count=5
action executed successfully; action=Sell{...}
rebalance completed; phase2_success=3, phase2_failed=0
trades executed; success=5, failed=0
success
```

### DEBUG (本番調査時に追加表示)

```
Using liquidated balance for new period; available_funds=1000
selected tokens from prediction service; count=20
tokens after buyability filtering; original=20, buyable=15
tokens after liquidity filtering; original=20, filtered=8
ensuring REF Finance storage setup; token_count=8
depositing initial investment; amount=1000
executing sell; from=token_a, to=token_b
sell completed; from=token_a, to=token_b
executing rebalance; weights={...}
Phase 1: executing sell operations; count=3
Phase 1 completed; available_wrap_near=500
Phase 2: executing buy operations; count=2
position added; token=token_c, weight=0.3
```

### TRACE (深い調査時に追加表示)

```
token prediction; token=token_a, current=1.2, predicted=1.5, return=25%
optimal weight; token=token_a, weight=0.3, percentage=30%
loaded existing position; token=token_a, amount=100
selling token; token=token_a, amount=50
buying token; token=token_b, amount=30
purchase completed; token=token_b
liquidating token; token=token_a
```

## 新規ログ追加時のガイドライン

1. **まず TRACE を検討する** — ループ内、中間値、関数入口は TRACE
2. **フェーズ区切りなら DEBUG** — 複数ステップの処理で次のフェーズに移る時
3. **ビジネス結果なら INFO** — ユーザが気にする結果、状態遷移
4. **リカバリ可能な問題は WARN** — リトライ上限、フォールバック発動
5. **要調査の障害は ERROR** — 継続不能、データ不整合
