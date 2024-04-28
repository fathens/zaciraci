use crate::logging::*;
use crate::persistence::tables;
use crate::ref_finance::{
    errors::Error::{AmountsNotTwo, TokenIdsNotTwo},
    CLIENT, CONTRACT_ADDRESS,
};
use crate::Result;
use bigdecimal::BigDecimal;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::{BlockReference, Finality, FunctionArgs};
use near_primitives::views::QueryRequest;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, json};

#[derive(Debug, Deserialize, Serialize)]
pub struct PoolInfo {
    pub pool_kind: String,
    pub token_account_ids: Vec<AccountId>,
    pub amounts: Vec<U128>,
    pub total_fee: u32,
    pub shares_total_supply: U128,
    pub amp: u64,
}

impl PoolInfo {
    fn get_first_two<T: Clone>(v: &[T]) -> Option<(T, T)> {
        let a = v.first();
        let b = v.get(1);
        match (a, b) {
            (Some(a), Some(b)) => Some((a.clone(), b.clone())),
            _ => None,
        }
    }

    fn token_account_ids(&self) -> Result<(AccountId, AccountId)> {
        Self::get_first_two(&self.token_account_ids)
            .ok_or_else(|| TokenIdsNotTwo(self.token_account_ids.len()).into())
    }

    fn amounts(&self) -> Result<(U128, U128)> {
        Self::get_first_two(&self.amounts).ok_or_else(|| AmountsNotTwo(self.amounts.len()).into())
    }
}

pub struct PoolInfoList(Vec<PoolInfo>);

impl PoolInfoList {
    fn to_records(&self) -> Result<Vec<tables::pool_info::PoolInfo>> {
        fn from_u128(value: U128) -> BigDecimal {
            let v: u128 = value.into();
            BigDecimal::from(v)
        }
        self.0
            .iter()
            .enumerate()
            .try_fold(vec![], |mut records, (id, pool)| {
                let (token_a, token_b) = pool.token_account_ids()?;
                let (amount_a, amount_b) = pool.amounts()?;
                let record = tables::pool_info::PoolInfo {
                    id: id as i32,
                    pool_kind: pool.pool_kind.clone(),
                    token_account_id_a: token_a.into(),
                    token_account_id_b: token_b.into(),
                    amount_a: from_u128(amount_a),
                    amount_b: from_u128(amount_b),
                    total_fee: pool.total_fee as i64,
                    shares_total_supply: from_u128(pool.shares_total_supply),
                    amp: BigDecimal::from(pool.amp),
                    updated_at: chrono::Utc::now().naive_utc(),
                };
                records.push(record);
                Ok(records)
            })
    }

    pub async fn update_all(&self) -> Result<usize> {
        tables::pool_info::update_all(self.to_records()?).await
    }
}

pub async fn get_all_from_node() -> Result<PoolInfoList> {
    let log = DEFAULT.new(o!("function" => "get_all_from_node"));
    info!(log, "start");

    let methods_name = "get_pools".to_string();

    let limit = 100;
    let mut index = 0;
    let mut pools = vec![];

    loop {
        trace!(log, "Getting all pools"; "count" => pools.len(), "index" => index, "limit" => limit);
        let request = methods::query::RpcQueryRequest {
            block_reference: BlockReference::Finality(Finality::Final),
            request: QueryRequest::CallFunction {
                account_id: CONTRACT_ADDRESS.clone(),
                method_name: methods_name.clone(),
                args: FunctionArgs::from(
                    json!({
                        "from_index": index,
                        "limit": limit,
                    })
                    .to_string()
                    .into_bytes(),
                ),
            },
        };

        let response = CLIENT.call(request).await?;

        if let QueryResponseKind::CallResult(result) = response.kind {
            let list: Vec<PoolInfo> = from_slice(&result.result)?;
            let count = list.len();
            trace!(log, "Got pools"; "count" => count);
            pools.extend(list);
            if count < limit {
                break;
            }
        }

        index += limit;
    }

    info!(log, "finish"; "count" => pools.len());
    Ok(PoolInfoList(pools))
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

        let pool_info: PoolInfo = serde_json::from_str(json).unwrap();
        assert_eq!(pool_info.pool_kind, "SIMPLE_POOL");
        assert_eq!(
            pool_info.token_account_ids,
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

        let pools: Vec<PoolInfo> = from_slice(json.as_bytes()).unwrap();
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
}
