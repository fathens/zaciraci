use crate::Result;
use crate::ref_finance;
use crate::ref_finance::path::graph::TokenGraph;
use crate::types::gas_price::GasPrice;
use common::types::{TokenAccount, YoctoValue};
use common::types::{TokenInAccount, TokenOutAccount};
use dex::{TokenPairLike, TokenPath};
use logging::*;
use near_gas::NearGas;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// swap の固定ガス（パス先頭の関数呼び出し）
const HEAD_GAS: NearGas = NearGas::from_ggas(2700);
/// swap の per-hop ガス
const BY_STEP_GAS: NearGas = NearGas::from_ggas(2600);

/// `swap_gas_cost_yocto_u128` の sanity cap（= 100 mNEAR = 10^23 yoctoNEAR）。
///
/// production の swap gas は `(HEAD + BY_STEP × depth) × gas_price ≈ 270 μNEAR`
/// オーダーで、100 mNEAR は production baseline の約 370× に相当する。
/// `saturating_mul` による u128 オーバーフロー防御に加え、敵対 RPC が
/// 異常な `gas_price` を返した場合の attack surface（cost-aware optimization
/// で全 token に巨大 cost_deduction が適用され、特に小ポートフォリオの active
/// set が全脱落して equal-weight rebalance を強制される攻撃経路）を構造的に
/// 縮小する。`STORAGE_MIN_SANE_CAP` と対称的な「実運用の 10× オーダー」基準。
///
/// 元は 1 NEAR (10^24) だったが、それは baseline の ~3700× で過大に
/// permissive だった。financial-correctness-reviewer + security-reviewer の
/// 連名指摘に基づき、attack surface を 10× 縮小して 100 mNEAR に絞った。
///
/// この cap は SSoT [`swap_gas_cost_yocto_u128`] 内で適用されるため、`Preview::cost`
/// （arbitrage 経路）と `estimate_swap_gas_cost_yocto`（trade 経路）の両方が同時に
/// 防御される。
const GAS_YOCTO_SANE_CAP: u128 = 10u128.pow(23);

/// cap clamp warn の最短再 emit 間隔（秒）。持続的攻撃下でも 60 秒に 1 回は
/// signal が残るよう調整。
const GAS_CAP_WARN_INTERVAL_SECS: u64 = 60;

/// 直前に cap clamp warn を emit した Unix 秒 (epoch second)。
/// `0` は未 emit を意味する。
static GAS_CAP_LAST_WARN_UNIX: AtomicU64 = AtomicU64::new(0);

/// 指定 depth の swap で消費するガス料金を yoctoNEAR を u128 で算出する SSoT。
///
/// `(HEAD + BY_STEP * depth) * gas_price` を saturating 算術で計算し、
/// 結果を [`GAS_YOCTO_SANE_CAP`] にクランプする。公開 API の
/// [`estimate_swap_gas_cost_yocto`] と private な [`Preview::cost`] が共に
/// この関数を経由するため、計算式と cap 適用は完全に一致する。
///
/// cap が発動した場合は `warn!` を [`GAS_CAP_WARN_INTERVAL_SECS`] 間隔で
/// rate-limited に emit する。`std::sync::Once` 方式は 1 回しか発火しない
/// ため持続的な敵対 RPC 応答が「ある日急に portfolio が equal-weight になった」
/// 等の症状調査時に observable signal を残せない問題があった。
/// `AtomicU64 + compare_exchange` で「前回 emit から N 秒経過」を判定する。
fn swap_gas_cost_yocto_u128(gas_price: GasPrice, depth: usize) -> u128 {
    let gas = HEAD_GAS
        .as_gas()
        .saturating_add(BY_STEP_GAS.as_gas().saturating_mul(depth as u64));
    let raw = (gas as u128).saturating_mul(gas_price.to_balance());
    if raw > GAS_YOCTO_SANE_CAP {
        emit_cap_warn_rate_limited(raw, depth, gas_price);
    }
    raw.min(GAS_YOCTO_SANE_CAP)
}

/// cap clamp 発動時の warn を [`GAS_CAP_WARN_INTERVAL_SECS`] 間隔で emit する。
///
/// `compare_exchange` で「前回 emit 時刻 → 今の時刻」への CAS が成功した
/// thread だけが warn を emit する。並列 thread が同時に発火条件を満たした
/// 場合でも 1 thread のみ通過し他は静かに退避する。
fn emit_cap_warn_rate_limited(raw: u128, depth: usize, gas_price: GasPrice) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if try_acquire_cap_warn_slot(now) {
        let log = DEFAULT.new(o!("function" => "swap_gas_cost_yocto_u128"));
        warn!(log, "gas yocto clamped to sane cap";
            "raw" => raw,
            "cap" => GAS_YOCTO_SANE_CAP,
            "depth" => depth,
            "gas_price_yocto" => gas_price.to_balance(),
            "rate_limit_secs" => GAS_CAP_WARN_INTERVAL_SECS,
        );
    }
}

/// rate-limit slot 取得の純粋ロジック (テスト用に時刻を引数化)。
///
/// `last + GAS_CAP_WARN_INTERVAL_SECS <= now` の場合のみ slot を取得し
/// `GAS_CAP_LAST_WARN_UNIX` を `now` に CAS で書き換える。`true` を返したら
/// caller は warn を emit してよい。並列 thread の同時取得は 1 thread のみが
/// 成功する。
fn try_acquire_cap_warn_slot(now: u64) -> bool {
    let last = GAS_CAP_LAST_WARN_UNIX.load(Ordering::Relaxed);
    if now.saturating_sub(last) < GAS_CAP_WARN_INTERVAL_SECS {
        return false;
    }
    GAS_CAP_LAST_WARN_UNIX
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

/// 指定 depth の swap で消費するガス料金を yoctoNEAR で見積もる。
///
/// 内部で [`swap_gas_cost_yocto_u128`] を呼ぶ薄いラッパで、`YoctoValue` を返す。
/// 外部クレート（trade 等）からもコスト推定できるよう公開する。
pub fn estimate_swap_gas_cost_yocto(gas_price: GasPrice, depth: usize) -> YoctoValue {
    YoctoValue::from_yocto_u128(swap_gas_cost_yocto_u128(gas_price, depth))
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Preview<M> {
    pub gas_price: GasPrice,
    pub input_value: M,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
    pub gain: u128,
}

impl<M> Preview<M>
where
    M: Into<u128> + Copy,
{
    pub fn new(
        gas_price: GasPrice,
        input_value: M,
        token: TokenOutAccount,
        depth: usize,
        output_value: u128,
    ) -> Self {
        let gain = Self::gain(gas_price, depth, input_value, output_value);
        Preview {
            gas_price,
            input_value,
            token,
            depth,
            output_value,
            gain,
        }
    }

    fn cost(gas_price: GasPrice, depth: usize) -> u128 {
        swap_gas_cost_yocto_u128(gas_price, depth)
    }

    fn gain(gas_price: GasPrice, depth: usize, input_value: M, output_value: u128) -> u128 {
        let input_value = input_value.into();
        if output_value <= input_value {
            return 0;
        }
        let gain = output_value - input_value;
        let cost = Self::cost(gas_price, depth);
        if gain <= cost {
            return 0;
        }
        gain - cost
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct PreviewList<M> {
    pub input_value: M,
    pub list: Vec<Preview<M>>,
    pub total_gain: u128,
}

impl<M> PreviewList<M> {
    pub fn new(input_value: M, previews: Vec<Preview<M>>) -> Option<Self> {
        let total_gain: u128 = previews.iter().map(|p| p.gain).sum();
        Some(PreviewList {
            input_value,
            list: previews,
            total_gain,
        })
    }

    pub async fn into_with_path(
        self,
        graph: &TokenGraph,
        start: &TokenInAccount,
    ) -> Result<(Vec<(Preview<M>, TokenPath)>, Vec<TokenAccount>)> {
        let mut tokens = Vec::new();
        let mut pre_path = Vec::new();
        for p in self.list {
            let path = ref_finance::path::swap_path(graph, start, &p.token).await?;
            for pair in path.0.iter() {
                tokens.push(pair.token_in_id().into());
                tokens.push(pair.token_out_id().into());
            }
            pre_path.push((p, path));
        }
        tokens.sort();
        tokens.dedup();

        Ok((pre_path, tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MicroNear, MilliNear};
    use common::types::TokenAccount;
    use common::types::TokenOutAccount;
    use near_sdk::NearToken;

    fn token_out(token: &str) -> TokenOutAccount {
        let token: TokenAccount = token.parse().unwrap();
        token.into()
    }

    const HEAD: u128 = MicroNear::of(270).to_yocto();
    const BY_STEP: u128 = MicroNear::of(260).to_yocto();
    const MIN_GAS_PRICE: GasPrice = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));

    #[test]
    fn test_preview_cost() {
        assert_eq!(Preview::<MilliNear>::cost(MIN_GAS_PRICE, 1), HEAD + BY_STEP);
        assert_eq!(
            Preview::<MilliNear>::cost(MIN_GAS_PRICE, 2),
            HEAD + 2 * BY_STEP
        );
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_matches_estimate() {
        // SSoT 関数と公開ラッパが同一値を返すことを保証する。
        for depth in [0usize, 1, 2, 3, 5] {
            let direct = swap_gas_cost_yocto_u128(MIN_GAS_PRICE, depth);
            let via_yocto = estimate_swap_gas_cost_yocto(MIN_GAS_PRICE, depth);
            assert_eq!(YoctoValue::from_yocto_u128(direct), via_yocto);
        }
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_zero_depth() {
        // depth=0 の場合は HEAD のみ。
        assert_eq!(swap_gas_cost_yocto_u128(MIN_GAS_PRICE, 0), HEAD);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_below_cap_passthrough() {
        // production gas_price は ~10^8 yocto/gas、cap (10^23 = 100 mNEAR) には
        // ~2 桁 (~64×) の余裕がある（depth=5 で raw ≈ 1.57e21）。
        // 通常の depth ではクランプは発動せず、入力どおりの値が返る。
        let result = swap_gas_cost_yocto_u128(MIN_GAS_PRICE, 5);
        assert!(result < GAS_YOCTO_SANE_CAP);
        let expected = HEAD + 5 * BY_STEP;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_above_cap_clamped() {
        // 異常な gas_price（敵対 RPC を模した値）を渡すと cap でクランプされる。
        // 100 mNEAR / (HEAD + BY_STEP) ≈ 10^23 / 5.3e12 gas ≈ 1.9e10 yocto/gas
        // が境界、それを超える gas_price では確実にクランプ発動。
        let hostile = GasPrice::from_balance(NearToken::from_yoctonear(10u128.pow(20)));
        let result = swap_gas_cost_yocto_u128(hostile, 1);
        assert_eq!(result, GAS_YOCTO_SANE_CAP);
    }

    /// rate-limit ロジックの直接検証ヘルパー。並列 test と同じ static を共有
    /// するため `serial_test::serial` で個別実行を強制し、毎回 0 にリセットする。
    fn reset_cap_warn_static() {
        GAS_CAP_LAST_WARN_UNIX.store(0, Ordering::Relaxed);
    }

    #[test]
    #[serial_test::serial]
    fn test_try_acquire_cap_warn_slot_first_call_acquires() {
        reset_cap_warn_static();
        let now = 1_000_000_u64;
        assert!(
            try_acquire_cap_warn_slot(now),
            "first call must acquire slot when static is 0"
        );
        // 初回取得後は static が `now` に進む。
        assert_eq!(GAS_CAP_LAST_WARN_UNIX.load(Ordering::Relaxed), now);
    }

    #[test]
    #[serial_test::serial]
    fn test_try_acquire_cap_warn_slot_within_interval_blocks() {
        reset_cap_warn_static();
        let t0 = 1_000_000_u64;
        assert!(try_acquire_cap_warn_slot(t0));
        // INTERVAL 未満で同じ thread が再度取得しようとしても false
        let t1 = t0 + GAS_CAP_WARN_INTERVAL_SECS - 1;
        assert!(
            !try_acquire_cap_warn_slot(t1),
            "second call within {GAS_CAP_WARN_INTERVAL_SECS}s must be rate-limited"
        );
        assert_eq!(
            GAS_CAP_LAST_WARN_UNIX.load(Ordering::Relaxed),
            t0,
            "blocked call must not update the static"
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_try_acquire_cap_warn_slot_after_interval_acquires() {
        reset_cap_warn_static();
        let t0 = 1_000_000_u64;
        assert!(try_acquire_cap_warn_slot(t0));
        let t1 = t0 + GAS_CAP_WARN_INTERVAL_SECS;
        assert!(
            try_acquire_cap_warn_slot(t1),
            "call >= INTERVAL after the first must acquire slot again"
        );
        assert_eq!(GAS_CAP_LAST_WARN_UNIX.load(Ordering::Relaxed), t1);
    }

    #[test]
    #[serial_test::serial]
    fn test_try_acquire_cap_warn_slot_clock_regression_blocks() {
        // CAS で last を取り直すため、clock が後退してもまず INTERVAL ガードで弾く。
        reset_cap_warn_static();
        let t0 = 1_000_000_u64;
        assert!(try_acquire_cap_warn_slot(t0));
        let t_back = t0.saturating_sub(GAS_CAP_WARN_INTERVAL_SECS / 2);
        assert!(
            !try_acquire_cap_warn_slot(t_back),
            "regressed clock must not acquire slot inside the interval"
        );
        assert_eq!(GAS_CAP_LAST_WARN_UNIX.load(Ordering::Relaxed), t0);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_at_u64_max_gas_price_clamped() {
        // gas_price が u64::MAX に張り付いた場合（saturating_mul 経由の overflow path）
        // も cap でクランプされ u128::MAX に発散しない。
        let max_gas_price = GasPrice::from_balance(NearToken::from_yoctonear(u64::MAX as u128));
        let result = swap_gas_cost_yocto_u128(max_gas_price, 5);
        assert_eq!(result, GAS_YOCTO_SANE_CAP);
    }

    #[test]
    fn test_preview_gain() {
        assert_eq!(
            Preview::gain(
                MIN_GAS_PRICE,
                1,
                MilliNear::of(100),
                MilliNear::of(300).to_yocto()
            ),
            MilliNear::of(200).to_yocto() - HEAD - BY_STEP
        );

        assert_eq!(
            Preview::gain(
                MIN_GAS_PRICE,
                2,
                MicroNear::of(100_000),
                MilliNear::of(200).to_yocto()
            ),
            MilliNear::of(100).to_yocto() - HEAD - 2 * BY_STEP
        );
    }

    #[test]
    fn test_preview_list_total_gain_milli() {
        let a = Preview::new(
            MIN_GAS_PRICE,
            MilliNear::of(100),
            token_out("a.token"),
            1,
            MilliNear::of(300).to_yocto(),
        );
        let b = Preview::new(
            MIN_GAS_PRICE,
            MilliNear::of(100),
            token_out("b.token"),
            1,
            MilliNear::of(200).to_yocto(),
        );
        let previews = vec![a.clone(), b.clone()];
        let preview_list = PreviewList::new(MilliNear::of(100), previews).unwrap();
        assert_eq!(preview_list.total_gain, a.gain + b.gain);
    }

    #[test]
    fn test_preview_list_total_gain_micro() {
        let a = Preview::new(
            MIN_GAS_PRICE,
            MicroNear::of(100_000),
            token_out("a.token"),
            1,
            MilliNear::of(300).to_yocto(),
        );
        let b = Preview::new(
            MIN_GAS_PRICE,
            MicroNear::of(100_000),
            token_out("b.token"),
            1,
            MilliNear::of(200).to_yocto(),
        );
        let previews = vec![a.clone(), b.clone()];
        let preview_list = PreviewList::new(MicroNear::of(100_000), previews).unwrap();
        assert_eq!(preview_list.total_gain, a.gain + b.gain);
    }
}
