//! An executed Ref Finance trade as reconstructed from one Ref receipt.
//!
//! A single receipt sent to `v2.ref-finance.near` may execute several
//! `swap` actions (hops). This module models that receipt as a [`Trade`]
//! whose hops are validated together so that downstream code (persistence,
//! analytics) can rely on the invariants without re-checking them:
//!
//! - at least one hop and at most [`MAX_HOPS`]
//! - when fees are present, exactly one fee per hop, each below
//!   [`FEE_DIVISOR`]
//! - amounts are non-negative integers that fit in `u128`, and the input
//!   amount is strictly positive
//!
//! Hop amounts are intentionally not stored per hop: only the overall
//! input and output of the receipt are known reliably for every route
//! shape. See [`Trade::amount_in`] for the split-route convention.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDateTime;
use common::types::{
    AccountId, ReceiptId, TokenInAccount, TokenOutAccount, TokenSmallestUnits, TxHash,
};
use num_traits::ToPrimitive;
use thiserror::Error;

use crate::token_pair::FEE_DIVISOR;

/// Upper bound on hops per receipt. Mirrors the `cardinality(pool_ids)
/// BETWEEN 1 AND 16` CHECK on `ref_flow_trades`.
pub const MAX_HOPS: usize = 16;

/// One `swap` action inside a Ref receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub pool_id: u32,
    pub token_in: TokenInAccount,
    pub token_out: TokenOutAccount,
}

/// How the trade reached the Ref contract.
///
/// `Swap` is a direct `swap` function call; `Ft` is a `ft_on_transfer`
/// callback where the swap actions were carried in the transfer `msg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TradeRoute {
    Swap,
    Ft,
}

impl TradeRoute {
    /// Every variant, for exhaustiveness tests against the DB CHECK.
    pub const ALL: [TradeRoute; 2] = [TradeRoute::Swap, TradeRoute::Ft];

    /// Canonical DB string.
    // SYNC: CHECK (route IN ('swap','ft')) on ref_flow_trades
    pub fn as_str(self) -> &'static str {
        match self {
            TradeRoute::Swap => "swap",
            TradeRoute::Ft => "ft",
        }
    }
}

impl fmt::Display for TradeRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown trade route: {0:?}")]
pub struct ParseTradeRouteError(String);

impl FromStr for TradeRoute {
    type Err = ParseTradeRouteError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // SYNC: CHECK (route IN ('swap','ft')) on ref_flow_trades
        match s {
            "swap" => Ok(TradeRoute::Swap),
            "ft" => Ok(TradeRoute::Ft),
            other => Err(ParseTradeRouteError(other.to_owned())),
        }
    }
}

/// Topology of the hops, derived from the hop token lists.
///
/// `Chain` means every hop's output token is the next hop's input token,
/// so the receipt is a single path and the overall input/output amounts
/// describe one swap. `Split` is anything else (parallel legs, several
/// independent swaps in one receipt); per-leg amounts are not
/// recoverable from the stored data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteShape {
    Chain,
    Split,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TradeError {
    #[error("trade has no hops")]
    NoHops,
    #[error("trade has {0} hops, more than the {MAX_HOPS} allowed")]
    TooManyHops(usize),
    #[error("trade has {fees} fees for {hops} hops")]
    FeeCountMismatch { hops: usize, fees: usize },
    #[error("hop fee {0} is not below the fee divisor {FEE_DIVISOR}")]
    FeeOutOfRange(u32),
    #[error("amount_in {0} is not a positive integer that fits in u128")]
    InvalidAmountIn(TokenSmallestUnits),
    #[error("amount_out {0} is not a non-negative integer that fits in u128")]
    InvalidAmountOut(TokenSmallestUnits),
}

/// One executed Ref receipt. Construct only through [`Trade::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    pub receipt_id: ReceiptId,
    pub tx_hash: TxHash,
    pub block_height: u64,
    pub block_time: NaiveDateTime,
    /// Economic sender. For `Ft` routes this is the `sender_id` of the
    /// transfer, not the predecessor contract.
    pub sender: AccountId,
    pub route: TradeRoute,
    hops: Vec<Hop>,
    fees: Option<Vec<u32>>,
    amount_in: TokenSmallestUnits,
    amount_out: TokenSmallestUnits,
}

/// Validated constructor input, so call sites stay readable without a
/// twelve-argument function.
#[derive(Debug, Clone)]
pub struct TradeParts {
    pub receipt_id: ReceiptId,
    pub tx_hash: TxHash,
    pub block_height: u64,
    pub block_time: NaiveDateTime,
    pub sender: AccountId,
    pub route: TradeRoute,
    pub hops: Vec<Hop>,
    /// Pool fees in units of `1/FEE_DIVISOR`, one per hop and in hop
    /// order, or `None` when the fee of at least one hop is unknown.
    pub fees: Option<Vec<u32>>,
    pub amount_in: TokenSmallestUnits,
    pub amount_out: TokenSmallestUnits,
}

fn is_u128_integer(value: &TokenSmallestUnits) -> bool {
    let bd = value.as_bigdecimal();
    bd.is_integer() && bd.to_u128().is_some()
}

impl Trade {
    /// Validate the invariants listed in the module docs and build a trade.
    pub fn new(parts: TradeParts) -> Result<Self, TradeError> {
        let hops = parts.hops;
        if hops.is_empty() {
            return Err(TradeError::NoHops);
        }
        if hops.len() > MAX_HOPS {
            return Err(TradeError::TooManyHops(hops.len()));
        }
        if let Some(fees) = &parts.fees {
            if fees.len() != hops.len() {
                return Err(TradeError::FeeCountMismatch {
                    hops: hops.len(),
                    fees: fees.len(),
                });
            }
            if let Some(&bad) = fees.iter().find(|&&f| f >= FEE_DIVISOR) {
                return Err(TradeError::FeeOutOfRange(bad));
            }
        }
        if !is_u128_integer(&parts.amount_in) || parts.amount_in.is_zero() {
            return Err(TradeError::InvalidAmountIn(parts.amount_in));
        }
        if !is_u128_integer(&parts.amount_out) {
            return Err(TradeError::InvalidAmountOut(parts.amount_out));
        }
        Ok(Self {
            receipt_id: parts.receipt_id,
            tx_hash: parts.tx_hash,
            block_height: parts.block_height,
            block_time: parts.block_time,
            sender: parts.sender,
            route: parts.route,
            hops,
            fees: parts.fees,
            amount_in: parts.amount_in,
            amount_out: parts.amount_out,
        })
    }

    /// Hops in execution order (never empty).
    pub fn hops(&self) -> &[Hop] {
        &self.hops
    }

    /// Pool fees in hop order, when known for every hop.
    pub fn fees(&self) -> Option<&[u32]> {
        self.fees.as_deref()
    }

    /// Attach the pool fee of every hop (hop order, units of
    /// `1/FEE_DIVISOR`), re-validating the fee invariants.
    pub fn with_fees(mut self, fees: Vec<u32>) -> Result<Self, TradeError> {
        if fees.len() != self.hops.len() {
            return Err(TradeError::FeeCountMismatch {
                hops: self.hops.len(),
                fees: fees.len(),
            });
        }
        if let Some(&bad) = fees.iter().find(|&&f| f >= FEE_DIVISOR) {
            return Err(TradeError::FeeOutOfRange(bad));
        }
        self.fees = Some(fees);
        Ok(self)
    }

    /// Hops zipped with their fee (`None` when fees are unknown).
    pub fn hops_with_fee(&self) -> impl Iterator<Item = (&Hop, Option<u32>)> + '_ {
        self.hops
            .iter()
            .enumerate()
            .map(move |(i, hop)| (hop, self.fees.as_ref().map(|f| f[i])))
    }

    /// Input token of the first hop.
    pub fn token_in(&self) -> &TokenInAccount {
        &self.first_hop().token_in
    }

    /// Output token of the last hop.
    pub fn token_out(&self) -> &TokenOutAccount {
        &self.last_hop().token_out
    }

    /// Total input amount in smallest units of [`Trade::token_in`].
    ///
    /// For a [`RouteShape::Chain`] this is the amount fed into the first
    /// hop. For a [`RouteShape::Split`] it is, by convention, the sum of
    /// the explicit `amount_in` of every action whose input token equals
    /// the first hop's input token; legs starting from other tokens are
    /// not included.
    pub fn amount_in(&self) -> &TokenSmallestUnits {
        &self.amount_in
    }

    /// Total output amount in smallest units of [`Trade::token_out`], as
    /// reported by the last `Swapped` log of the receipt.
    pub fn amount_out(&self) -> &TokenSmallestUnits {
        &self.amount_out
    }

    /// Whether the hops form one connected path.
    pub fn shape(&self) -> RouteShape {
        let chained = self
            .hops
            .windows(2)
            .all(|w| w[0].token_out.0 == w[1].token_in.0);
        if chained {
            RouteShape::Chain
        } else {
            RouteShape::Split
        }
    }

    fn first_hop(&self) -> &Hop {
        self.hops
            .first()
            .expect("Trade invariant: at least one hop")
    }

    fn last_hop(&self) -> &Hop {
        self.hops.last().expect("Trade invariant: at least one hop")
    }
}

#[cfg(test)]
mod tests;
