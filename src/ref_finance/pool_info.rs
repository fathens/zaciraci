use crate::jsonrpc;
use crate::logging::*;
use crate::persistence::tables;
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::ref_finance::token_index::{TokenIn, TokenIndex, TokenOut};
use crate::ref_finance::{errors::Error, CONTRACT_ADDRESS};
use crate::Result;
use bigdecimal::{BigDecimal, ToPrimitive};
use near_sdk::json_types::U128;
use num_bigint::Sign::NoSign;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, json};
use std::collections::HashMap;
use std::ops::Deref;
use std::slice::Iter;
use std::sync::Arc;

const POOL_KIND_SIMPLE: &str = "SIMPLE_POOL";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolInfoBared {
    pub pool_kind: String,
    pub token_account_ids: Vec<TokenAccount>,
    pub amounts: Vec<U128>,
    pub total_fee: u32,
    pub shares_total_supply: U128,
    pub amp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolInfo {
    id: u32,
    bare: PoolInfoBared,
    updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolInfoList {
    list: Vec<Arc<PoolInfo>>,
    by_id: HashMap<u32, Arc<PoolInfo>>,
}

impl From<PoolInfo> for tables::pool_info::PoolInfo {
    fn from(src: PoolInfo) -> Self {
        fn from_u128(value: U128) -> BigDecimal {
            let v: u128 = value.into();
            BigDecimal::from(v)
        }
        tables::pool_info::PoolInfo {
            id: src.id as i32,
            pool_kind: src.bare.pool_kind.clone(),
            token_account_ids: src
                .bare
                .token_account_ids
                .into_iter()
                .map(|v| v.to_string())
                .collect(),
            amounts: src.bare.amounts.into_iter().map(from_u128).collect(),
            total_fee: src.bare.total_fee as i64,
            shares_total_supply: from_u128(src.bare.shares_total_supply),
            amp: BigDecimal::from(src.bare.amp),
            updated_at: src.updated_at,
        }
    }
}

impl From<tables::pool_info::PoolInfo> for PoolInfo {
    fn from(src: tables::pool_info::PoolInfo) -> Self {
        fn to_u128(value: BigDecimal) -> U128 {
            let v: u128 = value.to_u128().expect("should be valid value");
            v.into()
        }
        PoolInfo {
            id: src.id as u32,
            bare: PoolInfoBared {
                pool_kind: src.pool_kind.clone(),
                token_account_ids: src
                    .token_account_ids
                    .iter()
                    .map(|v| v.parse().expect("should be valid AccountId"))
                    .collect(),
                amounts: src.amounts.into_iter().map(to_u128).collect(),
                total_fee: src.total_fee as u32,
                shares_total_supply: to_u128(src.shares_total_supply.clone()),
                amp: src.amp.to_u64().expect("should be valid value"),
            },
            updated_at: src.updated_at,
        }
    }
}

pub const FEE_DIVISOR: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPair {
    pool: Arc<PoolInfo>,
    pub token_in: TokenIn,
    pub token_out: TokenOut,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct TokenPairId {
    pub pool_id: u32,
    pub token_in: TokenIn,
    pub token_out: TokenOut,
}

impl TokenPair {
    pub fn pair_id(&self) -> TokenPairId {
        TokenPairId {
            pool_id: self.pool.id,
            token_in: self.token_in,
            token_out: self.token_out,
        }
    }

    pub fn pool_id(&self) -> u32 {
        self.pool.id
    }

    pub fn token_in_id(&self) -> TokenInAccount {
        self.pool
            .token(self.token_in.as_index())
            .map(|v| v.into())
            .expect("should be valid index")
    }

    pub fn token_out_id(&self) -> TokenOutAccount {
        self.pool
            .token(self.token_out.as_index())
            .map(|v| v.into())
            .expect("should be valid index")
    }

    pub fn estimate_return(&self, amount_in: u128) -> Result<u128> {
        self.pool
            .estimate_return(self.token_in, amount_in, self.token_out)
    }

    pub async fn get_return(&self, amount_in: u128) -> Result<u128> {
        self.pool
            .get_return(
                self.pool.token(self.token_in.as_index())?.into(),
                amount_in,
                self.pool.token(self.token_out.as_index())?.into(),
            )
            .await
    }
}

impl PoolInfo {
    pub fn new(id: u32, bare: PoolInfoBared) -> Self {
        PoolInfo {
            id,
            bare,
            updated_at: chrono::Utc::now().naive_utc(),
        }
    }

    pub fn kind(&self) -> &str {
        &self.bare.pool_kind
    }

    pub fn is_simple(&self) -> bool {
        self.bare.pool_kind == POOL_KIND_SIMPLE
    }

    pub fn len(&self) -> usize {
        self.bare.token_account_ids.len()
    }

    pub fn get_pair(self: &Arc<Self>, token_in: TokenIn, token_out: TokenOut) -> Result<TokenPair> {
        if token_in.as_index() == token_out.as_index() {
            return Err(Error::SwapSameToken.into());
        }
        if token_in.as_usize() >= self.len() || token_out.as_usize() >= self.len() {
            return Err(
                Error::OutOfIndexOfTokens(token_in.as_index().max(token_out.as_index())).into(),
            );
        }
        if token_in.as_usize() >= self.bare.amounts.len()
            || token_out.as_usize() >= self.bare.amounts.len()
        {
            return Err(Error::DifferentLengthOfTokens(
                self.bare.token_account_ids.len(),
                self.bare.amounts.len(),
            )
            .into());
        }
        Ok(TokenPair {
            pool: Arc::clone(self),
            token_in,
            token_out,
        })
    }

    pub fn tokens(&self) -> Iter<TokenAccount> {
        self.bare.token_account_ids.iter()
    }

    pub fn token(&self, index: TokenIndex) -> Result<TokenAccount> {
        self.bare
            .token_account_ids
            .get(index.as_usize())
            .cloned()
            .ok_or_else(|| Error::OutOfIndexOfTokens(index).into())
    }

    fn amount(&self, index: TokenIndex) -> Result<BigDecimal> {
        self.bare
            .amounts
            .get(index.as_usize())
            .map(|v| BigDecimal::from(v.0))
            .ok_or_else(|| Error::OutOfIndexOfTokens(index).into())
    }

    fn estimate_return(
        &self,
        token_in: TokenIn,
        amount_in: u128,
        token_out: TokenOut,
    ) -> Result<u128> {
        let log = DEFAULT.new(o!(
            "function" => "estimate_return",
            "pool_id" => self.id,
            "amount_in" => amount_in,
            "token_in" => token_in.as_usize(),
            "token_out" => token_out.as_usize(),
        ));
        info!(log, "start");
        if token_in.as_index() == token_out.as_index() {
            return Err(Error::SwapSameToken.into());
        }
        let in_balance = self.amount(token_in.as_index())?;
        trace!(log, "in_balance"; "value" => %in_balance);
        let out_balance = self.amount(token_out.as_index())?;
        trace!(log, "out_balance"; "value" => %out_balance);
        let amount_in = BigDecimal::from(amount_in);
        if in_balance.sign() <= NoSign || out_balance.sign() <= NoSign || amount_in.sign() <= NoSign
        {
            return Err(Error::ZeroAmount.into());
        }
        let amount_with_fee = amount_in * BigDecimal::from(FEE_DIVISOR - self.bare.total_fee);
        let result = &amount_with_fee * out_balance
            / (BigDecimal::from(FEE_DIVISOR) * in_balance + &amount_with_fee);
        info!(log, "finish"; "value" => %result);
        result.to_u128().ok_or_else(|| Error::Overflow.into())
    }

    async fn get_return(
        &self,
        token_in: TokenInAccount,
        amount_in: u128,
        token_out: TokenOutAccount,
    ) -> Result<u128> {
        let log = DEFAULT.new(o!(
            "function" => "get_return",
            "pool_id" => self.id,
            "amount_in" => amount_in,
        ));
        info!(log, "start");
        let method_name = "get_return";

        let args = json!({
            "pool_id": self.id,
            "token_in": token_in.as_id(),
            "amount_in": U128::from(amount_in),
            "token_out": token_out.as_id(),
        })
        .to_string();
        debug!(log, "request_json"; "value" => %args);

        let result = jsonrpc::view_contract(&CONTRACT_ADDRESS, method_name, &args).await?;

        let raw = result.result;
        let value: U128 = from_slice(&raw)?;
        info!(log, "finish"; "value" => %value.0);
        Ok(value.into())
    }
}

impl PoolInfoList {
    fn new(list: Vec<Arc<PoolInfo>>) -> Self {
        let mut by_id = HashMap::new();
        for pool in list.iter() {
            by_id.insert(pool.id, Arc::clone(pool));
        }
        PoolInfoList { list, by_id }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn iter(&self) -> Iter<Arc<PoolInfo>> {
        self.list.iter()
    }

    pub fn get_pair(&self, pair_id: TokenPairId) -> Result<TokenPair> {
        self.get(pair_id.pool_id)?
            .get_pair(pair_id.token_in, pair_id.token_out)
    }

    pub fn get(&self, index: u32) -> Result<Arc<PoolInfo>> {
        self.by_id
            .get(&index)
            .cloned()
            .ok_or_else(|| Error::OutOfIndexOfPools(index).into())
    }

    pub async fn save_to_db(&self) -> Result<usize> {
        let records = self
            .list
            .iter()
            .map(|info| info.deref().clone().into())
            .collect();
        tables::pool_info::update_all(records).await
    }

    pub async fn load_from_db() -> Result<PoolInfoList> {
        let records = tables::pool_info::select_all().await?;
        let pools = records
            .into_iter()
            .map(|record| Arc::new(record.into()))
            .collect();
        Ok(PoolInfoList::new(pools))
    }

    pub async fn read_from_node() -> Result<PoolInfoList> {
        let log = DEFAULT.new(o!("function" => "get_all_from_node"));
        info!(log, "start");

        let method_name = "get_pools";

        let limit = 100;
        let mut index = 0;
        let mut pools = vec![];

        loop {
            trace!(log, "Getting all pools"; "count" => pools.len(), "index" => index, "limit" => limit);
            let args = json!({
                "from_index": index,
                "limit": limit,
            });

            let result = jsonrpc::view_contract(&CONTRACT_ADDRESS, method_name, &args).await?;

            let list: Vec<PoolInfoBared> = from_slice(&result.result)?;
            let count = list.len();
            trace!(log, "Got pools"; "count" => count);
            pools.extend(list);
            if count < limit {
                break;
            }

            index += limit;
        }

        info!(log, "finish"; "count" => pools.len());
        let pools = pools
            .into_iter()
            .enumerate()
            .map(|(i, bare)| Arc::new(PoolInfo::new(i as u32, bare)))
            .collect();
        Ok(PoolInfoList::new(pools))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_pool_info_deserialization() {
        let json = r#"{
            "amounts": [
                "48737022992767037175615",
                "5494257256410498315169867023"
            ],
            "amp": 0,
            "pool_kind": "SIMPLE_POOL",
            "shares_total_supply": "1183889335924371026832035708",
            "token_account_ids": [
                "token.skyward.near",
                "wrap.near"
            ],
            "total_fee": 30
        }"#;

        let pool_info: PoolInfoBared = serde_json::from_str(json).unwrap();
        assert_eq!(pool_info.pool_kind, "SIMPLE_POOL");
        assert_eq!(
            pool_info
                .token_account_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<String>>(),
            vec!["token.skyward.near".to_string(), "wrap.near".to_string()]
        );
        assert_eq!(
            pool_info.amounts,
            vec![
                U128(48737022992767037175615),
                U128(5494257256410498315169867023)
            ]
        );
        assert_eq!(pool_info.total_fee, 30);
        assert_eq!(
            pool_info.shares_total_supply,
            U128(1183889335924371026832035708)
        );
        assert_eq!(pool_info.amp, 0);
    }

    #[test]
    fn test_pool_info_from_slice2() {
        let json = r#"[
          {
            "amounts": [
              "1298766831791624395",
              "662168456946503877590641866"
            ],
            "amp": 0,
            "pool_kind": "SIMPLE_POOL",
            "shares_total_supply": "33778523823194707550511225",
            "token_account_ids": [
              "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.factory.bridge.near",
              "wrap.near"
            ],
            "total_fee": 30
          },
          {
            "amounts": [
              "72878408222217023703924",
              "10387355075955565205240325202"
            ],
            "amp": 0,
            "pool_kind": "SIMPLE_POOL",
            "shares_total_supply": "335641087635970260772416710",
            "token_account_ids": [
              "6b175474e89094c44da98b954eedeac495271d0f.factory.bridge.near",
              "wrap.near"
            ],
            "total_fee": 30
          }
        ]"#;

        let pools: Vec<PoolInfoBared> = from_slice(json.as_bytes()).unwrap();
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0].pool_kind, "SIMPLE_POOL");
        assert_eq!(
            pools[0].shares_total_supply,
            U128(33778523823194707550511225)
        );
        assert_eq!(pools[1].pool_kind, "SIMPLE_POOL");
        assert_eq!(
            pools[1].shares_total_supply,
            U128(335641087635970260772416710)
        );
    }

    #[test]
    fn test_pool_info_estimate_return() {
        let sample = PoolInfo::new(
            0,
            PoolInfoBared {
                pool_kind: "SIMPLE_POOL".to_string(),
                token_account_ids: vec!["token_a".parse().unwrap(), "wrap.near".parse().unwrap()],
                amounts: vec![
                    49821249287591105626851_u128.into(),
                    5375219608484426244903787070_u128.into(),
                ],
                total_fee: 30,
                shares_total_supply: 0_u128.into(),
                amp: 0,
            },
        );
        let result = sample.estimate_return(0.into(), 100, 1.into());
        assert_eq!(Ok(10756643_u128), result);
    }
}
