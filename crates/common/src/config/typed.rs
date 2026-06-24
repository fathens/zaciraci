use std::sync::LazyLock;
use std::time::Duration;

// ── ConfigValueType enum ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValueType {
    Bool,
    U16,
    U32,
    U64,
    U128,
    I64,
    F64,
    String,
    Duration,
}

impl ConfigValueType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::String => "string",
            Self::Duration => "duration",
        }
    }
}

impl std::fmt::Display for ConfigValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── KeyDefinition / ResolvedKeyInfo ──

pub struct KeyDefinition {
    pub key: &'static str,
    pub description: &'static str,
    pub value_type: ConfigValueType,
    pub default_value: &'static str,
}

pub struct ResolvedKeyInfo {
    pub key: std::string::String,
    pub description: std::string::String,
    pub value_type: ConfigValueType,
    pub resolved_value: std::string::String,
}

// ── ConfigResolve trait: type-specific config resolution ──

pub(crate) trait ConfigResolve: Sized {
    type Default;
    const VALUE_TYPE: ConfigValueType;
    fn resolve(key: &str, default: Self::Default) -> Self;
    fn resolve_without_db(key: &str, default: Self::Default) -> Self;
    fn display_string(value: Self) -> std::string::String;

    /// 文字列値が当該型として有効か検証する (Layer 0 用)。
    ///
    /// `persistence::config_store::reload_to_config` が DB から読んだ値を
    /// `DB_STORE` に流入させる前に呼び、`Err` ならその key は load 対象から
    /// 除外して slog `error!` ログを残す。これにより `resolve` 経路で不正値
    /// に到達するのを根本的に防ぎ、enum 等の panic 経路を構造的に塞ぐ。
    ///
    /// default 実装は `Ok(())` を返す (数値型は別途 clamp で防御済み)。
    /// enum 系などで「parse 失敗 = silent 縮退させたくない」型は override する。
    fn validate_string(_s: &str) -> std::result::Result<(), std::string::String> {
        Ok(())
    }
}

impl ConfigResolve for bool {
    type Default = bool;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::Bool;
    fn resolve(key: &str, default: bool) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| v.to_lowercase().parse::<bool>().ok())
            .unwrap_or(default)
    }
    fn resolve_without_db(key: &str, default: bool) -> Self {
        crate::config::store::get_excluding_db(key)
            .ok()
            .and_then(|v| v.to_lowercase().parse::<bool>().ok())
            .unwrap_or(default)
    }
    fn display_string(value: Self) -> std::string::String {
        value.to_string()
    }
}

impl ConfigResolve for String {
    type Default = &'static str;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::String;
    fn resolve(key: &str, default: &'static str) -> Self {
        crate::config::store::get(key).unwrap_or_else(|_| default.to_string())
    }
    fn resolve_without_db(key: &str, default: &'static str) -> Self {
        crate::config::store::get_excluding_db(key).unwrap_or_else(|_| default.to_string())
    }
    fn display_string(value: Self) -> std::string::String {
        value
    }
}

macro_rules! impl_config_resolve_numeric {
    ($ty:ty, $variant:ident) => {
        impl ConfigResolve for $ty {
            type Default = $ty;
            const VALUE_TYPE: ConfigValueType = ConfigValueType::$variant;
            fn resolve(key: &str, default: $ty) -> Self {
                crate::config::store::get(key)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
                    .unwrap_or(default)
            }
            fn resolve_without_db(key: &str, default: $ty) -> Self {
                crate::config::store::get_excluding_db(key)
                    .ok()
                    .and_then(|v| v.parse::<$ty>().ok())
                    .unwrap_or(default)
            }
            fn display_string(value: Self) -> std::string::String {
                value.to_string()
            }
        }
    };
}

impl_config_resolve_numeric!(u16, U16);
impl_config_resolve_numeric!(u32, U32);
impl_config_resolve_numeric!(u64, U64);
impl_config_resolve_numeric!(u128, U128);
impl_config_resolve_numeric!(usize, U64);
impl_config_resolve_numeric!(i64, I64);
impl_config_resolve_numeric!(f64, F64);

impl ConfigResolve for Duration {
    type Default = Duration;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::Duration;
    fn resolve(key: &str, default: Duration) -> Self {
        crate::config::store::get(key)
            .ok()
            .and_then(|v| humantime::parse_duration(&v).ok())
            .unwrap_or(default)
    }
    fn resolve_without_db(key: &str, default: Duration) -> Self {
        crate::config::store::get_excluding_db(key)
            .ok()
            .and_then(|v| humantime::parse_duration(&v).ok())
            .unwrap_or(default)
    }
    fn display_string(value: Self) -> std::string::String {
        humantime::format_duration(value).to_string()
    }
}

impl ConfigResolve for anyhow::Result<String> {
    type Default = ();
    const VALUE_TYPE: ConfigValueType = ConfigValueType::String;
    fn resolve(key: &str, _default: ()) -> Self {
        crate::config::store::get(key)
            .map_err(|_| anyhow::anyhow!("required config key not found: {}", key))
    }
    fn resolve_without_db(key: &str, _default: ()) -> Self {
        crate::config::store::get_excluding_db(key)
            .map_err(|_| anyhow::anyhow!("required config key not found: {}", key))
    }
    fn display_string(value: Self) -> std::string::String {
        value.unwrap_or_else(|_| "(未設定)".to_string())
    }
}

/// `PredErrDiagonalMode` の typed config 解決。
///
/// 値が config に存在しない場合は `default` を返す。値が存在するが `FromStr`
/// で parse 失敗した場合の挙動は **value source ごとに分岐** する:
///
/// - `resolve` (CONFIG_STORE > DB_STORE > env > TOML 全経路): default fallback で
///   fail-soft。cron tick 毎に呼ばれる経路で panic すると persistent crash loop
///   DoS (CRITICAL-2) になるため、`resolve` 内の panic は撤去済み。
/// - `resolve_without_db` (env > TOML のみ、startup 限定パス): 不正値は panic で
///   起動失敗にし sysadmin が即時気付ける運用にする (再起動 = 修正機会)。
///
/// # 多層防御 (Layer 0 + Layer 1 + Layer 2)
///
/// **Layer 0**: [`persistence::config_store::reload_to_config`] が
/// [`crate::config::validate_db_configs`] と [`Self::validate_string`] を使い、
/// DB 由来の不正値を `DB_STORE` への流入前に排除して slog `error!` で構造化
/// ログを残す。`common` クレートは `logging` に循環依存できないため、
/// structured log 化は persistence 側で担う設計。
///
/// **Layer 1** (`resolve_without_db`): env/TOML 由来の不正値を startup panic で
/// 起動失敗にする。`resolve` (DB含む) との非対称性は意図的: DB_STORE 不正値は
/// cron tick で繰り返し発火し crash loop DoS 化するため fail-soft、env/TOML は
/// startup 1 回のみで再起動 = 修正機会。
///
/// **Layer 2** (`resolve` の silent default fallback): Layer 0/1 の保険。本来到達
/// 不能な経路として残しているが、以下 3 種の死角が残る:
/// 1. **Layer 0 race**: `reload_to_config` が完了する前に `resolve` が呼ばれる
///    起動時 race。実害はリロード完了後の次サイクルで自動収束するため軽微。
/// 2. **`validate_db_configs` 登録漏れ**: 新しい enum 系 typed config を追加した
///    ときに `validate_db_configs` への登録を忘れると DB 不正値が DB_STORE に流入
///    する。新型追加時は同関数のテストで網羅性を確認すること。
/// 3. **将来 admin/gRPC config write API 追加時の脆弱化**: 現時点 CONFIG_STORE は
///    `#[doc(hidden)]` test-only API でのみ書き込まれる trusted source だが、将来
///    admin API / gRPC endpoint で外部書き込みを許す場合は `validate_db_configs`
///    を `validate_all_configs` に汎用化して CONFIG_STORE/env も同時検証する
///    必要がある (follow-up: PR でない別 issue で追跡)。
impl ConfigResolve for crate::algorithm::portfolio::PredErrDiagonalMode {
    type Default = Self;
    const VALUE_TYPE: ConfigValueType = ConfigValueType::String;
    fn resolve(key: &str, default: Self) -> Self {
        match crate::config::store::get(key) {
            // 不正値は silent default fallback (Layer 2 の保険動作)。Layer 0 が
            // DB_STORE 流入を排除し、Layer 1 が env/TOML startup panic を担うため、
            // ここに到達する経路は本来存在しない。CONFIG_STORE 経由 (test override
            // など) で到達した場合のみ silent fallback で吸収する。
            Ok(s) => s.parse().unwrap_or(default),
            Err(_) => default,
        }
    }
    fn resolve_without_db(key: &str, default: Self) -> Self {
        match crate::config::store::get_excluding_db(key) {
            Ok(s) => s.parse().unwrap_or_else(|e| {
                // env/TOML 経由の不正値は startup-only パスで sysadmin 制御下にあるため
                // 起動失敗 (panic) で気付かせるのが妥当。`resolve` (DB含む) との非対称性は
                // 意図的: DB_STORE 不正値は cron tick で繰り返し発火し crash loop DoS 化
                // するため fail-soft、env/TOML は startup 1 回のみで再起動 = 修正機会。
                // `e` (= ParsePredErrDiagonalModeError) の Display は redact 済みの
                // 固定文字列のため、attacker-controlled 値は panic message に含まれない。
                panic!("invalid config value for {key}: {e}");
            }),
            Err(_) => default,
        }
    }
    fn display_string(value: Self) -> std::string::String {
        value.as_str().to_string()
    }
    fn validate_string(s: &str) -> std::result::Result<(), std::string::String> {
        // 攻撃者制御の入力値を error message に含めない (log forwarding 経由漏洩防御):
        // 失敗時のメッセージは「期待されたバリアント名」のみで input value を含まない。
        // 期待バリアントは PredErrDiagonalMode::variants_doc() を SSoT として参照する。
        s.parse::<crate::algorithm::portfolio::PredErrDiagonalMode>()
            .map(|_| ())
            .map_err(|_| {
                format!(
                    "expected one of {}",
                    crate::algorithm::portfolio::PredErrDiagonalMode::variants_doc()
                )
            })
    }
}

// ── MockStore trait: maps types to Clone-able mock storage ──

pub trait MockStore: Sized {
    /// The type stored in MockConfig fields (must be Clone).
    /// For most types this is Self. For Result<String> it is String.
    type Storage: Clone;

    /// Convert stored value to the actual return type.
    fn from_storage(s: &Self::Storage) -> Self;
}

impl MockStore for bool {
    type Storage = bool;
    fn from_storage(s: &bool) -> Self {
        *s
    }
}

impl MockStore for String {
    type Storage = String;
    fn from_storage(s: &String) -> Self {
        s.clone()
    }
}

macro_rules! impl_mock_store_copy {
    ($($ty:ty),*) => {
        $(impl MockStore for $ty {
            type Storage = $ty;
            fn from_storage(s: &$ty) -> Self { *s }
        })*
    }
}

impl_mock_store_copy!(u16, u32, u64, u128, usize, i64, f64);

impl MockStore for Duration {
    type Storage = Duration;
    fn from_storage(s: &Duration) -> Self {
        *s
    }
}

impl MockStore for crate::algorithm::portfolio::PredErrDiagonalMode {
    type Storage = Self;
    fn from_storage(s: &Self) -> Self {
        *s
    }
}

/// For Result<String>, MockConfig stores just a String.
/// Setting `mock.database_url = Some("postgres://...")` will return `Ok(...)`.
impl MockStore for anyhow::Result<String> {
    type Storage = String;
    fn from_storage(s: &String) -> Self {
        Ok(s.clone())
    }
}

// ── Main macro ──

/// Declarative macro that generates:
/// - `ConfigAccess` trait with typed accessor methods
/// - `ConfigResolver` struct that resolves values via `config::get()` priority chain
/// - `MockConfig` struct for test isolation (wraps real resolver, overrides per-field)
/// - `KEY_DEFINITIONS` const with static metadata for all config keys
/// - `resolve_all_without_db()` function for runtime key resolution excluding DB
///
/// ## Optional `clamp:` parameter
///
/// An entry may declare an optional `clamp: <fn>` parameter. The given
/// function is applied to every resolved value before the accessor returns
/// (defense-in-depth against extreme value injection via env / TOML /
/// CONFIG_STORE / DB_STORE — F016). The clamp is applied in
/// `ConfigResolver`, `MockConfig` (both the override path and the base
/// delegation), and `resolve_all_without_db()` so the displayed value
/// matches what callers actually receive. Clamping must be idempotent.
macro_rules! define_typed_config {
    (
        $(
            $(#[doc = $doc:expr])*
            fn $method:ident() -> $ty:ty {
                key: $key:expr,
                default: $default:expr
                $(, clamp: $clamp:expr)?
            }
        )*
    ) => {
        pub trait ConfigAccess: Send + Sync {
            $(
                $(#[doc = $doc])*
                fn $method(&self) -> $ty;
            )*
        }

        #[derive(Clone, Copy)]
        pub struct ConfigResolver;

        impl ConfigAccess for ConfigResolver {
            $(
                fn $method(&self) -> $ty {
                    let v = <$ty as ConfigResolve>::resolve($key, $default);
                    $( let v = ($clamp)(v); )?
                    v
                }
            )*
        }

        #[doc(hidden)]
        pub struct MockConfig {
            base: ConfigResolver,
            $( pub $method: Option<<$ty as MockStore>::Storage>, )*
        }

        impl Default for MockConfig {
            fn default() -> Self {
                Self::new()
            }
        }

        impl MockConfig {
            pub fn new() -> Self {
                Self {
                    base: ConfigResolver,
                    $( $method: None, )*
                }
            }
        }

        impl ConfigAccess for MockConfig {
            $(
                fn $method(&self) -> $ty {
                    let v = match &self.$method {
                        Some(v) => <$ty as MockStore>::from_storage(v),
                        None => self.base.$method(),
                    };
                    $( let v = ($clamp)(v); )?
                    v
                }
            )*
        }

        pub const KEY_DEFINITIONS: &[KeyDefinition] = &[
            $(
                KeyDefinition {
                    key: $key,
                    description: concat!($($doc, "\n",)*),
                    value_type: <$ty as ConfigResolve>::VALUE_TYPE,
                    default_value: stringify!($default),
                },
            )*
        ];

        pub fn resolve_all_without_db() -> Vec<ResolvedKeyInfo> {
            vec![
                $(
                    {
                        let value = <$ty as ConfigResolve>::resolve_without_db($key, $default);
                        $( let value = ($clamp)(value); )?
                        ResolvedKeyInfo {
                            key: $key.to_string(),
                            description: concat!($($doc, "\n",)*).trim().to_string(),
                            value_type: <$ty as ConfigResolve>::VALUE_TYPE,
                            resolved_value: <$ty as ConfigResolve>::display_string(value),
                        }
                    },
                )*
            ]
        }
    };
}

// ── Defense-in-depth clamp ranges (F016) ──
//
// These bounds defend the optimizer against extreme values injected via env,
// TOML, CONFIG_STORE, or DB_STORE (e.g. via DB write-privilege compromise).
// They are applied at the typed-config read boundary so every consumer sees
// a sane value without having to remember to clamp at the call site.

/// Lower bound for [`ConfigAccess::portfolio_cost_iterations_max`].
///
/// At least one iteration is always required so that the cost-aware
/// optimization records an initial-state weight assignment even on
/// misconfiguration.
const PORTFOLIO_COST_ITERATIONS_MAX_LOWER: u32 = 1;

/// Upper bound for [`ConfigAccess::portfolio_cost_iterations_max`].
///
/// Production normally converges in 3–5 iterations. The cap of 10 leaves
/// headroom for slow-converging market regimes while preventing DoS via
/// `u32::MAX` injection (each iteration runs the full Markowitz solve).
const PORTFOLIO_COST_ITERATIONS_MAX_UPPER: u32 = 10;

/// Lower bound for [`ConfigAccess::portfolio_pred_err_diagonal_k`].
///
/// Negative `k` would deflate (rather than inflate) the covariance diagonal
/// and bias the optimizer toward poorly-predicted tokens. `0.0` effectively
/// disables prediction-error inflation.
const PORTFOLIO_PRED_ERR_DIAGONAL_K_LOWER: f64 = 0.0;

/// Upper bound for [`ConfigAccess::portfolio_pred_err_diagonal_k`].
///
/// `k = 100` is already 1000× the production default `0.1`. Beyond this the
/// diagonal dominates the off-diagonal covariance and the matrix becomes
/// effectively diagonal, breaking the correlation structure the optimizer
/// relies on.
const PORTFOLIO_PRED_ERR_DIAGONAL_K_UPPER: f64 = 100.0;

/// Lower bound for [`ConfigAccess::portfolio_cost_iteration_damping`].
///
/// `0.1` keeps `damp_and_diff` (`next = (1 - α) × prev + α × candidate`)
/// progressing meaningfully toward the candidate at every iteration. Below this:
///
/// - `α = 0.0` freezes the iterate at the initial uniform `1/n` weights,
///   which makes `run_cost_aware_optimization` break out at iteration 1 via
///   `max_diff < CONVERGENCE_TOLERANCE` (= 1e-3) — `cost_deductions` are then
///   never propagated into the optimizer, silently disabling cost-aware return.
///   A DB-write attacker injecting `PORTFOLIO_COST_ITERATION_DAMPING = 0.0`
///   would defeat the cost defense without any observable signal.
/// - `α ∈ (0.0, 0.1)` produces a step weak enough that with the production
///   `iterations_max = 10`, the iterate covers under ~40 % of the distance to
///   the candidate (e.g. α = 0.05 ⇒ ~40 % cumulative progress) — a "degraded
///   but not stopped" mode harder to detect than the full freeze and still
///   meaningfully blunting the cost defense. `0.1` is the minimum that keeps
///   the iteration behavior recognizably converging.
///
/// Values below `0.0` would invert the update and push the iterate away from
/// the candidate, breaking the convergence invariant.
pub const PORTFOLIO_COST_ITERATION_DAMPING_LOWER: f64 = 0.1;

/// Upper bound for [`ConfigAccess::portfolio_cost_iteration_damping`].
///
/// `1.0` corresponds to a full replacement step (no damping). Values above
/// `1.0` overshoot the candidate and are equivalent to under-damping, which
/// `damp_and_diff` already clamps internally; we reject them here so the
/// effective value displayed by `resolve_all_without_db` matches what the
/// optimizer actually uses.
pub const PORTFOLIO_COST_ITERATION_DAMPING_UPPER: f64 = 1.0;

/// Fallback value applied when [`ConfigAccess::portfolio_cost_iteration_damping`]
/// resolves to `NaN`.
///
/// `0.5` matches the production default and is mid-range — neither freezing
/// the iterate (`0.0`) nor disabling damping entirely (`1.0`). Mirrors the
/// `pred_err_diagonal_k → lower` policy in spirit (NaN must not poison the
/// optimizer) while preserving useful iteration behavior on misconfiguration.
const PORTFOLIO_COST_ITERATION_DAMPING_NAN_FALLBACK: f64 = 0.5;

/// Idempotent clamp applied to `portfolio_cost_iterations_max` reads.
fn clamp_portfolio_cost_iterations_max(v: u32) -> u32 {
    v.clamp(
        PORTFOLIO_COST_ITERATIONS_MAX_LOWER,
        PORTFOLIO_COST_ITERATIONS_MAX_UPPER,
    )
}

/// Idempotent clamp applied to `portfolio_pred_err_diagonal_k` reads.
///
/// `f64::clamp` propagates `NaN`, so `NaN` is mapped to the lower bound
/// (effectively disabling the inflation) instead of poisoning the optimizer.
/// `±INFINITY` is handled correctly by `f64::clamp` itself.
fn clamp_portfolio_pred_err_diagonal_k(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_PRED_ERR_DIAGONAL_K_LOWER
    } else {
        v.clamp(
            PORTFOLIO_PRED_ERR_DIAGONAL_K_LOWER,
            PORTFOLIO_PRED_ERR_DIAGONAL_K_UPPER,
        )
    }
}

/// Lower bound for [`ConfigAccess::trade_prediction_shrinkage_lambda`].
///
/// `0.0` disables shrinkage entirely (the formula reduces to the identity
/// `μ_adj = μ`). Negative values would *amplify* the raw expected return
/// against its prediction error, which inverts the intent of the
/// uncertainty-aware adjustment.
const TRADE_PREDICTION_SHRINKAGE_LAMBDA_LOWER: f64 = 0.0;

/// Upper bound for [`ConfigAccess::trade_prediction_shrinkage_lambda`].
///
/// At `λ = 1.0`, a typical 10 % MAPE prediction (√MSRE ≈ 0.10) fully nulls
/// a typical 3 % expected return through the soft-threshold. Higher values
/// are not analytically wrong but would routinely zero out all signals,
/// reducing the optimizer to a cost-deduction-only mode.
const TRADE_PREDICTION_SHRINKAGE_LAMBDA_UPPER: f64 = 1.0;

/// Idempotent clamp applied to `trade_prediction_shrinkage_lambda` reads.
///
/// `NaN` is mapped to the lower bound (disabling shrinkage) rather than
/// poisoning the optimizer. `±INFINITY` is handled by `f64::clamp` itself.
fn clamp_trade_prediction_shrinkage_lambda(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_PREDICTION_SHRINKAGE_LAMBDA_LOWER
    } else {
        v.clamp(
            TRADE_PREDICTION_SHRINKAGE_LAMBDA_LOWER,
            TRADE_PREDICTION_SHRINKAGE_LAMBDA_UPPER,
        )
    }
}

/// Lower bound for [`ConfigAccess::trade_max_price_impact`].
///
/// `0.005` (0.5 %) is the floor: thresholds below half a percent would
/// reject almost every realistic swap (normal multi-hop AMM routing incurs
/// fractions of a percent of depth impact), collapsing the strategy into
/// permanent Hold.
const TRADE_MAX_PRICE_IMPACT_LOWER: f64 = 0.005;

/// Upper bound for [`ConfigAccess::trade_max_price_impact`].
///
/// `0.95` (95 %) is the ceiling: the guard exists to block catastrophic
/// thin-pool routes (observed up to 97 % impact), so a threshold at or above
/// 95 % would let essentially all of them through and defeat the purpose.
const TRADE_MAX_PRICE_IMPACT_UPPER: f64 = 0.95;

/// NaN fallback for [`ConfigAccess::trade_max_price_impact`].
///
/// A poisoned config read must not disable the guard: a `NaN` threshold in
/// the comparison `impact > threshold` would always evaluate to false and
/// silently allow catastrophic swaps. The fallback matches the documented
/// default.
const TRADE_MAX_PRICE_IMPACT_NAN_FALLBACK: f64 = 0.5;

/// Idempotent clamp applied to `trade_max_price_impact` reads.
///
/// `NaN` is mapped to the documented default (keeping the guard active)
/// rather than poisoning the comparison. `±INFINITY` is handled by
/// `f64::clamp` itself.
fn clamp_trade_max_price_impact(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_MAX_PRICE_IMPACT_NAN_FALLBACK
    } else {
        v.clamp(TRADE_MAX_PRICE_IMPACT_LOWER, TRADE_MAX_PRICE_IMPACT_UPPER)
    }
}

/// Lower bound for [`ConfigAccess::trade_lst_carry_min_hold_days`].
///
/// `30` is the floor: the liquid-staking carry backtest showed that holding
/// windows shorter than 30 days are not reliably positive (7–14 day windows
/// won only 55–77 % of the time as rate noise swamps the ~4 %/yr drift),
/// whereas every window of 30 days or more was net-positive. The min-hold
/// gate is the sole guarantor of the positive-return property, so the floor
/// must not drop below it.
const TRADE_LST_CARRY_MIN_HOLD_DAYS_LOWER: u32 = 30;

/// Upper bound for [`ConfigAccess::trade_lst_carry_min_hold_days`].
///
/// `90` caps the hold so the forced-liquidation fee drag at period boundaries
/// stays amortized over a reasonable horizon without locking capital
/// indefinitely. Longer holds give diminishing carry benefit and reduce the
/// strategy's ability to react to a de-peg.
const TRADE_LST_CARRY_MIN_HOLD_DAYS_UPPER: u32 = 90;

/// Idempotent clamp applied to `trade_lst_carry_min_hold_days` reads.
///
/// `u32` cannot be `NaN` or negative, so the only failure modes are values
/// below the 30-day positive-return floor or above the 90-day cap; both are
/// brought into range by `u32::clamp`.
fn clamp_trade_lst_carry_min_hold_days(v: u32) -> u32 {
    v.clamp(
        TRADE_LST_CARRY_MIN_HOLD_DAYS_LOWER,
        TRADE_LST_CARRY_MIN_HOLD_DAYS_UPPER,
    )
}

/// Lower bound for [`ConfigAccess::trade_lst_carry_max_depeg`].
///
/// `0.01` (1 %) is the floor: a tolerance below one percent would reject the
/// normal day-to-day rate noise of a healthy LST pool and collapse the carry
/// strategy into permanent Hold.
const TRADE_LST_CARRY_MAX_DEPEG_LOWER: f64 = 0.01;

/// Upper bound for [`ConfigAccess::trade_lst_carry_max_depeg`].
///
/// `0.5` (50 %) is the ceiling: the guard exists to detect a liquid-staking
/// de-peg (the pool rate moving sharply against the token's intrinsic value),
/// so a tolerance at or above half the position value would let a genuine
/// de-peg through and defeat the purpose.
const TRADE_LST_CARRY_MAX_DEPEG_UPPER: f64 = 0.5;

/// NaN fallback for [`ConfigAccess::trade_lst_carry_max_depeg`].
///
/// A poisoned config read must not disable the de-peg guard: a `NaN`
/// tolerance would make every comparison `deviation > tolerance` evaluate to
/// false and silently allow buying into a de-pegged pool. The fallback
/// matches the documented default.
const TRADE_LST_CARRY_MAX_DEPEG_NAN_FALLBACK: f64 = 0.05;

/// Idempotent clamp applied to `trade_lst_carry_max_depeg` reads.
///
/// `NaN` is mapped to the documented default (keeping the guard active)
/// rather than poisoning the comparison. `±INFINITY` is handled by
/// `f64::clamp` itself.
fn clamp_trade_lst_carry_max_depeg(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_LST_CARRY_MAX_DEPEG_NAN_FALLBACK
    } else {
        v.clamp(
            TRADE_LST_CARRY_MAX_DEPEG_LOWER,
            TRADE_LST_CARRY_MAX_DEPEG_UPPER,
        )
    }
}

/// Idempotent clamp applied to `portfolio_cost_iteration_damping` reads.
///
/// `NaN` is mapped to [`PORTFOLIO_COST_ITERATION_DAMPING_NAN_FALLBACK`] so
/// that an injected `NaN` does not propagate into `damp_and_diff` (which
/// would otherwise return `Err`, aborting the cost-aware optimization).
/// `±INFINITY` is handled correctly by `f64::clamp` itself.
fn clamp_portfolio_cost_iteration_damping(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_COST_ITERATION_DAMPING_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_COST_ITERATION_DAMPING_LOWER,
            PORTFOLIO_COST_ITERATION_DAMPING_UPPER,
        )
    }
}

/// Lower bound for [`ConfigAccess::prediction_accuracy_min_samples`].
///
/// `0` would let per-token statistics (`calculate_per_token_bias`,
/// `calculate_per_token_pred_err_variance`) operate on an empty sample slice
/// and divide by zero (NaN) or hit the `compute_median` empty-input guard.
/// At least one sample is required for the aggregations to be defined.
const PREDICTION_ACCURACY_MIN_SAMPLES_LOWER: usize = 1;

/// Idempotent clamp applied to `prediction_accuracy_min_samples` reads.
///
/// `usize` cannot represent negative values or `NaN`, so the only failure
/// mode is `0`, which is mapped up to
/// [`PREDICTION_ACCURACY_MIN_SAMPLES_LOWER`]. This protects the per-token
/// aggregation gates (`if samples.len() < min_samples { continue; }`) from
/// degenerating into "always pass through with zero samples".
fn clamp_min_samples(v: usize) -> usize {
    v.max(PREDICTION_ACCURACY_MIN_SAMPLES_LOWER)
}

/// Lower bound for [`ConfigAccess::trade_max_position_vs_pool_ratio`].
///
/// 0.001 (= 0.1% of pool TVL) is the floor for any nontrivial position; a
/// stricter cap effectively bans every realistic rebalance trade because the
/// trade size on a typical pool always exceeds 1 BPS of TVL.
const TRADE_MAX_POSITION_VS_POOL_RATIO_LOWER: f64 = 0.001;

/// Upper bound for [`ConfigAccess::trade_max_position_vs_pool_ratio`].
///
/// 0.5 (= 50% of pool TVL) is the operational ceiling; beyond that the
/// AMM round-trip price impact crosses the failure regime where deeper
/// curves (e.g. half the reserves moving in one trade) make every retry
/// worse than holding cash.
const TRADE_MAX_POSITION_VS_POOL_RATIO_UPPER: f64 = 0.5;

/// NaN fallback for [`ConfigAccess::trade_max_position_vs_pool_ratio`].
///
/// Mirrors the policy of [`clamp_portfolio_cost_iteration_damping`] — a
/// poisoned config read does not propagate `NaN` into the optimizer; instead
/// we return a conservative value that filters memecoin-sized positions but
/// still allows the typical mainstream-pool rebalance trade.
const TRADE_MAX_POSITION_VS_POOL_RATIO_NAN_FALLBACK: f64 = 0.02;

/// Idempotent clamp applied to `trade_max_position_vs_pool_ratio` reads.
///
/// `f64::clamp` propagates `NaN`, so `NaN` is mapped to the conservative
/// fallback instead of poisoning the cost-aware optimizer's pool-ratio
/// guard. `±INFINITY` is handled correctly by `f64::clamp` itself.
fn clamp_trade_max_position_vs_pool_ratio(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_MAX_POSITION_VS_POOL_RATIO_NAN_FALLBACK
    } else {
        v.clamp(
            TRADE_MAX_POSITION_VS_POOL_RATIO_LOWER,
            TRADE_MAX_POSITION_VS_POOL_RATIO_UPPER,
        )
    }
}

// ── TRADE_ALPHA_GATE_MULTIPLIER ──

/// Lower bound for [`ConfigAccess::trade_alpha_gate_multiplier`].
///
/// `0.1` is the floor: the gate filter `H × ER > k × round_trip_cost`
/// degenerates for `k < 0.1` because even break-even alpha would pass.
const TRADE_ALPHA_GATE_MULTIPLIER_LOWER: f64 = 0.1;

/// Upper bound for [`ConfigAccess::trade_alpha_gate_multiplier`].
///
/// `10.0` is the ceiling: thresholds above 10x round-trip cost reject
/// essentially every realistic alpha and collapse the strategy into
/// permanent Hold; the dedicated `TRADE_ALPHA_GATE_ENABLED` flag should
/// be used to disable the gate instead.
const TRADE_ALPHA_GATE_MULTIPLIER_UPPER: f64 = 10.0;

/// NaN fallback for [`ConfigAccess::trade_alpha_gate_multiplier`].
///
/// A poisoned config read does not propagate `NaN` into the gate
/// comparison `H × ER > k × round_trip_cost` (a `NaN` comparison
/// would always evaluate to false and silently disable the gate).
/// The fallback matches the documented default.
const TRADE_ALPHA_GATE_MULTIPLIER_NAN_FALLBACK: f64 = 2.0;

/// Idempotent clamp applied to `trade_alpha_gate_multiplier` reads.
fn clamp_trade_alpha_gate_multiplier(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_ALPHA_GATE_MULTIPLIER_NAN_FALLBACK
    } else {
        v.clamp(
            TRADE_ALPHA_GATE_MULTIPLIER_LOWER,
            TRADE_ALPHA_GATE_MULTIPLIER_UPPER,
        )
    }
}

/// Lower bound for [`ConfigAccess::trade_dd_threshold`].
///
/// `0.01` (= 1% drawdown) is the floor for any meaningful circuit breaker;
/// thresholds below 1% would fire on routine intraday volatility and turn
/// the breaker into a churn generator.
const TRADE_DD_THRESHOLD_LOWER: f64 = 0.01;

/// Upper bound for [`ConfigAccess::trade_dd_threshold`].
///
/// `0.99` (= 99% drawdown) keeps the breaker at least notionally enabled.
/// `1.0` would mean "only fire when the entire portfolio is gone", which is
/// equivalent to disabling the feature; the dedicated
/// `TRADE_DD_CIRCUIT_BREAKER_ENABLED` flag should be used to disable it.
const TRADE_DD_THRESHOLD_UPPER: f64 = 0.99;

/// NaN fallback for [`ConfigAccess::trade_dd_threshold`].
///
/// A poisoned config read does not propagate `NaN` into the period-end
/// comparison `current/initial < 1 - threshold`. The conservative fallback
/// (15%) matches the documented default and preserves the same semantics
/// as if the operator had not set the variable at all.
const TRADE_DD_THRESHOLD_NAN_FALLBACK: f64 = 0.15;

/// Idempotent clamp applied to `trade_dd_threshold` reads.
///
/// `NaN` is mapped to [`TRADE_DD_THRESHOLD_NAN_FALLBACK`] so that an
/// injected `NaN` does not silently disable the circuit breaker (a `NaN`
/// comparison would always evaluate to false). `±INFINITY` is handled
/// correctly by `f64::clamp` itself.
fn clamp_trade_dd_threshold(v: f64) -> f64 {
    if v.is_nan() {
        TRADE_DD_THRESHOLD_NAN_FALLBACK
    } else {
        v.clamp(TRADE_DD_THRESHOLD_LOWER, TRADE_DD_THRESHOLD_UPPER)
    }
}

// ── PORTFOLIO_VOLATILITY_TARGET (Phase 1: vol targeting) ──

/// Lower bound for [`ConfigAccess::portfolio_volatility_target`].
///
/// `0.005` (= 0.5 %/day, ~8 % annualized) is the floor for any nontrivial
/// vol-targeting signal; below this every realistic crypto-portfolio
/// volatility forces the cap to the absolute floor (10 %) and the signal
/// degenerates into a constant-cash regime.
const PORTFOLIO_VOLATILITY_TARGET_LOWER: f64 = 0.005;

/// Upper bound for [`ConfigAccess::portfolio_volatility_target`].
///
/// `0.05` (= 5 %/day, ~80 % annualized) is the ceiling at which the
/// vol-targeting signal saturates almost always at `cap = 1.0` for typical
/// diversified crypto portfolios; beyond that the signal is effectively
/// disabled regardless of the flag, so we clamp here for a clearer
/// "this is the supported range" error rather than silent no-op.
const PORTFOLIO_VOLATILITY_TARGET_UPPER: f64 = 0.05;

/// NaN fallback for [`ConfigAccess::portfolio_volatility_target`].
const PORTFOLIO_VOLATILITY_TARGET_NAN_FALLBACK: f64 = 0.015;

fn clamp_portfolio_volatility_target(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_VOLATILITY_TARGET_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_VOLATILITY_TARGET_LOWER,
            PORTFOLIO_VOLATILITY_TARGET_UPPER,
        )
    }
}

// ── PORTFOLIO_HALF_KELLY_FRACTION (Phase 3a: half-Kelly) ──

/// Lower bound for [`ConfigAccess::portfolio_half_kelly_fraction`].
///
/// `0.1` (= one-tenth Kelly) is the floor; lower values turn the Kelly cap
/// into a per-asset rounding-down, indistinguishable from the existing
/// `MAX_POSITION_SIZE` cap.
const PORTFOLIO_HALF_KELLY_FRACTION_LOWER: f64 = 0.1;

/// Upper bound for [`ConfigAccess::portfolio_half_kelly_fraction`].
///
/// `0.5` (= half Kelly, the namesake of the module) is the ceiling. Full
/// Kelly is famously fragile to ER estimation noise and a 10 % MAPE on `μ_i`
/// translates to ~100 % error on `f_i`; we keep the operator from accidentally
/// dialing in unstable territory.
const PORTFOLIO_HALF_KELLY_FRACTION_UPPER: f64 = 0.5;

const PORTFOLIO_HALF_KELLY_FRACTION_NAN_FALLBACK: f64 = 0.25;

fn clamp_portfolio_half_kelly_fraction(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_HALF_KELLY_FRACTION_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_HALF_KELLY_FRACTION_LOWER,
            PORTFOLIO_HALF_KELLY_FRACTION_UPPER,
        )
    }
}

// ── PORTFOLIO_STOP_LOSS_THRESHOLD (Phase 3b: per-token stop-loss) ──

/// Lower bound for [`ConfigAccess::portfolio_stop_loss_threshold`].
///
/// `0.05` (= 5 % drawdown) is the floor; tighter thresholds fire on routine
/// intraday volatility (~3-6 %/day for crypto) and turn the stop-loss into
/// a churn generator.
const PORTFOLIO_STOP_LOSS_THRESHOLD_LOWER: f64 = 0.05;

/// Upper bound for [`ConfigAccess::portfolio_stop_loss_threshold`].
///
/// `0.30` (= 30 % drawdown) is the ceiling; beyond that the trigger rarely
/// fires in practice and the stop-loss becomes a silent no-op.
const PORTFOLIO_STOP_LOSS_THRESHOLD_UPPER: f64 = 0.30;

const PORTFOLIO_STOP_LOSS_THRESHOLD_NAN_FALLBACK: f64 = 0.10;

fn clamp_portfolio_stop_loss_threshold(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_STOP_LOSS_THRESHOLD_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_STOP_LOSS_THRESHOLD_LOWER,
            PORTFOLIO_STOP_LOSS_THRESHOLD_UPPER,
        )
    }
}

// ── PORTFOLIO_REGIME_SMA_PERIOD (Phase 2: market breadth) ──

/// Lower bound for [`ConfigAccess::portfolio_regime_sma_period`].
///
/// `5` (= 5 days) is the floor; shorter windows make the breadth indicator
/// indistinguishable from the latest price tick and produce noise rather
/// than signal.
const PORTFOLIO_REGIME_SMA_PERIOD_LOWER: u32 = 5;

/// Upper bound for [`ConfigAccess::portfolio_regime_sma_period`].
///
/// `60` (= 60 days) is the ceiling. Beyond that the simulate window
/// (typically 10-30 days) cannot supply enough history for any token, and
/// every breadth call collapses to the `Neutral` defensive fallback,
/// which is equivalent to disabling the flag.
const PORTFOLIO_REGIME_SMA_PERIOD_UPPER: u32 = 60;

fn clamp_portfolio_regime_sma_period(v: u32) -> u32 {
    v.clamp(
        PORTFOLIO_REGIME_SMA_PERIOD_LOWER,
        PORTFOLIO_REGIME_SMA_PERIOD_UPPER,
    )
}

// ── PORTFOLIO_REGIME_*_EXPOSURE (Phase 2: regime → cap mapping) ──

const PORTFOLIO_REGIME_EXPOSURE_LOWER: f64 = 0.0;
const PORTFOLIO_REGIME_EXPOSURE_UPPER: f64 = 1.0;
const PORTFOLIO_REGIME_BULL_EXPOSURE_NAN_FALLBACK: f64 = 1.0;
const PORTFOLIO_REGIME_NEUTRAL_EXPOSURE_NAN_FALLBACK: f64 = 0.75;
const PORTFOLIO_REGIME_BEAR_EXPOSURE_NAN_FALLBACK: f64 = 0.5;

fn clamp_portfolio_regime_bull_exposure(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_REGIME_BULL_EXPOSURE_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_REGIME_EXPOSURE_LOWER,
            PORTFOLIO_REGIME_EXPOSURE_UPPER,
        )
    }
}

fn clamp_portfolio_regime_neutral_exposure(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_REGIME_NEUTRAL_EXPOSURE_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_REGIME_EXPOSURE_LOWER,
            PORTFOLIO_REGIME_EXPOSURE_UPPER,
        )
    }
}

fn clamp_portfolio_regime_bear_exposure(v: f64) -> f64 {
    if v.is_nan() {
        PORTFOLIO_REGIME_BEAR_EXPOSURE_NAN_FALLBACK
    } else {
        v.clamp(
            PORTFOLIO_REGIME_EXPOSURE_LOWER,
            PORTFOLIO_REGIME_EXPOSURE_UPPER,
        )
    }
}

define_typed_config! {
    // ── trade ──

    /// Whether trading is enabled
    fn trade_enabled() -> bool {
        key: "TRADE_ENABLED",
        default: false
    }

    /// Initial investment amount in NEAR
    fn trade_initial_investment() -> u32 {
        key: "TRADE_INITIAL_INVESTMENT",
        default: 100
    }

    /// Number of top tokens to track
    fn trade_top_tokens() -> u32 {
        key: "TRADE_TOP_TOKENS",
        default: 10
    }

    /// Evaluation period in days
    fn trade_evaluation_days() -> u32 {
        key: "TRADE_EVALUATION_DAYS",
        default: 10
    }

    /// Account reserve in NEAR
    fn trade_account_reserve() -> u32 {
        key: "TRADE_ACCOUNT_RESERVE",
        default: 10
    }

    /// Cron schedule for trade execution
    fn trade_cron_schedule() -> String {
        key: "TRADE_CRON_SCHEDULE",
        default: "0 0 0 * * *"
    }

    /// Cron schedule for rate recording
    fn record_rates_cron_schedule() -> String {
        key: "RECORD_RATES_CRON_SCHEDULE",
        default: "0 */15 * * * *"
    }

    /// Max retries for prediction fetch
    fn trade_prediction_max_retries() -> u32 {
        key: "TRADE_PREDICTION_MAX_RETRIES",
        default: 2
    }

    /// Delay between prediction retries in seconds
    fn trade_prediction_retry_delay_seconds() -> u64 {
        key: "TRADE_PREDICTION_RETRY_DELAY_SECONDS",
        default: 5
    }

    /// Days of price history for predictions and volatility calculation
    fn trade_price_history_days() -> u32 {
        key: "TRADE_PRICE_HISTORY_DAYS",
        default: 30
    }

    /// Whether to unwrap wrap.near on stop
    fn trade_unwrap_on_stop() -> bool {
        key: "TRADE_UNWRAP_ON_STOP",
        default: false
    }

    /// Enable the period-mid drawdown circuit breaker.
    ///
    /// When `true`, `manage_evaluation_period` checks the current portfolio
    /// value against `period.initial_value` on every cycle and forces an
    /// early end-of-period (liquidation + new period) once drawdown exceeds
    /// `trade_dd_threshold`. When `false` (default), the legacy behavior is
    /// preserved: positions are held until the scheduled
    /// `trade_evaluation_days` boundary regardless of intra-period loss.
    fn trade_dd_circuit_breaker_enabled() -> bool {
        key: "TRADE_DD_CIRCUIT_BREAKER_ENABLED",
        default: false
    }

    /// Drawdown threshold (as a fraction of the period's initial value)
    /// above which the circuit breaker fires.
    ///
    /// The trigger condition is `current_value / initial_value < 1 - threshold`,
    /// i.e. a 0.15 threshold fires when the portfolio has lost more than 15%
    /// of its period-start value. Has no effect when
    /// `trade_dd_circuit_breaker_enabled` is `false`.
    fn trade_dd_threshold() -> f64 {
        key: "TRADE_DD_THRESHOLD",
        default: 0.15,
        clamp: clamp_trade_dd_threshold
    }

    /// Phase 1: enable volatility targeting for the aggregate cap.
    ///
    /// When `true`, `compute_vol_target_cap(σ_target, σ_portfolio)` runs
    /// every cycle and contributes a `Volatility(cap)` signal to
    /// `compose_aggregate_cap`. Default `false` (legacy behaviour: no
    /// vol-targeting de-risk).
    fn portfolio_volatility_target_enabled() -> bool {
        key: "PORTFOLIO_VOLATILITY_TARGET_ENABLED",
        default: false
    }

    /// Daily volatility target for `compute_vol_target_cap`.
    ///
    /// `cap = σ_target / σ_portfolio`, so smaller values force more cash.
    /// Default 0.015 (= 1.5 %/day, ~24 % annualized) matches hedge-fund VaR
    /// conventions and is conservative enough that typical crypto
    /// portfolios get meaningful de-risk in volatile regimes without
    /// collapsing to full cash in calm ones. Has no effect when
    /// `portfolio_volatility_target_enabled` is `false`.
    fn portfolio_volatility_target() -> f64 {
        key: "PORTFOLIO_VOLATILITY_TARGET",
        default: 0.015,
        clamp: clamp_portfolio_volatility_target
    }

    /// Phase 2: enable market-breadth regime detection.
    ///
    /// When `true`, `detect_regime_from_prices` runs every cycle and
    /// contributes a `Breadth(MarketRegime::aggregate_cap(scales))` signal
    /// to `compose_aggregate_cap`. Default `false`.
    fn portfolio_regime_breadth_enabled() -> bool {
        key: "PORTFOLIO_REGIME_BREADTH_ENABLED",
        default: false
    }

    /// SMA window length (days) used by the breadth indicator.
    fn portfolio_regime_sma_period() -> u32 {
        key: "PORTFOLIO_REGIME_SMA_PERIOD",
        default: 20,
        clamp: clamp_portfolio_regime_sma_period
    }

    /// Aggregate cap for the `Bull` regime.
    fn portfolio_regime_bull_exposure() -> f64 {
        key: "PORTFOLIO_REGIME_BULL_EXPOSURE",
        default: 1.0,
        clamp: clamp_portfolio_regime_bull_exposure
    }

    /// Aggregate cap for the `Neutral` regime.
    fn portfolio_regime_neutral_exposure() -> f64 {
        key: "PORTFOLIO_REGIME_NEUTRAL_EXPOSURE",
        default: 0.75,
        clamp: clamp_portfolio_regime_neutral_exposure
    }

    /// Aggregate cap for the `Bear` regime.
    fn portfolio_regime_bear_exposure() -> f64 {
        key: "PORTFOLIO_REGIME_BEAR_EXPOSURE",
        default: 0.5,
        clamp: clamp_portfolio_regime_bear_exposure
    }

    /// Phase 3a: enable per-asset half-Kelly upper bound.
    ///
    /// When `true`, `compute_half_kelly_uppers` runs every cycle and the
    /// resulting per-asset uppers tighten `BoxBounds` via
    /// `apply_half_kelly`. Default `false`.
    fn portfolio_half_kelly_enabled() -> bool {
        key: "PORTFOLIO_HALF_KELLY_ENABLED",
        default: false
    }

    /// Kelly fraction for `compute_half_kelly_uppers`.
    ///
    /// Default `0.25` (Quarter Kelly) — Kelly sizing is fragile to ER
    /// estimation noise; 10 % MAPE on `μ_i` translates to ~100 % error on
    /// `f_i = (μ_i - rf) / σ²_i`. A fractional Kelly tames this. Has no
    /// effect when `portfolio_half_kelly_enabled` is `false`.
    fn portfolio_half_kelly_fraction() -> f64 {
        key: "PORTFOLIO_HALF_KELLY_FRACTION",
        default: 0.25,
        clamp: clamp_portfolio_half_kelly_fraction
    }

    /// Phase 3b: enable per-token stop-loss override.
    ///
    /// When `true`, every held token whose realised drawdown exceeds
    /// `portfolio_stop_loss_threshold` gets its per-asset upper forced to
    /// `0` (sell-only). Default `false`.
    fn portfolio_stop_loss_enabled() -> bool {
        key: "PORTFOLIO_STOP_LOSS_ENABLED",
        default: false
    }

    /// Drawdown threshold (as a fraction of entry price) above which the
    /// per-token stop-loss fires.
    ///
    /// Default `0.10` (= 10 % drawdown) matches the ~2σ-event heuristic
    /// for typical 3-6 %/day crypto volatility — tight enough to cap
    /// realised loss without firing on routine intraday swings.
    fn portfolio_stop_loss_threshold() -> f64 {
        key: "PORTFOLIO_STOP_LOSS_THRESHOLD",
        default: 0.10,
        clamp: clamp_portfolio_stop_loss_threshold
    }

    /// Parallel prediction tasks
    fn trade_prediction_concurrency() -> u32 {
        key: "TRADE_PREDICTION_CONCURRENCY",
        default: 4
    }

    /// Number of tokens to process per prediction chunk.
    /// Controls peak memory: each chunk loads chunk_size * ~2335 rows of price history.
    /// Recommended range: 5–50. Smaller values reduce peak memory but increase DB round-trips.
    fn trade_prediction_chunk_size() -> u32 {
        key: "TRADE_PREDICTION_CHUNK_SIZE",
        default: 20
    }

    /// Number of threads for model training pool.
    /// Controls peak memory: each thread can hold one augurs model buffer (~200 MB).
    /// Independent of TRADE_PREDICTION_CONCURRENCY.
    /// Recommended range: 1–8. Higher values increase peak memory proportionally.
    fn trade_prediction_model_threads() -> u32 {
        key: "TRADE_PREDICTION_MODEL_THREADS",
        default: 3
    }

    /// Minimum pool liquidity in NEAR
    fn trade_min_pool_liquidity() -> u32 {
        key: "TRADE_MIN_POOL_LIQUIDITY",
        default: 100
    }

    /// Maximum trade size as a fraction of the smallest pool TVL on the
    /// candidate's swap path. Tokens whose required `|Δw| × total_value`
    /// exceeds this fraction of the bottleneck pool are dropped from the
    /// cost-aware optimizer's candidate set — they have no liquidity-safe
    /// rebalance path at the requested weight, regardless of their
    /// expected return.
    ///
    /// Default 0.02 (= 2%) matches the v3 simulation post-mortem: memecoin
    /// pools at TRADE_MIN_POOL_LIQUIDITY (100 NEAR) had 16 NEAR positions
    /// pushed in (16% of TVL), generating the catastrophic price impact
    /// the cost model was supposed to prevent.
    fn trade_max_position_vs_pool_ratio() -> f64 {
        key: "TRADE_MAX_POSITION_VS_POOL_RATIO",
        default: 0.02,
        clamp: clamp_trade_max_position_vs_pool_ratio
    }

    /// Parallel token cache update tasks
    fn trade_token_cache_concurrency() -> u32 {
        key: "TRADE_TOKEN_CACHE_CONCURRENCY",
        default: 8
    }

    /// Base backoff interval in minutes for failed decimals RPC fetches
    fn trade_token_cache_backoff_base_minutes() -> u64 {
        key: "TRADE_TOKEN_CACHE_BACKOFF_BASE_MINUTES",
        default: 15
    }

    /// Maximum backoff interval in minutes for failed decimals RPC fetches
    fn trade_token_cache_max_backoff_minutes() -> u64 {
        key: "TRADE_TOKEN_CACHE_MAX_BACKOFF_MINUTES",
        default: 1440
    }

    /// Minimum per-token prediction confidence to include in portfolio.
    /// Tokens below this threshold are excluded from trading.
    fn trade_min_token_confidence() -> f64 {
        key: "TRADE_MIN_TOKEN_CONFIDENCE",
        default: 0.3
    }

    /// Apply per-token bias correction to predicted prices before computing expected returns.
    /// When enabled, the median historical bias `(predicted - actual) / actual` is used to
    /// adjust the current prediction via `corrected = predicted / (1 + bias_clamped)`.
    ///
    /// # Default change history
    ///
    /// 2026-04: introduced as `default: false`.
    /// 2026-04 (commit 3da237e): flipped to `default: true` together with
    ///   `PORTFOLIO_PRED_ERR_DIAGONAL_ENABLED` / `TRADE_COST_AWARE_RETURN_ENABLED`.
    /// 2026-05: flipped back to `default: false` because the cost-aware /
    ///   pred-err-diagonal pipeline ships with two known numerical limitations
    ///   (Entry-from-cash exit-token under-pricing, Additive `k=0.1` collapsing
    ///   diversification on low-volatility regimes); operators can re-enable
    ///   per environment via `CONFIG_STORE` / `DB_STORE` / env, but the
    ///   workspace default stays off until follow-up PRs land Δw-based cost
    ///   accounting and correlation-preserving rescaling.
    fn trade_bias_correction_enabled() -> bool {
        key: "TRADE_BIAS_CORRECTION_ENABLED",
        default: false
    }

    /// Use all-token candidate selection per cycle instead of locking in the
    /// top-N volatility tokens at the start of each evaluation period.
    ///
    /// When `false` (default), the legacy behavior is preserved: at the start
    /// of a new evaluation period, `select_top_volatility_tokens` picks the
    /// top `TRADE_TOP_TOKENS` tokens, and those same tokens are used for the
    /// entire period.
    ///
    /// When `true`, each cycle re-evaluates the full candidate set: every
    /// token with a fresh prediction is unioned with the currently held
    /// tokens (so sell-only liquidation paths are always available), and the
    /// portfolio optimizer chooses among them. The set is *not* truncated to
    /// `TRADE_TOP_TOKENS` — the optimizer applies its own bounds and cost
    /// model. Held tokens remain in the candidate set even if their pool
    /// liquidity has fallen below the entry threshold, so they can still be
    /// exited.
    ///
    /// This is gated behind a feature flag so the legacy fixed-set behavior
    /// can be A/B compared against the all-token policy via the `simulate`
    /// crate before it ships as the production default.
    fn trade_all_predicted_enabled() -> bool {
        key: "TRADE_ALL_PREDICTED_ENABLED",
        default: false
    }

    /// Cap on the number of candidates handed to the portfolio optimizer
    /// after the confidence filter, when all-token mode is enabled.
    ///
    /// When `0` (default), no Top-N pruning is applied — every candidate
    /// surviving confidence + liquidity filters reaches the optimizer.
    /// When `> 0`, a composite score
    /// `confidence × liquidity_score × max(0, expected_return)` is computed
    /// per token and only the top N are kept. Currently held tokens are
    /// always included regardless of rank, so existing positions can still
    /// be sold even if their score is low.
    ///
    /// The cap is intended to reduce the optimizer's search space in
    /// all-token mode (~290 candidates) where noise from low-quality
    /// predictions appears to dominate. It is gated as a feature flag
    /// so its effect can be A/B compared via `simulate` before shipping
    /// as a production default.
    ///
    /// Has no effect when `TRADE_ALL_PREDICTED_ENABLED` is `false`.
    fn trade_top_n_after_prediction() -> u32 {
        key: "TRADE_TOP_N_AFTER_PREDICTION",
        default: 0
    }

    /// Enable the alpha gate filter that rejects tokens whose expected
    /// return cannot recoup the round-trip AMM cost.
    ///
    /// When `true`, before the optimizer sees the candidate set the strategy
    /// estimates the round-trip variable cost for a worst-case position
    /// (`total_value × MAX_POSITION_SIZE`) and excludes tokens where
    /// `hold_cycles × expected_return < multiplier × round_trip_cost`.
    /// Held tokens always bypass the gate so existing positions can still
    /// be liquidated. Default `false` (legacy: no gate, optimizer sees
    /// every confidence-filtered token).
    fn trade_alpha_gate_enabled() -> bool {
        key: "TRADE_ALPHA_GATE_ENABLED",
        default: false
    }

    /// Safety multiplier `k` applied to the round-trip cost in the alpha
    /// gate comparison `H × ER > k × round_trip_cost`.
    ///
    /// `k = 2.0` (default) requires alpha at least 2× the estimated
    /// round-trip cost before a token is allowed into the optimizer. This
    /// is the "safety margin" axis; the holding-period axis lives in
    /// [`ConfigAccess::trade_alpha_gate_hold_cycles`]. Has no effect when
    /// `trade_alpha_gate_enabled` is `false`.
    fn trade_alpha_gate_multiplier() -> f64 {
        key: "TRADE_ALPHA_GATE_MULTIPLIER",
        default: 2.0,
        clamp: clamp_trade_alpha_gate_multiplier
    }

    /// Number of cycles the strategy expects to hold a position before
    /// closing it, used in the alpha gate comparison
    /// `H × ER > k × round_trip_cost`.
    ///
    /// `H = 1` (default) is the conservative single-cycle round trip
    /// assumption — alpha must recoup the full cost on the *next* cycle.
    /// Larger values amortize the cost across multiple cycles and lower
    /// the effective gate threshold. The `(1..=100)` clamp range covers
    /// daily-rebalance horizons from one day to ~3 months. Has no effect
    /// when `trade_alpha_gate_enabled` is `false`.
    fn trade_alpha_gate_hold_cycles() -> u32 {
        key: "TRADE_ALPHA_GATE_HOLD_CYCLES",
        default: 1
    }

    /// Minimum number of tokens that must reach the optimizer after the
    /// alpha gate filter.
    ///
    /// When the gate rejects so many tokens that fewer than this many
    /// remain, the strategy supplies missing slots from the rejected
    /// tokens ranked by composite score (the same score used by
    /// `TRADE_TOP_N_AFTER_PREDICTION`). This prevents the Markowitz
    /// optimizer from collapsing to a single-token corner solution when
    /// the gate is too strict. `0` disables the fallback (gate decisions
    /// are final). Default `5`. Has no effect when
    /// `trade_alpha_gate_enabled` is `false`.
    fn trade_alpha_gate_min_pass_count() -> u32 {
        key: "TRADE_ALPHA_GATE_MIN_PASS_COUNT",
        default: 5
    }

    /// Soft-threshold shrinkage strength applied to per-token expected returns.
    ///
    /// The optimizer's `expected_return` for each token is adjusted to
    /// `sign(μ) × max(0, |μ| - λ × √MSRE)` where `λ` is this value and
    /// `MSRE = mean((mape/100)²)` is the per-token prediction error
    /// (`calculate_per_token_pred_err_variance`). The form preserves the sign
    /// of the original return, dampens magnitudes toward zero in proportion
    /// to prediction uncertainty, and bounds `|μ_adj| ≤ |μ|`.
    ///
    /// Defaults to `0.0` (no shrinkage, identical to the legacy behavior).
    /// Production-recommended range is roughly `[0.05, 0.3]`; the upper
    /// clamp bound is `1.0` because beyond that point typical signals
    /// (≈3 % return) are fully nulled by typical prediction error
    /// (≈10 % MAPE → √MSRE ≈ 0.1).
    ///
    /// ## Interaction with `PORTFOLIO_PRED_ERR_DIAGONAL_ENABLED`
    ///
    /// Both flags address prediction uncertainty: this shrinks `μ` in the
    /// optimizer numerator while `PORTFOLIO_PRED_ERR_DIAGONAL_*` inflates
    /// `Σ` in the denominator. Enabling both simultaneously produces a
    /// super-linear joint effect against high-MSRE tokens that may be too
    /// aggressive. Treat them as alternatives in production until A/B
    /// evidence justifies stacking.
    fn trade_prediction_shrinkage_lambda() -> f64 {
        key: "TRADE_PREDICTION_SHRINKAGE_LAMBDA",
        default: 0.0,
        clamp: clamp_trade_prediction_shrinkage_lambda
    }

    /// Maximum AMM price impact (depth slippage) tolerated for a single swap.
    ///
    /// Before executing a swap the strategy compares the route's effective
    /// rate at the full trade size against its marginal rate at a tiny
    /// reference size (`execution_guard::price_impact_ratio`). When the
    /// resulting impact exceeds this threshold the swap is skipped instead of
    /// executed, because such routes are dominated by a thin or stale pool
    /// and would convert most of the input into slippage (real cycles up to
    /// 97 % impact were observed against dead pools).
    ///
    /// This is independent of `SlippagePolicy` / `min_out`: `min_out` only
    /// caps *additional* slippage beyond the (already bad) estimated output,
    /// and is `0` for `Unprotected` liquidation swaps, so it does not block
    /// entry into a thin-pool route. The guard applies to every swap
    /// regardless of policy.
    ///
    /// Defaults to `0.5` (50 %). A backtest sweep (block 2026-06-04..06-15)
    /// showed that `0.03` (3 %) blocks routine thin-pool meme swaps — normal
    /// executed impact for these tokens runs 6–37 % — which only churns the
    /// portfolio (6 → 17 swaps) and marginally worsens return (-6.29 % →
    /// -6.54 %) without avoiding any catastrophe. At `0.5` the guard is
    /// return-neutral versus disabled in normal windows while still blocking
    /// the catastrophic dead-pool routes (observed up to 97 %) it exists for.
    /// The `[0.005, 0.95]` clamp keeps the guard from degenerating into
    /// permanent Hold (too low) or a no-op (too high).
    fn trade_max_price_impact() -> f64 {
        key: "TRADE_MAX_PRICE_IMPACT",
        default: 0.5,
        clamp: clamp_trade_max_price_impact
    }

    /// Enable the Tier-1 liquid-staking carry strategy.
    ///
    /// When `true`, the trade engine runs a dedicated low-turnover mode that
    /// buys-and-holds an equal weight of the liquid-staking tokens (LiNEAR,
    /// stNEAR) to capture their structural ~4 %/yr appreciation against NEAR,
    /// bypassing the volatility-portfolio pipeline (prediction, CoV ranking,
    /// alpha gate, Markowitz optimizer) entirely. The legacy/all-predicted
    /// modes are untouched when this is `false` (the default), so the carry
    /// mode can be A/B compared via `simulate` before shipping.
    fn trade_lst_carry_enabled() -> bool {
        key: "TRADE_LST_CARRY_ENABLED",
        default: false
    }

    /// Minimum holding horizon (in days) for the liquid-staking carry mode.
    ///
    /// The carry backtest showed positive returns only for holds of at least
    /// 30 days (shorter windows lose to rate noise), so this is both the
    /// floor of the `[30, 90]` clamp and the default. The value is propagated
    /// into the evaluation-period length so the period machinery does not
    /// force-liquidate the position before the hold completes; it also bounds
    /// the forced-liquidation fee drag at period boundaries
    /// (`round_trip_cost × 365 / N`, which must stay small relative to the
    /// carry). Has no effect when `trade_lst_carry_enabled` is `false`.
    fn trade_lst_carry_min_hold_days() -> u32 {
        key: "TRADE_LST_CARRY_MIN_HOLD_DAYS",
        default: 30,
        clamp: clamp_trade_lst_carry_min_hold_days
    }

    /// Maximum tolerated de-peg deviation for a liquid-staking token before
    /// the carry mode refuses to buy it.
    ///
    /// Because the carry mode bypasses the optimizer and CoV ranking, it
    /// loses their implicit protection against distorted exchange rates. This
    /// guard re-introduces a sanity bound: when an LST's observed rate
    /// deviates from its expected (slowly, monotonically drifting) value by
    /// more than this fraction — the signature of a liquidity-crisis de-peg —
    /// the token is held/skipped rather than bought into. Defaults to `0.05`
    /// (5 %); the `[0.01, 0.5]` clamp keeps it from collapsing into permanent
    /// Hold (too low) or letting a genuine de-peg through (too high). Has no
    /// effect when `trade_lst_carry_enabled` is `false`.
    fn trade_lst_carry_max_depeg() -> f64 {
        key: "TRADE_LST_CARRY_MAX_DEPEG",
        default: 0.05,
        clamp: clamp_trade_lst_carry_max_depeg
    }

    // ── arbitrage ──

    /// Whether arbitrage engine is enabled
    fn arbitrage_needed() -> bool {
        key: "ARBITRAGE_NEEDED",
        default: false
    }

    /// Wait duration when token not found
    fn arbitrage_token_not_found_wait() -> Duration {
        key: "ARBITRAGE_TOKEN_NOT_FOUND_WAIT",
        default: Duration::from_secs(1)
    }

    /// Wait duration on other errors
    fn arbitrage_other_error_wait() -> Duration {
        key: "ARBITRAGE_OTHER_ERROR_WAIT",
        default: Duration::from_secs(5)
    }

    /// Wait duration when preview not found
    fn arbitrage_preview_not_found_wait() -> Duration {
        key: "ARBITRAGE_PREVIEW_NOT_FOUND_WAIT",
        default: Duration::from_secs(2)
    }

    // ── harvest ──

    /// Harvest destination account ID (required)
    fn harvest_account_id() -> anyhow::Result<String> {
        key: "HARVEST_ACCOUNT_ID",
        default: ()
    }

    /// Minimum NEAR to trigger harvest
    fn harvest_min_amount() -> u32 {
        key: "HARVEST_MIN_AMOUNT",
        default: 10
    }

    /// NEAR to keep in account when harvesting
    fn harvest_reserve_amount() -> u32 {
        key: "HARVEST_RESERVE_AMOUNT",
        default: 1
    }

    /// Interval between harvests in seconds
    fn harvest_interval_seconds() -> u64 {
        key: "HARVEST_INTERVAL_SECONDS",
        default: 86400
    }

    /// Multiplier for harvest balance calculation
    fn harvest_balance_multiplier() -> u128 {
        key: "HARVEST_BALANCE_MULTIPLIER",
        default: 128
    }

    // ── rpc ──

    /// Max RPC retry attempts
    fn rpc_max_attempts() -> u16 {
        key: "RPC_MAX_ATTEMPTS",
        default: 128
    }

    // ── cron ──

    /// Retention period for pool info records in days
    fn pool_info_retention_days() -> u32 {
        key: "POOL_INFO_RETENTION_DAYS",
        default: 30
    }

    /// Retention period for token rate records in days
    fn token_rates_retention_days() -> u32 {
        key: "TOKEN_RATES_RETENTION_DAYS",
        default: 90
    }

    /// Retention period for evaluation period records in days
    /// (ON DELETE CASCADE also removes related trade_transactions and portfolio_holdings)
    fn evaluation_periods_retention_days() -> u32 {
        key: "EVALUATION_PERIODS_RETENTION_DAYS",
        default: 365
    }

    /// Retention period for config store history records in days
    fn config_store_history_retention_days() -> u32 {
        key: "CONFIG_STORE_HISTORY_RETENTION_DAYS",
        default: 365
    }

    /// Max sleep duration in cron loop in seconds
    fn cron_max_sleep_seconds() -> u64 {
        key: "CRON_MAX_SLEEP_SECONDS",
        default: 60
    }

    /// Log threshold for long waits in seconds
    fn cron_log_threshold_seconds() -> u64 {
        key: "CRON_LOG_THRESHOLD_SECONDS",
        default: 300
    }

    /// Cron schedule for database maintenance (REINDEX)
    fn db_maintenance_cron_schedule() -> String {
        key: "DB_MAINTENANCE_CRON_SCHEDULE",
        default: "0 0 4 * * 7"
    }

    // ── wallet / logging: moved to StartupConfig ──

    // ── portfolio/liquidity ──

    /// Portfolio rebalance trigger threshold
    fn portfolio_rebalance_threshold() -> f64 {
        key: "PORTFOLIO_REBALANCE_THRESHOLD",
        default: 0.1
    }

    /// Inflate covariance diagonal with prediction error variance per token.
    /// When enabled, the optimizer's risk evaluation incorporates per-token
    /// prediction accuracy (high MAPE → higher diagonal → smaller weight).
    ///
    /// # Default change history
    ///
    /// See `trade_bias_correction_enabled` for the rationale; this flag was
    /// flipped together with the bias-correction and cost-aware-return flags
    /// in 2026-04 (commit 3da237e) and reverted to `false` in 2026-05.
    fn portfolio_pred_err_diagonal_enabled() -> bool {
        key: "PORTFOLIO_PRED_ERR_DIAGONAL_ENABLED",
        default: false
    }

    /// Scale factor `k` applied to prediction error variance in the diagonal
    /// inflation rule (additive: `cov[i,i] + k * pred_err_var`,
    /// max: `max(cov[i,i], k * pred_err_var)`).
    ///
    /// Default is `0.1` — `pred_err_var` is on the same return scale as
    /// `cov[i,i]` but typical MAPE 20% gives `pev = 0.04` which is ~100x
    /// the daily price variance (~10⁻⁴). `k=0.1` keeps the inflation in
    /// a comparable order of magnitude. See `apply_prediction_error_diagonal`
    /// docstring for the correlation-distortion caveat.
    ///
    /// **Defense-in-depth (F016)**: clamped to
    /// `[PORTFOLIO_PRED_ERR_DIAGONAL_K_LOWER, PORTFOLIO_PRED_ERR_DIAGONAL_K_UPPER]`
    /// (currently `[0.0, 100.0]`) at the read boundary; `NaN` is mapped to
    /// the lower bound.
    fn portfolio_pred_err_diagonal_k() -> f64 {
        key: "PORTFOLIO_PRED_ERR_DIAGONAL_K",
        default: 0.1,
        clamp: clamp_portfolio_pred_err_diagonal_k
    }

    /// Diagonal composition mode for prediction error variance.
    ///
    /// Accepts `"additive"` or `"max"` (case-insensitive). Invalid values
    /// trigger a startup `panic!` rather than silent fallback (F007:
    /// preventing typo-induced silent regression to a different mode).
    /// Default is `Additive` — see `apply_prediction_error_diagonal`
    /// docstring for the financial reasoning.
    fn portfolio_pred_err_diagonal_mode() -> crate::algorithm::portfolio::PredErrDiagonalMode {
        key: "PORTFOLIO_PRED_ERR_DIAGONAL_MODE",
        default: crate::algorithm::portfolio::PredErrDiagonalMode::Additive
    }

    /// Deduct AMM fee + price impact + gas + storage + slippage from expected return
    /// before optimization, and run iterative optimization to converge weight↔cost.
    ///
    /// # Default change history
    ///
    /// See `trade_bias_correction_enabled` for the rationale; this flag was
    /// flipped together with the bias-correction and pred-err-diagonal flags
    /// in 2026-04 (commit 3da237e) and reverted to `false` in 2026-05.
    fn trade_cost_aware_return_enabled() -> bool {
        key: "TRADE_COST_AWARE_RETURN_ENABLED",
        default: false
    }

    /// Maximum iterations for the cost-aware optimization loop.
    /// On non-convergence, the last iterate is used.
    ///
    /// **Defense-in-depth (F016)**: clamped to
    /// `[PORTFOLIO_COST_ITERATIONS_MAX_LOWER, PORTFOLIO_COST_ITERATIONS_MAX_UPPER]`
    /// (currently `[1, 10]`) at the read boundary so that an injected
    /// `u32::MAX` cannot stall the cron loop.
    fn portfolio_cost_iterations_max() -> u32 {
        key: "PORTFOLIO_COST_ITERATIONS_MAX",
        default: 3,
        clamp: clamp_portfolio_cost_iterations_max
    }

    /// Damping factor α for the iterative cost-aware optimization
    /// (`next = (1 - α) × prev + α × new`). Lower values dampen oscillation.
    ///
    /// **Defense-in-depth (F003)**: clamped to
    /// `[PORTFOLIO_COST_ITERATION_DAMPING_LOWER, PORTFOLIO_COST_ITERATION_DAMPING_UPPER]`
    /// (currently `[0.1, 1.0]`) at the read boundary. The lower bound is `0.1`
    /// rather than `0.0` to block the silent disable mode where `α = 0` (or
    /// `α ∈ (0, 0.1)`) freezes — or barely advances — the iterate so that
    /// `cost_deductions` never feed back into the optimizer; see
    /// [`PORTFOLIO_COST_ITERATION_DAMPING_LOWER`] for the full attack mechanism.
    /// `NaN` is mapped to [`PORTFOLIO_COST_ITERATION_DAMPING_NAN_FALLBACK`]
    /// (`0.5`) so an injected non-finite value does not abort the optimization
    /// in `damp_and_diff`.
    fn portfolio_cost_iteration_damping() -> f64 {
        key: "PORTFOLIO_COST_ITERATION_DAMPING",
        default: 0.5,
        clamp: clamp_portfolio_cost_iteration_damping
    }

    /// Weight for volume-based liquidity score
    fn liquidity_volume_weight() -> f64 {
        key: "LIQUIDITY_VOLUME_WEIGHT",
        default: 0.6
    }

    /// Weight for pool-based liquidity score
    fn liquidity_pool_weight() -> f64 {
        key: "LIQUIDITY_POOL_WEIGHT",
        default: 0.4
    }

    /// Default liquidity score on error
    fn liquidity_error_default_score() -> f64 {
        key: "LIQUIDITY_ERROR_DEFAULT_SCORE",
        default: 0.3
    }

    // ── prediction ──

    /// Retention days for evaluated prediction records
    fn prediction_record_retention_days() -> u32 {
        key: "PREDICTION_RECORD_RETENTION_DAYS",
        default: 30
    }

    /// Retention days for unevaluated predictions
    fn prediction_unevaluated_retention_days() -> u32 {
        key: "PREDICTION_UNEVALUATED_RETENTION_DAYS",
        default: 20
    }

    /// Time tolerance for prediction evaluation in minutes
    fn prediction_eval_tolerance_minutes() -> i64 {
        key: "PREDICTION_EVAL_TOLERANCE_MINUTES",
        default: 30
    }

    /// Window size for accuracy calculation
    fn prediction_accuracy_window() -> i64 {
        key: "PREDICTION_ACCURACY_WINDOW",
        default: 20
    }

    /// Min samples needed for accuracy evaluation.
    ///
    /// **Defense-in-depth (F004)**: clamped to
    /// `[PREDICTION_ACCURACY_MIN_SAMPLES_LOWER, usize::MAX]` (currently
    /// `[1, usize::MAX]`) at the read boundary. `0` is mapped up to `1` so
    /// that the per-token aggregation gates (`if samples.len() < min_samples
    /// { continue; }`) cannot pass through with an empty sample slice and
    /// divide by zero (NaN) or hit the `compute_median` empty-input guard.
    fn prediction_accuracy_min_samples() -> usize {
        key: "PREDICTION_ACCURACY_MIN_SAMPLES",
        default: 5,
        clamp: clamp_min_samples
    }

    /// MAPE threshold for excellent predictions
    fn prediction_mape_excellent() -> f64 {
        key: "PREDICTION_MAPE_EXCELLENT",
        default: 3.0
    }

    /// MAPE threshold for poor predictions
    fn prediction_mape_poor() -> f64 {
        key: "PREDICTION_MAPE_POOR",
        default: 15.0
    }

    // ── ref-finance storage ──

    /// Maximum auto top-up amount per cycle for REF Finance storage (in yoctoNEAR).
    /// Default: 0.5 NEAR = 500_000_000_000_000_000_000_000
    ///
    /// **Absolute ceiling**: regardless of the configured value (env, TOML, DB_STORE,
    /// CONFIG_STORE), the effective top-up per cycle is capped at
    /// [`REF_STORAGE_MAX_TOP_UP_ABSOLUTE_CEILING`] (= 5 NEAR). This exists to
    /// defend against cap bypass via DB write privilege compromise; see the
    /// constant's doc for the threat model.
    fn ref_storage_max_top_up_yoctonear() -> u128 {
        key: "REF_STORAGE_MAX_TOP_UP_YOCTONEAR",
        default: 500_000_000_000_000_000_000_000
    }

    // ── persistence: database_url, pg_pool_size, instance_id moved to StartupConfig ──
}

/// DB_STORE への load 前に DB 由来の typed config 値を検証する (Layer 0)。
///
/// `persistence::config_store::reload_to_config` から呼ばれ、enum 系 typed config
/// などで `parse` 失敗する不正値を `DB_STORE` に流入させずに除外する。除外された
/// 値は呼び出し側で slog `error!` ログとして出力されるため、運用検知が遅れない。
///
/// `common` クレートは `logging` に循環依存できないため、structured log 化は
/// persistence 側に任せる設計 (本関数は何が無効だったかを `Vec<(key, reason)>`
/// で返すだけ)。
///
/// `configs` は不正値を除外した状態に書き換える (caller の `load_db_config` 直前
/// で呼ぶ前提)。戻り値は `(key, reason)` のリストで、log 出力用。`reason` は
/// 攻撃者制御値を含まず、期待形式の説明のみ (log forwarding 経由漏洩防御)。
///
/// # 汎用化の余地 (follow-up)
///
/// 本関数は **DB 由来の値のみ**を対象とする。CONFIG_STORE / env / TOML 由来の
/// 値の検証は `resolve_without_db` (env/TOML) の startup panic で部分的に
/// カバーされるが、CONFIG_STORE は現時点で `#[doc(hidden)]` test-only API のみ
/// で書き込まれる trusted source として許容している。将来 admin API / gRPC
/// endpoint で外部書き込みを許す場合は本関数を `validate_all_configs` に
/// 汎用化し、全ストアを同時検証すること (security CRITICAL の follow-up)。
pub fn validate_db_configs(
    configs: &mut std::collections::HashMap<std::string::String, std::string::String>,
) -> Vec<(std::string::String, std::string::String)> {
    let mut invalid = Vec::new();

    // PORTFOLIO_PRED_ERR_DIAGONAL_MODE: enum 型の typo / 未対応バリアントを排除。
    // ここを通さず DB_STORE に load すると、`PredErrDiagonalMode::resolve` の
    // silent default fallback (Layer 2) が cron tick 毎に発火し、運用上は
    // observable な signal がないまま fallback 動作を続ける状態になる。
    const KEY_PRED_ERR_MODE: &str = "PORTFOLIO_PRED_ERR_DIAGONAL_MODE";
    if let Some(v) = configs.get(KEY_PRED_ERR_MODE)
        && let Err(reason) =
            <crate::algorithm::portfolio::PredErrDiagonalMode as ConfigResolve>::validate_string(v)
    {
        invalid.push((KEY_PRED_ERR_MODE.to_string(), reason));
    }

    for (k, _) in &invalid {
        configs.remove(k);
    }
    invalid
}

/// Hard-coded absolute ceiling for REF Finance storage auto top-up per cycle.
///
/// Value: 5 NEAR = 10× the default (0.5 NEAR).
///
/// # Why a hard-coded ceiling
///
/// The configured `REF_STORAGE_MAX_TOP_UP_YOCTONEAR` is resolved through the
/// priority chain `CONFIG_STORE > DB_STORE > env > defaults`. `DB_STORE` is
/// populated from the `config_store` table — if DB write privilege is
/// compromised, an attacker can inject an extreme value and effectively
/// disable the cap enforced by `ensure_ref_storage_setup` step 5.
///
/// This constant is applied in code (not in config) so it cannot be overridden
/// through any of the config sources. Raising the ceiling requires a code
/// change + redeploy.
///
/// # Where the clip happens
///
/// `blockchain::ref_finance::storage::max_top_up_from_config` applies
/// `configured.min(CEILING)` on every resolution. When the clip engages
/// (configured > CEILING) a `warn!` record is emitted — silent clips are
/// forbidden so that attempted bypasses leave an audit trail.
pub const REF_STORAGE_MAX_TOP_UP_ABSOLUTE_CEILING: u128 = 5_000_000_000_000_000_000_000_000;

static TYPED: LazyLock<ConfigResolver> = LazyLock::new(|| ConfigResolver);

/// Returns a reference to the global typed config resolver.
///
/// Each accessor resolves the value at call time through the priority chain
/// (CONFIG_STORE > DB_STORE > env > defaults).
pub fn typed() -> &'static ConfigResolver {
    &TYPED
}

#[cfg(test)]
mod tests;
