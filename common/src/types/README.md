# 型設計ドキュメント

このドキュメントでは `common/src/types/` で定義される型の設計と関係を説明する。

## 背景

NEAR プロトコルでは：
- 1 NEAR = 10^24 yoctoNEAR
- 各トークンは独自の `decimals` を持つ（例: wNEAR=24, USDT=6, BRRR=18）

異なる `decimals` を持つトークン間で価格やレートを比較する際、単位の変換ミスが発生しやすい。
型システムでこれを防ぐ。

## 型の分類

### NEAR 専用型（decimals=24 固定）

| 型 | 内部表現 | 意味 | 用途 |
|---|---|---|---|
| `YoctoAmount` | BigDecimal | yoctoNEAR 量 | NEAR 残高、スワップ量 |
| `NearAmount` | BigDecimal | NEAR 量 | 表示用 |
| `YoctoValue` | BigDecimal | yoctoNEAR 金額 | ポートフォリオ評価（精密） |
| `NearValue` | BigDecimal | NEAR 金額 | ポートフォリオ評価 |

### 任意トークン対応型（decimals 可変）

| 型 | 内部表現 | 意味 | 用途 |
|---|---|---|---|
| `TokenAmount` | BigDecimal + decimals | トークン量 | 保有量 |
| `ExchangeRate` | BigDecimal + decimals | tokens_smallest/NEAR | DB保存、計算 |
| `TokenPrice` | BigDecimal | NEAR/token | 比較、リターン計算 |

### f64 版（シミュレーション用）

| 型 | 内部表現 | 意味 |
|---|---|---|
| `TokenPriceF64` | f64 | 価格 NEAR/token（高速計算用） |
| `TokenAmountF64` | f64 | トークン量 |
| `YoctoValueF64` | f64 | yoctoNEAR 金額 |
| `NearValueF64` | f64 | NEAR 金額 |

## 型の詳細

### ExchangeRate

DB に保存される交換レート。`token_rates` テーブルの `rate` カラムに対応。

```rust
pub struct ExchangeRate {
    /// 1 NEAR あたりの smallest_unit 数
    /// 例: USDT (decimals=6) で 1 NEAR = 5 USDT なら 5_000_000
    raw_rate: BigDecimal,

    /// トークンの decimals
    decimals: u8,
}
```

**重要**: `raw_rate` は「価格」ではなく「レート」。価格の逆数。

- `raw_rate` が大きい = 1 NEAR で多くのトークンが買える = トークンが安い
- `raw_rate` が小さい = 1 NEAR で少ないトークンしか買えない = トークンが高い

### TokenPrice

人間が理解しやすい「価格」。1トークンあたりの NEAR 価値。

```rust
pub struct TokenPrice(BigDecimal);
```

- decimals を考慮済み（whole token 単位）
- `TokenPrice` が大きい = トークンが高い
- `TokenPrice` が小さい = トークンが安い

### TokenAmount

任意トークンの量。decimals 情報を保持。

```rust
pub struct TokenAmount {
    /// 最小単位での量
    smallest_units: BigDecimal,

    /// トークンの decimals
    decimals: u8,
}
```

## 型間の変換

```
ExchangeRate { raw_rate, decimals }
      │
      └── to_price() ──► TokenPrice
          (decimals を考慮して変換)
```

変換式:
```
TokenPrice = 10^decimals / raw_rate
```

例（USDT, decimals=6, 1 NEAR = 5 USDT）:
```
raw_rate = 5_000_000 (= 5 × 10^6)
TokenPrice = 10^6 / 5_000_000 = 0.2 NEAR/USDT
```

## 演算

### TokenAmount / ExchangeRate = NearValue

トークン保有量から NEAR 建て価値を計算。

```rust
impl Div<&ExchangeRate> for TokenAmount {
    type Output = NearValue;
    fn div(self, rate: &ExchangeRate) -> NearValue {
        // decimals の整合性チェック
        debug_assert_eq!(self.decimals, rate.decimals);
        NearValue::new(&self.smallest_units / &rate.raw_rate)
    }
}
```

例:
```
holdings = 100_000_000 smallest_USDT (= 100 USDT)
rate = 5_000_000 smallest_USDT/NEAR
value = 100_000_000 / 5_000_000 = 20 NEAR
```

### TokenAmount × TokenPrice = NearValue

TokenPrice を使う場合は decimals 変換が必要。

```rust
impl Mul<&TokenPrice> for TokenAmount {
    type Output = NearValue;
    fn mul(self, price: &TokenPrice) -> NearValue {
        let whole_tokens = self.to_whole();  // smallest → whole
        NearValue::new(whole_tokens * price.as_bigdecimal())
    }
}
```

例:
```
holdings = 100_000_000 smallest_USDT
whole_tokens = 100_000_000 / 10^6 = 100 USDT
price = 0.2 NEAR/USDT
value = 100 × 0.2 = 20 NEAR
```

### リターン計算

```rust
impl TokenPrice {
    /// 期待リターンを計算
    /// (predicted - current) / current
    pub fn expected_return(&self, predicted: &TokenPrice) -> f64 {
        let current = self.0.to_f64().unwrap_or(0.0);
        let pred = predicted.0.to_f64().unwrap_or(0.0);
        if current == 0.0 { return 0.0; }
        (pred - current) / current
    }
}
```

**注意**: `ExchangeRate` からリターンを計算する場合は符号が逆になる。
`TokenPrice` を使えばこの混乱を防げる。

## 型の関係図

```
                    ┌─────────────────────────────────────┐
                    │         任意トークン対応            │
                    │                                     │
TokenAmount ────────┼──► / ExchangeRate ──► NearValue    │
 { smallest,        │                          │         │
   decimals }       │                          │         │
      │             │                          ▼         │
      │             │    × TokenPrice ──► NearValue      │
      │             │                          │         │
      ▼             │                          │         │
 to_whole()         │                          │         │
      │             │                          │         │
      ▼             └─────────────────────────────────────┘
  BigDecimal                                   │
                                               ▼
                    ┌─────────────────────────────────────┐
                    │         NEAR 専用                   │
                    │                                     │
                    │  NearValue ◄──► YoctoValue          │
                    │     │ × 10^24      │ / 10^24        │
                    │     └──────────────┘                │
                    │                                     │
                    │  NearAmount ◄──► YoctoAmount        │
                    │     │ × 10^24      │ / 10^24        │
                    │     └──────────────┘                │
                    │                                     │
                    └─────────────────────────────────────┘
```

## 既存型との互換性

### Price（非推奨）

現在の `Price` 型は「価格」という名前だが、実際には `ExchangeRate.raw_rate` と同じ値を保持している。
混乱を避けるため、新しいコードでは `ExchangeRate` と `TokenPrice` を使用すること。

```rust
// 非推奨
let price = Price::new(rate_from_db);

// 推奨
let rate = ExchangeRate::new(rate_from_db, decimals);
let price = rate.to_price();
```

## 使用例

### ポートフォリオ評価

```rust
async fn evaluate_portfolio(holdings: &[(String, TokenAmount)]) -> NearValue {
    let mut total = NearValue::zero();

    for (token_id, amount) in holdings {
        // DB からレートを取得
        let rate = get_exchange_rate(token_id).await?;

        // NEAR 建て価値に変換
        let value = amount.clone() / &rate;
        total = total + value;
    }

    total
}
```

### リターン比較

```rust
fn compare_returns(
    current_rates: &HashMap<String, ExchangeRate>,
    predicted_rates: &HashMap<String, ExchangeRate>,
) -> Vec<(String, f64)> {
    current_rates.iter().filter_map(|(token, current_rate)| {
        let predicted_rate = predicted_rates.get(token)?;

        // TokenPrice に変換してリターン計算（符号の間違いを防ぐ）
        let current_price = current_rate.to_price();
        let predicted_price = predicted_rate.to_price();
        let return_rate = current_price.expected_return(&predicted_price);

        Some((token.clone(), return_rate))
    }).collect()
}
```
