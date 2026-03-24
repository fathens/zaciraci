# Ref Finance コントラクト仕様

Ref Finance の DEX コントラクトの仕様をまとめる。自動トレード実装に必要な機能を中心に記述する。

## 概要

Ref Finance は NEAR 上の分散型取引所(DEX)で、Automated Market Maker (AMM) モデルを採用している。
コントラクトのソースコードは `~/devel/workspace/ref-finance/ref-contracts/ref-exchange` にある。

### コントラクト構造

- **メインコントラクト**: `Contract` (lib.rs)
- **プールタイプ**:
  - `SimplePool`: Uniswap スタイルの constant product AMM (2トークンのみ)
  - `StableSwapPool`: Curve スタイルの stable swap
  - `RatedSwapPool`: レート考慮型の stable swap
  - `DegenSwapPool`: デジェン向けスワッププール

## 1. Swap 機能

### 1.1 基本的な swap フロー

#### エントリーポイント

```rust
// lib.rs:337
pub fn swap(&mut self, actions: Vec<SwapAction>, referral_id: Option<ValidAccountId>) -> U128
```

**パラメータ**:
- `actions`: スワップアクションの配列（複数のスワップを連鎖可能）
- `referral_id`: リファラーのアカウント ID (オプション)

**戻り値**:
- 最終的に受け取ったトークン量 (`U128`)

#### SwapAction の構造

```rust
// action.rs:6-22
pub struct SwapAction {
    pub pool_id: u64,           // 使用するプールの ID
    pub token_in: AccountId,    // スワップ元トークン
    pub amount_in: Option<U128>, // スワップするトークン量（None の場合は前ステップの結果を使用）
    pub token_out: AccountId,   // スワップ先トークン
    pub min_amount_out: U128,   // 最小受取量（スリッページ保護）
}
```

### 1.2 Swap 実行フロー

1. **前処理** (lib.rs:337-348)
   - `swap` 関数が呼ばれる
   - `SwapAction` を `Action::Swap` に変換
   - `execute_actions` を呼び出す

2. **アクション実行準備** (lib.rs:320-331)
   - `execute_actions` 関数
   - 呼び出し元のアカウント情報を取得
   - リファラー情報を処理
   - `internal_execute_actions` を呼び出す

3. **アクション実行** (lib.rs:701-743)
   - `internal_execute_actions` 関数
   - frozen token のチェック
   - アクションタイプの統一性チェック
   - 各アクションを順次実行

4. **個別アクション処理** (lib.rs:752-794)
   - `internal_execute_action` 関数
   - `amount_in` の決定（指定値または前の結果）
   - アカウントから `token_in` を引き出し (`account.withdraw`)
   - プールでのスワップ実行 (`internal_pool_swap`)
   - アカウントに `token_out` を預け入れ (`account.deposit`)

5. **プールでのスワップ** (lib.rs:798-823)
   - `internal_pool_swap` 関数
   - プールの取得
   - プール種別に応じた `pool.swap` 呼び出し
   - プールの更新

### 1.3 SimplePool でのスワップ計算

#### スワップ計算ロジック

```rust
// simple_pool.rs:300-325
pub fn swap(
    &mut self,
    token_in: &AccountId,
    amount_in: Balance,
    token_out: &AccountId,
    min_amount_out: Balance,
    admin_fee: &AdminFees,
    is_view: bool
) -> Balance
```

**処理内容**:
1. トークンが異なることを確認
2. トークンのインデックスを取得
3. `internal_get_return` で受取量を計算
4. スリッページチェック (`amount_out >= min_amount_out`)
5. プール状態を更新し、手数料を分配

#### 受取量の計算

Constant Product Formula を使用:

```
x * y = k (constant)

amount_out = (amount_in * fee_factor * reserve_out) / (reserve_in * FEE_DIVISOR + amount_in * fee_factor)

where:
  fee_factor = FEE_DIVISOR - total_fee
  FEE_DIVISOR = 10000
```

### 1.4 Swap by Output

出力量を指定してスワップする機能も提供:

```rust
// lib.rs:354
pub fn swap_by_output(&mut self, actions: Vec<SwapByOutputAction>, referral_id: Option<ValidAccountId>) -> U128
```

**SwapByOutputAction の構造**:
```rust
// action.rs:24-40
pub struct SwapByOutputAction {
    pub pool_id: u64,
    pub token_in: AccountId,
    pub amount_out: Option<U128>,    // 受け取りたいトークン量
    pub token_out: AccountId,
    pub max_amount_in: Option<U128>, // 最大投入量（スリッページ保護）
}
```

### 1.5 手数料

- **total_fee**: プールごとに設定される総手数料率（basis points、FEE_DIVISOR=10000 で割る）
- **admin_fee_bps**: コントラクトレベルの管理手数料率
- 手数料は `AdminFees` 構造体で管理され、リファラー報酬も含まれる

### 1.6 注意事項

1. **アカウント登録**: スワップ実行前にアカウントが登録されている必要がある
2. **トークン登録**: 使用するトークンがアカウントに登録されているか、ホワイトリストに含まれている必要がある
3. **ストレージ**: アカウントに十分なストレージデポジットが必要
4. **Frozen トークン**: frozen 指定されたトークンはスワップできない
5. **アクションの連鎖**: 複数の `SwapAction` を渡すことでマルチホップスワップが可能
6. **アクションタイプの統一**: 1回の呼び出しで `Swap` と `SwapByOutput` を混在させることはできない

## 2. Storage Deposit 機能

Ref Finance では、ユーザーがコントラクトを利用する前にストレージデポジットを行う必要がある。
これは NEAR の Storage Staking モデルに基づいており、データ保存にかかるコストをユーザーが負担する仕組み。

### 2.1 Storage Management トレイト

NEA 標準の `StorageManagement` トレイトを実装:

```rust
// storage_impl.rs:5
impl StorageManagement for Contract
```

### 2.2 storage_deposit

#### 関数シグネチャ

```rust
// storage_impl.rs:7-42
#[payable]
fn storage_deposit(
    &mut self,
    account_id: Option<ValidAccountId>,
    registration_only: Option<bool>,
) -> StorageBalance
```

**パラメータ**:
- `account_id`: 登録するアカウント ID（None の場合は呼び出し元）
- `registration_only`: true の場合、最小限の登録のみ（トークン用の余剰ストレージなし）

**必要な NEAR**:
- `#[payable]` アノテーションにより、NEAR を添付する必要がある
- 最小金額は `storage_balance_bounds().min` で確認可能

**戻り値**:
- `StorageBalance`: 登録後のストレージバランス情報

#### 処理フロー

1. **コントラクト状態チェック**: コントラクトが Running 状態であることを確認
2. **パラメータ処理**:
   - `account_id`: 未指定の場合は呼び出し元を使用
   - `registration_only`: 未指定の場合は false
   - `amount`: 添付された NEAR の量を取得
3. **最小残高チェック**:
   - 新規登録で最小残高未満の場合はエラー
4. **アカウント処理**:
   - **registration_only = true の場合**:
     - 既に登録済み: 添付金額を全額返金
     - 新規登録: 最小金額で登録し、超過分を返金
   - **registration_only = false の場合**:
     - 全額をアカウントに登録
5. **残高情報を返す**

### 2.3 storage_withdraw

```rust
// storage_impl.rs:45-54
#[payable]
fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance
```

**パラメータ**:
- `amount`: 引き出す量（None または 0 の場合は利用可能な全額）

**必要な NEAR**:
- `assert_one_yocto()`: 1 yoctoNEAR (0.000000000000000000000001 NEAR) を添付する必要がある
  - これはセキュリティ対策で、署名付きトランザクションであることを保証

**制約**:
- 使用中のストレージ分は引き出せない
- 利用可能額を超える引き出しはエラー

### 2.4 storage_unregister

```rust
// storage_impl.rs:59-75
#[payable]
fn storage_unregister(&mut self, force: Option<bool>) -> bool
```

**パラメータ**:
- `force`: 予約されているが未使用（インターフェース互換性のため）

**必要な NEAR**:
- `assert_one_yocto()`: 1 yoctoNEAR を添付

**前提条件**:
- すべてのトークン残高が 0 であること
- legacy_tokens も空であること
- shadow_records も空であること

**処理**:
- 条件を満たす場合: アカウントを削除し、預けていた NEAR を全額返金
- 条件を満たさない場合: エラー

**戻り値**:
- `true`: 削除成功
- `false`: アカウントが存在しなかった

### 2.5 storage_balance_bounds

```rust
// storage_impl.rs:77-82
fn storage_balance_bounds(&self) -> StorageBalanceBounds
```

**戻り値**:
```rust
StorageBalanceBounds {
    min: U128,  // 最小必要ストレージ量
    max: None,  // 最大値（制限なし）
}
```

最小値の計算:
```rust
// account_deposit.rs:39-40
pub const INIT_ACCOUNT_STORAGE: StorageUsage =
    ACC_ID_AS_CLT_KEY_STORAGE + 1 + U128_STORAGE + U32_STORAGE + U32_STORAGE + U64_STORAGE;

// account_deposit.rs:202-204
pub fn min_storage_usage() -> Balance {
    INIT_ACCOUNT_STORAGE as Balance * env::storage_byte_cost()
}
```

### 2.6 storage_balance_of

```rust
// storage_impl.rs:84-93
fn storage_balance_of(&self, account_id: ValidAccountId) -> Option<StorageBalance>
```

**パラメータ**:
- `account_id`: 確認するアカウント ID

**戻り値**:
```rust
Some(StorageBalance {
    total: U128,      // 預けている総 NEAR 量
    available: U128,  // 利用可能な NEAR 量（total - 使用中）
})
```

アカウントが存在しない場合は `None` を返す。

### 2.7 Account 構造体

```rust
// account_deposit.rs:68-79
pub struct Account {
    pub near_amount: Balance,                        // 預けている NEAR 総量
    pub legacy_tokens: HashMap<AccountId, Balance>,  // 旧形式のトークン残高
    pub tokens: UnorderedMap<AccountId, Balance>,    // トークン残高
    pub storage_used: StorageUsage,                  // 使用中のストレージ量
    pub shadow_records: UnorderedMap<u64, VShadowRecord> // シャドウレコード（farming等用）
}
```

#### ストレージ使用量の計算

```rust
// account_deposit.rs:173-179
pub fn storage_usage(&self) -> Balance {
    (INIT_ACCOUNT_STORAGE +
        self.legacy_tokens.len() as u64 * (ACC_ID_AS_KEY_STORAGE + U128_STORAGE) +
        self.tokens.len() as u64 * (KEY_PREFIX_ACC + ACC_ID_AS_KEY_STORAGE + U128_STORAGE)
    ) as u128
        * env::storage_byte_cost()
}
```

#### 利用可能ストレージ

```rust
// account_deposit.rs:182-190
pub fn storage_available(&self) -> Balance {
    let locked = self.storage_usage();
    if self.near_amount > locked {
        self.near_amount - locked
    } else {
        0
    }
}
```

### 2.8 トークン登録

アカウント内で特定のトークンを使用するには、トークンを登録する必要がある:

```rust
// account_deposit.rs:318-325
#[payable]
pub fn register_tokens(&mut self, token_ids: Vec<ValidAccountId>)
```

**必要な NEAR**:
- `assert_one_yocto()`: 1 yoctoNEAR を添付

**処理**:
- 各トークンをアカウントに登録（残高 0 で初期化）
- 既に登録済みのトークンはスキップ
- ストレージが不足している場合はエラー

**注意事項**:
- トークンごとに追加のストレージが必要
- `storage_available()` が十分でない場合、登録は失敗する
- トークンがホワイトリストに含まれている場合、預け入れ時に自動登録される

### 2.9 トークン登録解除

```rust
// account_deposit.rs:330-339
#[payable]
pub fn unregister_tokens(&mut self, token_ids: Vec<ValidAccountId>)
```

**必要な NEAR**:
- `assert_one_yocto()`: 1 yoctoNEAR を添付

**前提条件**:
- 各トークンの残高が 0 であること

**処理**:
- トークン登録を解除し、ストレージを解放
- 残高が 0 でない場合はエラー

### 2.10 Storage Deposit のベストプラクティス

#### 自動トレードでの推奨フロー

1. **初回セットアップ**:
   ```
   1. storage_balance_bounds() を呼び出して最小金額を確認
   2. 使用するトークン数から必要なストレージを計算
   3. storage_deposit() を呼び出してアカウントを登録
   ```

2. **トークン追加時**:
   ```
   1. storage_balance_of() で利用可能ストレージを確認
   2. 不足している場合は storage_deposit() で追加
   3. register_tokens() でトークンを登録
   ```

3. **定期的なチェック**:
   ```
   1. storage_balance_of() で available を監視
   2. 閾値を下回った場合は storage_deposit() で補充
   ```

#### ストレージ計算の目安

- 基本アカウント: `INIT_ACCOUNT_STORAGE * storage_byte_cost()`
- トークン1つあたり: `(KEY_PREFIX_ACC + ACC_ID_AS_KEY_STORAGE + U128_STORAGE) * storage_byte_cost()`
  - 約 `(64 + 64 + 4 + 16) * storage_byte_cost()` = `148 * storage_byte_cost()`

現在の `storage_byte_cost()` は約 10^19 yoctoNEAR (0.00001 NEAR) なので:
- 基本アカウント: 約 0.001 NEAR
- トークン1つ追加ごと: 約 0.00148 NEAR

#### 注意事項

1. **最小デポジット**: `registration_only=false` で十分な量を預けることを推奨
2. **マージン**: トークン追加の可能性を考慮して、余裕を持った金額を預ける
3. **ホワイトリストトークン**: ホワイトリストに含まれるトークンは `ft_transfer_call` での預け入れ時に自動登録される
4. **残高確認**: スワップ前に `storage_balance_of()` で利用可能ストレージを確認
5. **エラーハンドリング**: ストレージ不足エラー (`ERR11_INSUFFICIENT_STORAGE`) に対応する

## 3. 自動トレード実装のための推奨フロー

### 3.1 初期セットアップ

1. `storage_balance_bounds()` で必要最小金額を確認
2. 予想されるトークン数を考慮して、十分な金額で `storage_deposit()` を呼び出す
3. 使用するトークンを `register_tokens()` で登録

### 3.2 スワップ実行前

1. `storage_balance_of()` でストレージ状況を確認
2. 必要に応じて `storage_deposit()` で追加
3. 新しいトークンを使う場合は `register_tokens()` で登録

### 3.3 スワップ実行

1. `SwapAction` を構築:
   - `pool_id`: 使用するプールの ID
   - `token_in` と `token_out`: スワップするトークンペア
   - `amount_in`: スワップする量
   - `min_amount_out`: スリッページを考慮した最小受取量
2. `swap()` を呼び出す
3. トランザクション結果を確認し、実際の受取量を記録

### 3.4 エラーハンドリング

- `ERR11_INSUFFICIENT_STORAGE`: ストレージ不足 → storage_deposit で追加
- `ERR68_SLIPPAGE`: スリッページエラー → min_amount_out を調整
- `ERR22_NOT_ENOUGH_TOKENS`: トークン残高不足 → 預け入れが必要
- `ERR21_TOKEN_NOT_REG`: トークン未登録 → register_tokens で登録

## 4. 参照ファイル

- **メインロジック**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/lib.rs`
- **アクション定義**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/action.rs`
- **ストレージ管理**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/storage_impl.rs`
- **アカウント管理**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/account_deposit.rs`
- **プール実装**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/pool.rs`
- **SimplePool**: `~/devel/workspace/ref-finance/ref-contracts/ref-exchange/src/simple_pool.rs`

## 5. まとめ

Ref Finance コントラクトは以下の特徴を持つ:

1. **複数のプールタイプ**: SimplePool, StableSwapPool, RatedSwapPool, DegenSwapPool
2. **マルチホップスワップ**: 複数の SwapAction を連鎖可能
3. **ストレージステーキング**: NEAR のストレージモデルに基づいた預け入れが必要
4. **トークン登録**: 使用するトークンは事前に登録が必要
5. **スリッページ保護**: min_amount_out / max_amount_in で保護
6. **手数料**: プールごとの total_fee と全体の admin_fee_bps

自動トレード実装では、ストレージ管理とトークン登録を適切に行い、スリッページエラーに対応できるロジックが重要。
