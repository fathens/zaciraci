# NEAR Protocol RPC Endpoints 調査結果

## 調査日
2025-10-07

## 背景

### NEAR公式RPC非推奨について
- **実施期間**: 2025年6月1日から段階的に制限開始
- **対象**: `rpc.mainnet.near.org` および `pagoda.co` ドメイン配下の全エンドポイント
- **理由**:
  1. **分散化の推進**: Pagoda運営終了に伴うインフラ分散化
  2. **商用プロバイダー育成**: 無料サービスが商用プロバイダーの成長を妨害していた
  3. **濫用防止**: 大規模バックエンド利用を制限

### 現在の制限（2025年10月時点）
- **10分間のrate limit**: プロダクション利用を防ぐために実施済み
- **IPブロック**: 大規模利用はIPアドレスでブロック対象
- **8月1日以降**: FastNearが最小限のレート制限で維持（レガシーツール向け）

## 代替RPCエンドポイント一覧

### 1. Ankr ⭐ **推奨**

**エンドポイント**: `https://rpc.ankr.com/near`

**無料プラン (Freemium)**:
- **Rate Limit**: 30 requests/second (Node API)
- **月間クォータ**: 200M API Credits
- **特徴**:
  - 65+ blockchains対応
  - Discord community support
  - 安定性が高い

**有料プラン (Premium)**:
- **Rate Limit**: 1,500 requests/second
- **価格**: $10/100M API Credits (Pay-As-You-Go)
- **サブスクリプション**: $500/6B API Credits (20%ボーナス)

**評価**:
- ✅ 明確なrate limit (30 req/s)
- ✅ 十分な無料クォータ (200M/月)
- ✅ スケーラブル (有料プランで1,500 req/s)

---

### 2. dRPC

**エンドポイント**: `https://near.drpc.org`

**無料プラン**:
- **Rate Limit**:
  - 通常: 120,000 CU/分
  - 混雑時最小: 50,400 CU/分 (≈40 eth_call相当)
  - 2025年6月1日以降: 2,100 CU/秒
- **月間クォータ**: 210M Compute Units (30日周期)
- **特徴**:
  - API Key 5個まで作成可能
  - trace, debug, filter メソッドは無効
  - 分散型ネットワーク

**有料プラン**:
- **Rate Limit**: 制限なし
- **価格**: 従量課金

**評価**:
- ⚠️ 動的rate limit（混雑時に制限される）
- ✅ 月間クォータは十分 (210M CU)
- ❌ 無料プランでのdebugメソッド利用不可

---

### 3. BlockPI

**エンドポイント**: `https://near.blockpi.network/v1/rpc/public`

**無料プラン**:
- **Rate Limit**:
  - 10 RPS (requests per second)
  - 200 RUPS (RU per second)
- **月間クォータ**:
  - 一般: 100M Request Units
  - NEAR特化: 50M RUs/月
- **特徴**:
  - 毎月1日に自動付与
  - Pay-As-You-Go有効化で20 RPS/400 RUPSに増加

**有料プラン (Pay-As-You-Go with deposit)**:
- **Rate Limit**: 1,000 RPS / 40,000 RUPS
- **価格**: 従量課金

**評価**:
- ❌ 非常に低いrate limit (10 req/s)
- ⚠️ 重いメソッドでRUPSに先に到達する可能性
- ✅ 有料プランへのアップグレードが容易

---

### 4. FASTNEAR

**エンドポイント**: `https://free.rpc.fastnear.com`

**無料プラン**:
- **Rate Limit**: 不明（ドキュメント未記載）
- **特徴**:
  - 高性能を謳うサービス
  - Redis/LMDB ベース
  - 2025年8月以降も最小限の制限で維持

**評価**:
- ❓ Rate limit仕様が不明確
- ✅ 高性能・低レイテンシ
- ⚠️ 長期的な安定性が不透明

---

### 5. 1RPC

**エンドポイント**: `https://1rpc.io/near`

**無料プラン**:
- **Rate Limit**:
  - 日次クォータあり（具体的数値不明）
  - デフォルト: 00:00 UTCにリセット
- **リクエストサイズ**: 最大2MB
- **特徴**:
  - プライバシー重視（zero-trace）
  - 永続無料を宣言

**評価**:
- ❓ Rate limit仕様が不明確
- ✅ プライバシー保護機能
- ⚠️ 具体的な数値が公開されていない

---

### 6. その他の無料エンドポイント

以下は調査未完了ですが、公式ドキュメントに記載されているエンドポイント:

- **All That Node**: `https://allthatnode.com/protocol/near.dsrv`
- **fast-near web4**: `https://rpc.web4.near.page`
- **Grove**: `https://near.rpc.grove.city/v1/01fdb492`
- **Lava Network**: `https://near.lava.build:443`
- **OMNIA**: `https://endpoints.omniatech.io/v1/near/mainnet/public`

---

## 推奨事項

### 第一候補: Ankr

**理由**:
1. ✅ **明確なrate limit**: 30 req/s（予測可能）
2. ✅ **十分な無料クォータ**: 200M API Credits/月
3. ✅ **簡単なスケーリング**: 有料プランで1,500 req/s
4. ✅ **安定性**: 大手プロバイダーとして実績あり

**想定利用状況**:
- **トレード実行**: 5-10分で完了想定、rate limit内に収まる
- **record_rates**: 最適化後は15分間隔で実行
- **通常クエリ**: 30 req/sは十分

### 第二候補: dRPC

**理由**:
1. ✅ **分散型**: 単一障害点なし
2. ✅ **月間クォータ大**: 210M CU
3. ⚠️ **動的制限**: 混雑時に制限される可能性

**利用シナリオ**:
- Ankrのバックアップとして
- フェイルオーバー時の代替

### 非推奨: BlockPI (無料プラン)

**理由**:
- ❌ **rate limit低すぎ**: 10 req/s では不足
- 現在のトレード実行では数分で上限到達の可能性

---

## 実装計画

### Phase 1: 単一エンドポイント切り替え
1. Ankr RPCに切り替え
2. 動作確認とrate limit到達の監視
3. ログで実際のリクエスト数を計測

### Phase 2: マルチエンドポイント対応
1. 環境変数で複数エンドポイント設定
2. ラウンドロビンまたはweight-based負荷分散
3. rate limit時の自動フェイルオーバー

### Phase 3: 最適化
1. record_ratesの間隔調整（5分→15分）
2. プール取得の並列度制限
3. リクエストバッチング

---

## 参考資料

- [NEAR Official RPC Providers](https://docs.near.org/api/rpc/providers)
- [NEAR RPC Deprecation Announcement](https://www.near.org/blog/deprecation-of-near-org-and-pagoda-co-rpc-endpoints)
- [Pagoda Services Future](https://docs.near.org/blog/2024-08-13-pagoda-services)
- [Ankr Pricing](https://www.ankr.com/rpc/pricing/)
- [dRPC Rate Limiting](https://drpc.org/docs/howitworks/ratelimiting)
- [BlockPI Pricing](https://docs.blockpi.io/pricing/pricing-and-rate-limit)
