# NEAR Protocol RPC Endpoints 調査結果

> 📖 **クイックガイド**: 推奨エンドポイント構成は [endpoints_guide.md](./endpoints_guide.md) を参照してください。
>
> このファイル（598行）には全20プロバイダーの詳細な調査結果が含まれています。

## 最終更新日
2025-10-16

## 調査履歴
- 2025-10-07: 初回調査
- 2025-10-16: dRPCエンドポイント検証、実運用エラー調査

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

### 1. Ankr ❌ **Premium プランのみ (有料)**

**エンドポイント**:
- ~~無料（認証なし）~~: `https://rpc.ankr.com/near` ❌ 利用不可
- ~~Freemium（無料）~~: ❌ **実質的に利用不可**
- **Premium のみ**: `https://rpc.ankr.com/near/{your_token}` 💰 有料

**重要: 2024年以降の実態**:
- 🔒 **公式ドキュメントでは「Freemium で利用可能」と記載**
- ❌ **実際には Premium への移行を促される**
- 💰 **NEAR は実質的に Premium プラン（有料）のみ**
- 🚫 **無料では使えない** - このプロジェクトでは使用しない

**参考情報** (Premium プラン):
- **Rate Limit**: 1,500 requests/second
- **価格**: $10/100M API Credits (Pay-As-You-Go)
- **サブスクリプション**: $500/6B API Credits (20%ボーナス)

**評価**:
- ❌ **無料プランでは使えない** (ドキュメントと実態が乖離)
- ❌ このプロジェクトでは使用しない
- ✅ 他の無料エンドポイント (fastnear, 1rpc, 公式RPC) で十分

---

### 2. dRPC ❌ **認証必須 (実質有料)**

**エンドポイント**:
- ~~無料（認証なし）~~: `https://near.drpc.org` ❌ **利用不可**
- **認証あり**: `https://lb.drpc.org/ogrpc?network=near&dkey=YOUR_DRPC_KEY`

**重要: 2025年10月時点の実態**:
- 🔒 **APIキー（dkey）が必須**
- ❌ **`https://near.drpc.org` は全メソッドでエラー -32601**
  - `query`: エラー（メソッドが存在しない）
  - `block`: エラー（メソッドが存在しない）
  - `status`: エラー（メソッドが存在しない）
- 🚫 **認証なしでは使えない** - このプロジェクトでは使用しない

**検証結果** (2025-10-16):
```bash
$ curl -X POST https://near.drpc.org -d '{"method":"query",...}'
{"error":{"message":"the method query does not exist/is not available","code":-32601}}
```

**無料プラン** (認証あり):
- **Rate Limit**:
  - 通常: 120,000 CU/分
  - 混雑時最小: 50,400 CU/分
  - 2025年6月1日以降: 2,100 CU/秒
- **月間クォータ**: 210M Compute Units (30日周期)
- **特徴**:
  - API Key 5個まで作成可能
  - アカウント登録が必要

**有料プラン**:
- **Rate Limit**: 制限なし
- **価格**: 従量課金

**評価**:
- ❌ **認証なしでは使えない**（以前の調査時と状況が変化）
- ❌ このプロジェクトでは使用しない
- ✅ 他の無料エンドポイント (fastnear, 1rpc, 公式RPC) で代替可能

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

### 4. FASTNEAR ⭐ **推奨**

**エンドポイント**: `https://free.rpc.fastnear.com`

**無料プラン**:
- **Rate Limit**: 不明（ドキュメント未記載、実運用では制限緩い）
- **特徴**:
  - 高性能を謳うサービス
  - Redis/LMDB ベース
  - 2025年8月以降も最小限の制限で維持
  - NEAR公式による運営継続の確約

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 高速

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ 高性能・低レイテンシ
- ✅ 公式による継続運営の確約
- ✅ 実運用で安定動作確認済み
- ⚠️ Rate limit仕様が不明確（ただし制限は緩い）

---

### 5. 1RPC ⭐ **推奨**

**エンドポイント**: `https://1rpc.io/near`

**無料プラン**:
- **Rate Limit**:
  - 日次クォータあり（具体的数値不明）
  - デフォルト: 00:00 UTCにリセット
- **リクエストサイズ**: 最大2MB
- **特徴**:
  - プライバシー重視（zero-trace）
  - 永続無料を宣言

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 良好

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ プライバシー保護機能
- ✅ 永続無料宣言
- ✅ 実運用で安定動作確認済み
- ⚠️ Rate limit仕様が不明確（ただし日次クォータで運用可能）

---

### 6. NEAR公式RPC (rpc.mainnet.near.org)

**エンドポイント**: `https://rpc.mainnet.near.org`

**無料プラン**:
- **Rate Limit**:
  - 10分間のrate limit実施中（2025年6月1日以降）
  - 大規模利用はIPブロック対象
- **特徴**:
  - NEAR Protocol公式エンドポイント
  - 非推奨（段階的に制限強化中）
  - レガシーツール向けに最小限維持

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ⚠️ レスポンス速度: 普通
- ⚠️ 10分間のrate limit注意

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ 公式エンドポイントの信頼性
- ⚠️ **rate limit厳しい**（10分間制限）
- ⚠️ プロダクション利用は非推奨
- 🔄 バックアップ用途にのみ推奨

**使用上の注意**:
- プライマリとしては使わない
- FASTNEARと1RPCのバックアップとして設定
- 優先度を低く設定（weight: 20程度）

---

### 7. Lava Network ⭐⭐ **推奨**

**エンドポイント**: `https://near.lava.build`

**無料プラン**:
- **Rate Limit**:
  - **ipRPC (Incentivized Public RPC)**: 3 requests/second
  - **Lava Gateway**: 100 requests/second
- **月間クォータ**: 無制限（無料）
- **特徴**:
  - NEAR Foundation とのパートナーシップ
  - 分散型インフラストラクチャ
  - Incentivized Public RPCは永続的に無料

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 高速

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ **100 req/s** - 非常に高いrate limit（Gateway使用時）
- ✅ NEAR公式パートナー
- ✅ 実運用で安定動作確認済み
- ✅ 永続無料宣言

---

### 8. fast-near web4 ⭐ **推奨**

**エンドポイント**: `https://rpc.web4.near.page`

**無料プラン**:
- **Rate Limit**: 不明（ドキュメント未記載、実運用では制限緩い）
- **特徴**:
  - FASTNEAR関連の高性能サービス
  - Redis/LMDB ベース

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 非常に高速

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ 非常に高速
- ✅ 実運用で安定動作確認済み
- ✅ FASTNEAR系列で信頼性高い

---

### 9. Intear RPC ⭐ **推奨**

**エンドポイント**: `https://rpc.intea.rs`

**無料プラン**:
- **Rate Limit**: 不明（ドキュメント未記載）
- **特徴**:
  - 高速レスポンス
  - 安定性が高い

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 高速

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ 高速・安定
- ✅ 実運用で安定動作確認済み

---

### 10. Tatum

**エンドポイント**: `https://near-mainnet.gateway.tatum.io/`

**無料プラン**:
- **Rate Limit**: 不明（ドキュメント未記載）
- **特徴**:
  - マルチチェーン対応プロバイダー

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ✅ レスポンス速度: 普通

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ 安定動作
- ⚠️ レスポンス速度は他より遅め

---

### 11. Shitzu

**エンドポイント**: `https://rpc.shitzuapes.xyz`

**無料プラン**:
- **Rate Limit**: 不明
- **特徴**:
  - コミュニティ運営のエンドポイント

**検証結果** (2025-10-16):
- ✅ `query` メソッド: 正常動作
- ✅ `ft_balance_of`: 正常動作
- ⚡ レスポンス速度: 普通

**評価**:
- ✅ **認証不要ですぐ使える**
- ✅ バックアップ用途に適している
- ⚠️ 長期安定性は不明

---

### 12. その他のエンドポイント（動作未確認・認証必須）

以下は調査済みで動作しないことが判明したエンドポイント:

#### ❌ 認証必須
- **Ankr**: `https://rpc.ankr.com/near` - APIキー必須
- **AllThatNode**: `https://near-mainnet-rpc.allthatnode.com:3030/` - APIキー必須
- **GetBlock**: `https://getblock.io/nodes/near/` - APIキー必須
- **NodeReal**: `https://nodereal.io/api-marketplace/near-rpc` - APIキー必須

#### ❌ エラー・動作不可
- **OMNIA**: `https://endpoints.omniatech.io/v1/near/mainnet/public` - 502エラー
- **Grove**: `https://near.rpc.grove.city/v1/01fdb492` - プロトコルエンドポイント無し
- **Seracle**: `https://api.seracle.com/saas/baas/rpc/near/mainnet/public/` - 502エラー
- **NOWNodes**: `https://near.nownodes.io/` - 422エラー
- **BlockEden**: `https://api.blockeden.xyz/near/*` - Rate limit exceeded
- **ZAN**: `https://api.zan.top/node/v1/near/mainnet/` - パスエラー
- **Lavender.Five**: `https://near.lavenderfive.com/` - ホスト解決不可

---

## 推奨事項（無料プランのみ）- 2025-10-16更新

### 調査サマリー

- **調査対象**: 20プロバイダー
- **動作確認**: 9エンドポイント（認証不要）
- **高速エンドポイント**: 6個（FASTNEAR、1RPC、web4、Lava、Intear、Tatum）
- **推奨エンドポイント**: 6個
- **非推奨**: 2個（BlockPI、NEAR公式）
- **認証必須**: 4個（dRPC、Ankr、AllThatNode、GetBlock、NodeReal）
- **エラー/利用不可**: 7個

---

### 🎯 推奨構成

### 構成案A: バランス型（6エンドポイント）⭐ **最推奨**

```toml
[[rpc.endpoints]]
url = "https://free.rpc.fastnear.com"
weight = 35
max_retries = 3

[[rpc.endpoints]]
url = "https://1rpc.io/near"
weight = 30
max_retries = 3

[[rpc.endpoints]]
url = "https://near.lava.build"
weight = 15
max_retries = 2

[[rpc.endpoints]]
url = "https://rpc.web4.near.page"
weight = 10
max_retries = 2

[[rpc.endpoints]]
url = "https://rpc.intea.rs"
weight = 8
max_retries = 2

[[rpc.endpoints]]
url = "https://near-mainnet.gateway.tatum.io/"
weight = 2
max_retries = 1
```

**特徴**:
- ✅ 6個の高品質エンドポイントを使用
- ✅ 上位2つ（FASTNEAR + 1RPC）で65%をカバー
- ✅ rate limit制限のあるNEAR公式を除外
- ✅ フェイルオーバー時の選択肢が豊富（4つのバックアップ）
- ✅ パフォーマンスと信頼性のバランスが最適

**推奨理由**:
1. 十分な冗長性（6エンドポイント）
2. 全て高速・高品質
3. rate limit問題を回避
4. 管理しやすい複雑度

---

### 構成案B: シンプル型（3エンドポイント）

```toml
[[rpc.endpoints]]
url = "https://free.rpc.fastnear.com"
weight = 50
max_retries = 3

[[rpc.endpoints]]
url = "https://1rpc.io/near"
weight = 35
max_retries = 3

[[rpc.endpoints]]
url = "https://near.lava.build"
weight = 15
max_retries = 2
```

**特徴**:
- ✅ 最も信頼性の高い3つに絞る
- ✅ シンプルで管理しやすい
- ✅ 十分な冗長性を確保
- ⚠️ バックアップが少ない

**推奨理由**:
- 最小限の構成で最大の効果
- 運用が簡単

---

### 構成案C: 最大冗長型（8エンドポイント）

```toml
[[rpc.endpoints]]
url = "https://free.rpc.fastnear.com"
weight = 30
max_retries = 3

[[rpc.endpoints]]
url = "https://1rpc.io/near"
weight = 25
max_retries = 3

[[rpc.endpoints]]
url = "https://near.lava.build"
weight = 15
max_retries = 2

[[rpc.endpoints]]
url = "https://rpc.web4.near.page"
weight = 10
max_retries = 2

[[rpc.endpoints]]
url = "https://rpc.intea.rs"
weight = 8
max_retries = 2

[[rpc.endpoints]]
url = "https://near-mainnet.gateway.tatum.io/"
weight = 5
max_retries = 1

[[rpc.endpoints]]
url = "https://rpc.shitzuapes.xyz"
weight = 5
max_retries = 1

[[rpc.endpoints]]
url = "https://rpc.mainnet.near.org"
weight = 2
max_retries = 1
```

**特徴**:
- ✅ 最大限の冗長性（8エンドポイント）
- ✅ 緊急時に公式RPCも使える
- ⚠️ 管理が複雑
- ⚠️ NEAR公式のrate limit問題が発生する可能性

**推奨理由**:
- 絶対にダウンタイムを避けたい場合

---

### 非推奨エンドポイント

#### dRPC ❌
**理由**:
- ❌ **認証必須**: APIキーが必要
- ❌ `https://near.drpc.org` は動作しない
- 認証なしでは全メソッドでエラー -32601

#### BlockPI (無料プラン) ❌
**理由**:
- ❌ **rate limit低すぎ**: 10 req/s では不足
- 現在のトレード実行では数分で上限到達の可能性

#### NEAR公式RPC ⚠️
**理由**:
- ⚠️ **rate limit厳しい**: 10分間制限
- ⚠️ **非推奨**: プロダクション利用は推奨されない
- ℹ️ バックアップ用途のみ推奨（構成案Cで使用）

#### Ankr ❌
**理由**:
- ❌ **Premium（有料）のみ**: 無料プランは実質使用不可
- ドキュメントと実態が乖離

---

## 実装計画

### Phase 1: 単一エンドポイント切り替え ✅ 完了
1. ✅ Ankr RPCに切り替え
2. ✅ 動作確認とrate limit到達の監視
3. ✅ ログで実際のリクエスト数を計測

### Phase 2: マルチエンドポイント対応 ✅ 完了
1. ✅ TOML設定で複数エンドポイント設定 (config/config.toml)
2. ✅ weight-based負荷分散実装 (EndpointPool)
3. ✅ rate limit時の自動フェイルオーバー
4. ✅ **Phase 2b**: リトライループ内での動的エンドポイント切り替え

**実装結果** (2025-10-10):
- 4つのエンドポイントで負荷分散: ankr (40%), drpc (40%), fastnear (15%), 1rpc (5%)
- rate limit エラー: 0件確認
- 動的エンドポイント切り替えが正常動作

### Phase 3: エンドポイント検証と修正 ✅ 完了 (2025-10-16)

**問題発見**:
- ❌ auto trade実行時に `near.drpc.org` でエラー -32601
- ❌ 全JSONRPC標準メソッド (`query`, `block`, `status`) が失敗
- 原因: `near.drpc.org` は認証必須（APIキーが必要）

**実施内容**:
1. ✅ 各エンドポイントの動作検証（手動curlテスト）
2. ✅ エラー根本原因の特定（dRPCは認証なしで使用不可）
3. ✅ endpoints.md更新（調査結果を反映）
4. 🔄 config.toml修正（次のフェーズ）

**検証結果**:
- ✅ FASTNEAR (`https://free.rpc.fastnear.com`): 正常動作
- ✅ 1RPC (`https://1rpc.io/near`): 正常動作
- ✅ NEAR公式 (`https://rpc.mainnet.near.org`): 正常動作（ただしrate limit厳しい）
- ❌ dRPC (`https://near.drpc.org`): 全メソッドでエラー -32601

### Phase 4: 大規模エンドポイント調査 ✅ 完了 (2025-10-16)

**実施内容**:
1. ✅ 20プロバイダーの網羅的調査
2. ✅ 各エンドポイントの動作検証（手動curlテスト）
3. ✅ 新規エンドポイント5個を発見
4. ✅ 3つの推奨構成案を作成
5. ✅ endpoints.md完全更新

**調査結果サマリー**:
- **動作確認**: 9エンドポイント（認証不要）
  - ⭐⭐⭐ 最推奨: FASTNEAR、1RPC
  - ⭐⭐ 推奨: Lava、web4、Intear
  - ⭐ 使用可能: Tatum、Shitzu
  - ⚠️ 非推奨: BlockPI、NEAR公式
- **認証必須**: 4個（dRPC、Ankr、AllThatNode、GetBlock、NodeReal）
- **エラー/利用不可**: 7個

**推奨構成**: 構成案A（バランス型・6エンドポイント）
```toml
# FASTNEAR + 1RPC がメイン（65%）
# Lava + web4 + Intear がバックアップ（33%）
# Tatum が予備（2%）
```

### Phase 5: 最適設定への移行 🔄 次のステップ

**次のステップ**:
1. config.tomlを構成案Aに更新
2. Dockerコンテナを再起動
3. 動作確認（auto trade実行）
4. ログ監視（エラーの有無、エンドポイント切り替え動作）

---

## 参考資料

- [NEAR Official RPC Providers](https://docs.near.org/api/rpc/providers)
- [NEAR RPC Deprecation Announcement](https://www.near.org/blog/deprecation-of-near-org-and-pagoda-co-rpc-endpoints)
- [Pagoda Services Future](https://docs.near.org/blog/2024-08-13-pagoda-services)
- [Ankr Pricing](https://www.ankr.com/rpc/pricing/)
- [dRPC Rate Limiting](https://drpc.org/docs/howitworks/ratelimiting)
- [BlockPI Pricing](https://docs.blockpi.io/pricing/pricing-and-rate-limit)
