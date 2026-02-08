use crate::errors::Error;
use crate::pool_info::PoolInfo;
use crate::token_index::{TokenIn, TokenOut};
use anyhow::Result;
use common::types::{TokenInAccount, TokenOutAccount};
use std::sync::Arc;

/// TokenPair の機能を抽象化するトレイト
pub trait TokenPairLike {
    /// プールIDを返す
    fn pool_id(&self) -> u32;

    /// 入力トークンIDを返す
    fn token_in_id(&self) -> TokenInAccount;

    /// 出力トークンIDを返す
    fn token_out_id(&self) -> TokenOutAccount;

    /// 入力量から推定出力量を計算する
    fn estimate_return(&self, amount_in: u128) -> Result<u128>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TokenPairId {
    pub pool_id: u32,
    pub token_in: TokenIn,
    pub token_out: TokenOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPair {
    pub(crate) pool: Arc<PoolInfo>,
    pub token_in: TokenIn,
    pub token_out: TokenOut,
}

// TokenPairLike トレイトの実装
impl TokenPairLike for TokenPair {
    fn pool_id(&self) -> u32 {
        self.pool.id
    }

    fn token_in_id(&self) -> TokenInAccount {
        self.pool
            .token(self.token_in.as_index())
            .map(|v| v.into())
            .expect("should be valid index")
    }

    fn token_out_id(&self) -> TokenOutAccount {
        self.pool
            .token(self.token_out.as_index())
            .map(|v| v.into())
            .expect("should be valid index")
    }

    fn estimate_return(&self, amount_in: u128) -> Result<u128> {
        self.pool
            .estimate_return(self.token_in, amount_in, self.token_out)
    }
}

impl TokenPair {
    pub fn pair_id(&self) -> TokenPairId {
        TokenPairId {
            pool_id: self.pool.id,
            token_in: self.token_in,
            token_out: self.token_out,
        }
    }

    /// 入力側のプールサイズを取得
    pub fn amount_in(&self) -> Result<u128> {
        self.pool.amount(self.token_in.as_index())
    }

    /// 出力側のプールサイズを取得
    pub fn amount_out(&self) -> Result<u128> {
        self.pool.amount(self.token_out.as_index())
    }

    pub fn estimate_normal_return(&self) -> Result<(u128, u128)> {
        let balance_in = self.pool.amount(self.token_in.as_index())?;
        if balance_in == 0 {
            return Err(Error::ZeroAmount.into());
        }
        // balance_in/2を使用することで、極端な流動性の偏りがあるプールでも
        // 適切な見積もり値を取得できる
        let in_value = balance_in / 2;
        let out_value = self
            .pool
            .estimate_return(self.token_in, in_value, self.token_out)?;
        Ok((in_value, out_value))
    }
}

pub struct TokenPath(pub Vec<TokenPair>);

impl TokenPath {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn calc_value(&self, initial: u128) -> Result<u128> {
        if initial == 0 {
            return Ok(0);
        }
        let mut value = initial;
        for pair in self.0.iter() {
            value = pair.estimate_return(value)?;
            if value == 0 {
                return Ok(0);
            }
        }
        Ok(value)
    }
}

pub const FEE_DIVISOR: u32 = 10_000;
