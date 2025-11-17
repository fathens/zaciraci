# NEAR RPC Endpoints 選択ガイド

> 📖 詳細な調査結果は [endpoints.md](./endpoints.md) を参照してください。

## 🎯 推奨構成

### 構成案A: バランス型（推奨）✅

**6エンドポイント構成** - 安定性と多様性のバランス

```toml
[[rpc.endpoints]]
url = "https://free.rpc.fastnear.com"
weight = 35  # メイン1

[[rpc.endpoints]]
url = "https://1rpc.io/near"
weight = 30  # メイン2

[[rpc.endpoints]]
url = "https://near.lava.build"
weight = 15  # バックアップ1

[[rpc.endpoints]]
url = "https://rpc.web4.near.page"
weight = 12  # バックアップ2

[[rpc.endpoints]]
url = "https://endpoints.omniatech.io/v1/near/mainnet/public"
weight = 6   # バックアップ3

[[rpc.endpoints]]
url = "https://near-mainnet.gateway.tatum.io"
weight = 2   # 予備
```

**特徴**:
- ✅ 高品質な2つのメインエンドポイント（FASTNEAR + 1RPC）
- ✅ 3つの安定したバックアップ（Lava + web4 + Intear）
- ✅ 予備エンドポイント（Tatum）
- ✅ 合計6エンドポイントで冗長性確保

## 📊 エンドポイント評価

### ⭐⭐⭐ 最推奨（メイン使用）

#### FASTNEAR
- URL: `https://free.rpc.fastnear.com`
- Rate Limit: 制限なし（公式推奨）
- 評価: 最高品質、NEARコミュニティ推奨

#### 1RPC
- URL: `https://1rpc.io/near`
- Rate Limit: 700 req/s
- 評価: 非常に高速、安定性高い

### ⭐⭐ 推奨（バックアップ）

#### Lava
- URL: `https://near.lava.build`
- Rate Limit: 制限なし
- 評価: 新しいプロバイダー、品質良好

#### web4
- URL: `https://rpc.web4.near.page`
- Rate Limit: 不明（高め推定）
- 評価: NEAR公式関連、信頼性高い

#### Intear (Omnia)
- URL: `https://endpoints.omniatech.io/v1/near/mainnet/public`
- Rate Limit: 不明（高め推定）
- 評価: NEAR公式関連、安定

### ⭐ 使用可能（予備）

#### Tatum
- URL: `https://near-mainnet.gateway.tatum.io`
- Rate Limit: 5 req/s
- 評価: rate limit低いが使用可能

#### Shitzu
- URL: `https://rpc.shitzuapes.xyz`
- Rate Limit: 不明
- 評価: コミュニティ運営、安定性不明

### ⚠️ 非推奨

#### BlockPI
- Rate Limit: 10 req/s（低すぎる）
- 理由: トレード実行で不足

#### NEAR公式RPC
- Rate Limit: 10分間制限
- 理由: プロダクション非推奨、バックアップ用途のみ

### ❌ 使用不可

#### dRPC
- 理由: 認証必須（APIキー必要）

#### Ankr
- 理由: Premium（有料）プランのみ

## 📋 実装状況

### ✅ 完了（2025-10-16）

- **Phase 1**: 単一エンドポイント切り替え
- **Phase 2**: マルチエンドポイント対応
  - TOML設定で複数エンドポイント設定
  - weight-based負荷分散実装
  - rate limit時の自動フェイルオーバー
  - リトライループ内での動的エンドポイント切り替え
- **Phase 3**: エンドポイント検証と修正
  - dRPCが認証必須であることを発見
  - 動作確認済みエンドポイントを特定
- **Phase 4**: 大規模エンドポイント調査
  - 20プロバイダーの網羅的調査
  - 推奨構成案を作成

### 🔄 次のステップ

- **Phase 5**: 最適設定への移行
  1. config.tomlを構成案Aに更新
  2. Dockerコンテナ再起動
  3. 動作確認とログ監視

## 🔧 設定方法

### config.toml編集

```bash
# config/config.tomlを編集
vim config/config.toml

# 上記の構成案Aを[rpc.endpoints]セクションにコピー
```

### Docker再起動

```bash
cd run_local
docker compose restart backend
```

### 動作確認

```bash
# ログ確認
docker compose logs -f backend | grep endpoint

# トレード実行確認
docker compose logs -f backend | grep "trade::start"
```

## 📚 参考資料

- [詳細な調査結果](./endpoints.md) - 全20プロバイダーの詳細情報
- [マルチエンドポイント設計](./roundrobin.md) - 実装計画と仕様
- [NEAR Official RPC Providers](https://docs.near.org/api/rpc/providers)
