//! Parsing of NearBlocks v3 responses into domain values.
//!
//! Two inputs:
//!
//! - **List items** (`/v3/accounts/v2.ref-finance.near/receipts`): one
//!   object per receipt, no arguments. [`parse_list_item`] keeps the
//!   `swap` / `ft_on_transfer` receipts of the Ref contract as
//!   [`RefReceipt`] values and drops everything else.
//! - **Transaction receipt trees** (`/v3/txns/{hash}/receipts`): nested
//!   receipts with arguments, outcome and logs. [`parse_tx_receipts`]
//!   walks the tree and reconstructs one [`Trade`] per successful Ref
//!   receipt.
//!
//! Amounts come from the contract's own `Swapped {in} {token_in} for
//! {out} {token_out}` log line, one per swap action, never from JSON
//! numbers (serde_json rounds anything above `u64` to `f64`) and never
//! from `outcome.result` (which belongs to the last action of a batch and
//! is absent for routes that unwrap NEAR). Everything NearBlocks sends is
//! untrusted: strings are length-capped, counts are bounded and any
//! inconsistency rejects the receipt instead of guessing.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, NaiveDateTime};
use common::types::{AccountId, ReceiptId, TokenAccount, TokenSmallestUnits, TxHash};
use dex::ref_trade::{MAX_HOPS, TradeParts};
use dex::{Hop, RouteShape, Trade, TradeError, TradeRoute};
use persistence::ref_flow::receipt::{RefReceipt, RefReceiptError, RefReceiptParts};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use thiserror::Error;

/// The contract whose receipts are collected.
pub const REF_CONTRACT: &str = "v2.ref-finance.near";
const SWAP_METHOD: &str = "swap";
const FT_METHOD: &str = "ft_on_transfer";

/// Bounds on untrusted input.
pub const MAX_RECEIPTS_PER_TX: usize = 10_000;
pub const MAX_LOG_LINES: usize = 1_000;
pub const MAX_LOG_LINE_LEN: usize = 1_024;
pub const MAX_MSG_LEN: usize = 65_536;
const MAX_AMOUNT_DIGITS: usize = 39;

fn u64_from_str_or_num<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Num(u64),
        Text(String),
    }
    match Raw::deserialize(d)? {
        Raw::Num(n) => Ok(n),
        Raw::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Block {
    #[serde(deserialize_with = "u64_from_str_or_num")]
    block_height: u64,
    #[serde(deserialize_with = "u64_from_str_or_num")]
    block_timestamp: u64,
}

impl Block {
    fn time(&self) -> Option<NaiveDateTime> {
        let secs = i64::try_from(self.block_timestamp / 1_000_000_000).ok()?;
        let nanos = u32::try_from(self.block_timestamp % 1_000_000_000).ok()?;
        DateTime::from_timestamp(secs, nanos).map(|t| t.naive_utc())
    }
}

// ---------------------------------------------------------------- list

/// Block timestamp (ns) of a list item, from `block.block_timestamp` or
/// the flat `included_in_block_timestamp`. Used for cursor bookkeeping
/// before the item is fully parsed.
pub fn list_item_timestamp_ns(value: &Value) -> Option<u64> {
    let raw = value
        .get("block")
        .and_then(|b| b.get("block_timestamp"))
        .or_else(|| value.get("included_in_block_timestamp"))?;
    match raw {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct ListItem {
    receipt_id: String,
    transaction_hash: String,
    predecessor_account_id: String,
    receiver_account_id: String,
    #[serde(default)]
    actions: Vec<ListAction>,
    #[serde(default)]
    outcome: ListOutcome,
    block: Block,
}

#[derive(Debug, Deserialize)]
struct ListAction {
    method: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ListOutcome {
    #[serde(default)]
    status: bool,
}

#[derive(Debug, Error)]
pub enum ListParseError {
    #[error("list item is not the expected shape: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("list item has an invalid {field}: {source}")]
    Invalid {
        field: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("list item block_timestamp is out of range")]
    Timestamp,
    #[error(transparent)]
    Receipt(#[from] RefReceiptError),
}

fn invalid(field: &'static str, e: impl Into<anyhow::Error>) -> ListParseError {
    ListParseError::Invalid {
        field,
        source: e.into(),
    }
}

/// Convert one list item. `Ok(None)` for receipts that are not swap /
/// ft_on_transfer calls on the Ref contract (callbacks, transfers, other
/// receivers): these are listed by the API but never stored.
pub fn parse_list_item(value: &Value) -> Result<Option<RefReceipt>, ListParseError> {
    let item: ListItem = serde_json::from_value(value.clone())?;
    if item.receiver_account_id != REF_CONTRACT {
        return Ok(None);
    }
    let methods: Vec<String> = item.actions.into_iter().filter_map(|a| a.method).collect();
    if !methods.iter().any(|m| m == SWAP_METHOD || m == FT_METHOD) {
        return Ok(None);
    }
    let block_time = item.block.time().ok_or(ListParseError::Timestamp)?;
    let receipt = RefReceipt::new(RefReceiptParts {
        receipt_id: ReceiptId::new(&item.receipt_id).map_err(|e| invalid("receipt_id", e))?,
        tx_hash: TxHash::new(&item.transaction_hash).map_err(|e| invalid("transaction_hash", e))?,
        block_height: item.block.block_height,
        block_time,
        predecessor: AccountId::from_str(&item.predecessor_account_id)
            .map_err(|e| invalid("predecessor_account_id", e))?,
        methods,
        ok: item.outcome.status,
    })?;
    Ok(Some(receipt))
}

// -------------------------------------------------------------- enrich

#[derive(Debug, Deserialize)]
struct EnrichReceipt {
    receipt_id: String,
    predecessor_account_id: String,
    receiver_account_id: String,
    #[serde(default)]
    actions: Vec<EnrichAction>,
    #[serde(default)]
    outcome: EnrichOutcome,
    block: Option<Block>,
    #[serde(default)]
    receipts: Vec<EnrichReceipt>,
}

#[derive(Debug, Deserialize)]
struct EnrichAction {
    method: Option<String>,
    args: Option<EnrichArgs>,
}

#[derive(Debug, Deserialize)]
struct EnrichArgs {
    args_json: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
struct EnrichOutcome {
    #[serde(default)]
    status: bool,
    #[serde(default)]
    logs: Vec<String>,
    /// Base64 JSON on success; an error object on failure.
    result: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SwapArgs {
    #[serde(default)]
    actions: Vec<SwapAction>,
}

#[derive(Debug, Deserialize)]
struct SwapAction {
    pool_id: u64,
    token_in: String,
    token_out: String,
}

#[derive(Debug, Deserialize)]
struct FtArgs {
    sender_id: String,
    msg: String,
}

/// Why a Ref receipt yielded no trade. Each variant maps to a counter in
/// [`ParseStats`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TradeRejectReason {
    #[error("no swap actions in the receipt")]
    NoActions,
    #[error("{0} swap actions exceed the {MAX_HOPS} allowed")]
    TooManyActions(usize),
    #[error("arguments are not the expected shape: {0}")]
    MalformedArgs(String),
    #[error("ft_on_transfer msg is not a JSON swap message")]
    MalformedMsg,
    #[error("ft_on_transfer msg is {0} bytes, longer than the {MAX_MSG_LEN} allowed")]
    MsgTooLong(usize),
    #[error("ft_on_transfer refunded {0} (nothing or only part was swapped)")]
    FtRefund(String),
    #[error("ft_on_transfer result is missing or not a number")]
    MissingRefund,
    #[error("{logs} Swapped logs for {actions} swap actions")]
    LogCountMismatch { actions: usize, logs: usize },
    #[error("{0} log lines exceed the {MAX_LOG_LINES} allowed")]
    TooManyLogs(usize),
    #[error("malformed Swapped log: {0:?}")]
    MalformedLog(String),
    #[error("Swapped log {index} names other tokens than the swap action")]
    LogTokenMismatch { index: usize },
    #[error("invalid token account {0:?}")]
    InvalidToken(String),
    #[error("invalid sender account {0:?}")]
    InvalidSender(String),
    #[error("invalid receipt id {0:?}")]
    InvalidReceiptId(String),
    #[error("receipt has no block time")]
    MissingBlock,
    #[error(transparent)]
    Trade(#[from] TradeError),
}

impl TradeRejectReason {
    /// Stable counter key.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NoActions => "no_actions",
            Self::TooManyActions(_) => "too_many_actions",
            Self::MalformedArgs(_) => "malformed_args",
            Self::MalformedMsg => "malformed_msg",
            Self::MsgTooLong(_) => "msg_too_long",
            Self::FtRefund(_) => "ft_refund",
            Self::MissingRefund => "missing_refund",
            Self::LogCountMismatch { .. } => "log_count_mismatch",
            Self::TooManyLogs(_) => "too_many_logs",
            Self::MalformedLog(_) => "malformed_log",
            Self::LogTokenMismatch { .. } => "log_token_mismatch",
            Self::InvalidToken(_) => "invalid_token",
            Self::InvalidSender(_) => "invalid_sender",
            Self::InvalidReceiptId(_) => "invalid_receipt_id",
            Self::MissingBlock => "missing_block",
            Self::Trade(_) => "trade_invariant",
        }
    }
}

/// A Ref receipt that produced no trade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject {
    /// Raw receipt id as sent by the API (may itself be invalid).
    pub receipt_id: String,
    pub reason: TradeRejectReason,
}

/// Counters for one parsed transaction; serialised into
/// `ref_flow_runs.parse_stats` after aggregation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// Receipts in the tree addressed to the Ref contract.
    pub ref_receipts: u32,
    /// Of those, receipts whose outcome failed (nothing was swapped).
    pub failed_receipts: u32,
    /// Ref receipts with no swap / ft_on_transfer action (liquidity,
    /// callbacks).
    pub other_receipts: u32,
    pub accepted: u32,
    /// Ref receipt ids seen more than once in the tree.
    pub duplicate_receipts: u32,
    pub rejects: BTreeMap<&'static str, u32>,
}

impl ParseStats {
    pub fn merge(&mut self, other: &ParseStats) {
        self.ref_receipts += other.ref_receipts;
        self.failed_receipts += other.failed_receipts;
        self.other_receipts += other.other_receipts;
        self.accepted += other.accepted;
        self.duplicate_receipts += other.duplicate_receipts;
        for (k, v) in &other.rejects {
            *self.rejects.entry(k).or_insert(0) += v;
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "ref_receipts": self.ref_receipts,
            "failed_receipts": self.failed_receipts,
            "other_receipts": self.other_receipts,
            "accepted": self.accepted,
            "duplicate_receipts": self.duplicate_receipts,
            "rejects": self.rejects,
        })
    }
}

/// Result of parsing every top-level receipt of one transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedTx {
    pub trades: Vec<Trade>,
    pub rejects: Vec<Reject>,
    pub stats: ParseStats,
}

#[derive(Debug, Error)]
pub enum TxParseError {
    #[error("receipt tree is not the expected shape: {0}")]
    Shape(#[from] serde_json::Error),
    #[error("receipt tree has more than {MAX_RECEIPTS_PER_TX} receipts")]
    TooManyReceipts,
}

/// Parse the top-level receipts returned for `tx_hash`. Structural
/// problems (not an object, too deep, too many receipts) are errors;
/// per-receipt problems become [`Reject`]s so one bad receipt never hides
/// the others.
pub fn parse_tx_receipts(tx_hash: &TxHash, roots: &[Value]) -> Result<ParsedTx, TxParseError> {
    let mut parsed = ParsedTx::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut visited = 0usize;
    for root in roots {
        let root: EnrichReceipt = serde_json::from_value(root.clone())?;
        // Iterative walk: the tree depth is bounded by serde_json's
        // recursion limit, the node count by MAX_RECEIPTS_PER_TX.
        let mut stack: Vec<&EnrichReceipt> = vec![&root];
        while let Some(receipt) = stack.pop() {
            visited += 1;
            if visited > MAX_RECEIPTS_PER_TX {
                return Err(TxParseError::TooManyReceipts);
            }
            stack.extend(receipt.receipts.iter());
            if receipt.receiver_account_id != REF_CONTRACT {
                continue;
            }
            parsed.stats.ref_receipts += 1;
            if !seen.insert(receipt.receipt_id.clone()) {
                parsed.stats.duplicate_receipts += 1;
                continue;
            }
            match parse_ref_receipt(tx_hash, receipt) {
                Ok(RefReceiptOutcome::Trade(trade)) => {
                    parsed.stats.accepted += 1;
                    parsed.trades.push(*trade);
                }
                Ok(RefReceiptOutcome::Failed) => parsed.stats.failed_receipts += 1,
                Ok(RefReceiptOutcome::NotASwap) => parsed.stats.other_receipts += 1,
                Err(reason) => {
                    *parsed.stats.rejects.entry(reason.kind()).or_insert(0) += 1;
                    parsed.rejects.push(Reject {
                        receipt_id: receipt.receipt_id.clone(),
                        reason,
                    });
                }
            }
        }
    }
    // Deterministic order: by block time, then receipt id.
    parsed.trades.sort_by(|a, b| {
        (a.block_time, a.receipt_id.as_str()).cmp(&(b.block_time, b.receipt_id.as_str()))
    });
    Ok(parsed)
}

enum RefReceiptOutcome {
    Trade(Box<Trade>),
    Failed,
    NotASwap,
}

struct SwappedLog {
    amount_in: u128,
    token_in: TokenAccount,
    amount_out: u128,
    token_out: TokenAccount,
}

fn parse_amount(text: &str) -> Option<u128> {
    if text.is_empty()
        || text.len() > MAX_AMOUNT_DIGITS
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    text.parse().ok()
}

/// `Swapped {in} {token_in} for {out} {token_out}[, total fee N, admin fee N]`
fn parse_swapped_log(line: &str) -> Result<SwappedLog, TradeRejectReason> {
    let malformed = || TradeRejectReason::MalformedLog(line.chars().take(120).collect());
    if line.len() > MAX_LOG_LINE_LEN {
        return Err(malformed());
    }
    let body = line.strip_prefix("Swapped ").ok_or_else(malformed)?;
    let body = body.split(',').next().unwrap_or("");
    let parts: Vec<&str> = body.split_whitespace().collect();
    let [amount_in, token_in, kw, amount_out, token_out] = parts.as_slice() else {
        return Err(malformed());
    };
    if *kw != "for" {
        return Err(malformed());
    }
    Ok(SwappedLog {
        amount_in: parse_amount(amount_in).ok_or_else(malformed)?,
        token_in: TokenAccount::from_str(token_in)
            .map_err(|_| TradeRejectReason::InvalidToken((*token_in).to_owned()))?,
        amount_out: parse_amount(amount_out).ok_or_else(malformed)?,
        token_out: TokenAccount::from_str(token_out)
            .map_err(|_| TradeRejectReason::InvalidToken((*token_out).to_owned()))?,
    })
}

/// NearBlocks returns the `msg` of `ft_on_transfer` over-escaped
/// (`{\"force\":0,...}` or worse). Try as-is, then with the backslashes
/// before quotes removed; unwrap up to two levels of string-encoded JSON.
fn parse_ft_msg(msg: &str) -> Result<Vec<SwapAction>, TradeRejectReason> {
    if msg.len() > MAX_MSG_LEN {
        return Err(TradeRejectReason::MsgTooLong(msg.len()));
    }
    #[derive(Deserialize)]
    struct FtMsg {
        #[serde(default)]
        actions: Vec<SwapAction>,
    }
    let stripped = strip_escaped_quotes(msg);
    for candidate in [msg, stripped.as_str()] {
        let Ok(mut value) = serde_json::from_str::<Value>(candidate) else {
            continue;
        };
        for _ in 0..2 {
            if let Value::String(inner) = &value {
                match serde_json::from_str::<Value>(inner) {
                    Ok(v) => value = v,
                    Err(_) => break,
                }
            }
        }
        if value.is_object() {
            let parsed: FtMsg =
                serde_json::from_value(value).map_err(|_| TradeRejectReason::MalformedMsg)?;
            return Ok(parsed.actions);
        }
    }
    Err(TradeRejectReason::MalformedMsg)
}

/// Replace every run of backslashes followed by a quote with a bare quote.
fn strip_escaped_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_backslashes = 0usize;
    for c in s.chars() {
        match c {
            '\\' => pending_backslashes += 1,
            '"' => {
                pending_backslashes = 0;
                out.push('"');
            }
            other => {
                out.extend(std::iter::repeat_n('\\', pending_backslashes));
                pending_backslashes = 0;
                out.push(other);
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', pending_backslashes));
    out
}

/// Decode `outcome.result` (base64 of a JSON value) as an integer, which
/// the contract returns as a JSON string (sometimes a number).
fn decode_result_int(result: &str) -> Option<u128> {
    let bytes = BASE64.decode(result).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    match value {
        Value::String(s) => parse_amount(&s),
        Value::Number(n) => n.as_u64().map(u128::from),
        _ => None,
    }
}

fn parse_ref_receipt(
    tx_hash: &TxHash,
    receipt: &EnrichReceipt,
) -> Result<RefReceiptOutcome, TradeRejectReason> {
    if !receipt.outcome.status {
        return Ok(RefReceiptOutcome::Failed);
    }
    let mut actions: Vec<SwapAction> = Vec::new();
    let mut route: Option<TradeRoute> = None;
    let mut sender: Option<String> = None;
    for action in &receipt.actions {
        let args = action.args.as_ref().and_then(|a| a.args_json.clone());
        match action.method.as_deref() {
            Some(SWAP_METHOD) => {
                let parsed: SwapArgs = serde_json::from_value(args.unwrap_or(Value::Null))
                    .map_err(|e| TradeRejectReason::MalformedArgs(e.to_string()))?;
                actions.extend(parsed.actions);
                route.get_or_insert(TradeRoute::Swap);
                sender.get_or_insert_with(|| receipt.predecessor_account_id.clone());
            }
            Some(FT_METHOD) => {
                let parsed: FtArgs = serde_json::from_value(args.unwrap_or(Value::Null))
                    .map_err(|e| TradeRejectReason::MalformedArgs(e.to_string()))?;
                let refund = receipt
                    .outcome
                    .result
                    .as_ref()
                    .and_then(Value::as_str)
                    .and_then(decode_result_int)
                    .ok_or(TradeRejectReason::MissingRefund)?;
                if refund != 0 {
                    return Err(TradeRejectReason::FtRefund(refund.to_string()));
                }
                actions.extend(parse_ft_msg(&parsed.msg)?);
                route.get_or_insert(TradeRoute::Ft);
                sender.get_or_insert(parsed.sender_id);
            }
            _ => {}
        }
    }
    let (Some(route), Some(sender)) = (route, sender) else {
        return Ok(RefReceiptOutcome::NotASwap);
    };
    if actions.is_empty() {
        return Err(TradeRejectReason::NoActions);
    }
    if actions.len() > MAX_HOPS {
        return Err(TradeRejectReason::TooManyActions(actions.len()));
    }
    if receipt.outcome.logs.len() > MAX_LOG_LINES {
        return Err(TradeRejectReason::TooManyLogs(receipt.outcome.logs.len()));
    }
    let logs = receipt
        .outcome
        .logs
        .iter()
        .filter(|l| l.starts_with("Swapped "))
        .map(|l| parse_swapped_log(l))
        .collect::<Result<Vec<SwappedLog>, _>>()?;
    if logs.len() != actions.len() {
        return Err(TradeRejectReason::LogCountMismatch {
            actions: actions.len(),
            logs: logs.len(),
        });
    }
    let hops = actions
        .iter()
        .zip(logs)
        .enumerate()
        .map(|(index, (action, log))| {
            if log.token_in.as_str() != action.token_in
                || log.token_out.as_str() != action.token_out
            {
                return Err(TradeRejectReason::LogTokenMismatch { index });
            }
            let pool_id = u32::try_from(action.pool_id).map_err(|_| {
                TradeRejectReason::MalformedArgs(format!("pool_id {} exceeds u32", action.pool_id))
            })?;
            Ok(Hop {
                pool_id,
                token_in: log.token_in.to_in(),
                token_out: log.token_out.to_out(),
                amount_in: TokenSmallestUnits::from_u128(log.amount_in),
                amount_out: TokenSmallestUnits::from_u128(log.amount_out),
            })
        })
        .collect::<Result<Vec<Hop>, _>>()?;
    let block = receipt
        .block
        .as_ref()
        .ok_or(TradeRejectReason::MissingBlock)?;
    let block_time = block.time().ok_or(TradeRejectReason::MissingBlock)?;
    let (amount_in, amount_out) = receipt_amounts(&hops);
    let trade = Trade::new(TradeParts {
        receipt_id: ReceiptId::new(&receipt.receipt_id)
            .map_err(|_| TradeRejectReason::InvalidReceiptId(receipt.receipt_id.clone()))?,
        tx_hash: tx_hash.clone(),
        block_height: block.block_height,
        block_time,
        sender: AccountId::from_str(&sender)
            .map_err(|_| TradeRejectReason::InvalidSender(sender.clone()))?,
        route,
        hops,
        fees: None,
        amount_in: TokenSmallestUnits::from_u128(amount_in),
        amount_out: TokenSmallestUnits::from_u128(amount_out),
    })?;
    Ok(RefReceiptOutcome::Trade(Box::new(trade)))
}

/// Receipt-level amounts per the [`Trade::amount_in`] / [`Trade::amount_out`]
/// convention: a chain uses its endpoints, a split sums the legs that
/// share the first input token / the last output token.
fn receipt_amounts(hops: &[Hop]) -> (u128, u128) {
    let first = hops.first().expect("caller guarantees at least one hop");
    let last = hops.last().expect("caller guarantees at least one hop");
    let chained = hops.windows(2).all(|w| w[0].token_out.0 == w[1].token_in.0);
    let shape = if chained {
        RouteShape::Chain
    } else {
        RouteShape::Split
    };
    let as_u128 = |u: &TokenSmallestUnits| -> u128 {
        use num_traits::ToPrimitive;
        u.as_bigdecimal()
            .to_u128()
            .expect("hop amounts were built from u128")
    };
    match shape {
        RouteShape::Chain => (as_u128(&first.amount_in), as_u128(&last.amount_out)),
        RouteShape::Split => {
            let amount_in = hops
                .iter()
                .filter(|h| h.token_in.0 == first.token_in.0)
                .map(|h| as_u128(&h.amount_in))
                .fold(0u128, u128::saturating_add);
            let amount_out = hops
                .iter()
                .filter(|h| h.token_out.0 == last.token_out.0)
                .map(|h| as_u128(&h.amount_out))
                .fold(0u128, u128::saturating_add);
            (amount_in, amount_out)
        }
    }
}

#[cfg(test)]
mod tests;
