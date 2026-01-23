# 自動トレードシステム 未実装タスク

## 🔥 優先度: 中

### 1. マルチエンドポイントRPC Phase 3-4
- **Phase 3**: エンドポイント選択アルゴリズムの改善
  - レイテンシベースのエンドポイント選択
  - 重み付けラウンドロビン
- **Phase 4**: 監視とメトリクス
  - エンドポイント別の成功率・レイテンシ収集
  - ダッシュボード表示

詳細は `diagram/roundrobin.md` および `diagram/endpoints.md` を参照。

## 🔥 優先度: 低

### 2. 追加の取引戦略の実装
- **Momentum戦略**: モメンタムベースの取引
- **TrendFollowing戦略**: トレンドフォロー戦略

詳細は `diagram/trade/algorithm_momentum.md` および `diagram/trade/algorithm_trend_following.md` を参照。

### 3. パフォーマンス分析とアルゴリズム調整
- バックテスト結果の分析
- アルゴリズムパラメータの最適化
- リスク管理機能の強化

## 🔄 次回 cron 実行待ち

- Storage Deposit 一括実行の動作確認
- rate limit エラーの解消確認
