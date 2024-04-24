use crate::logging::*;
use crate::persistence::tables;
use crate::ref_finance::{CLIENT, CONTRACT_ADDRESS};
use crate::Result;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::{BlockReference, Finality, FunctionArgs};
use near_primitives::views::QueryRequest;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde::Deserialize;
use serde_json::{from_slice, json};

#[derive(Debug, Deserialize)]
pub struct PoolInfo {
    pub pool_kind: String,
    pub token_account_ids: Vec<AccountId>,
    pub amounts: Vec<U128>,
    pub total_fee: u32,
    pub shares_total_supply: U128,
    pub amp: u64,
}

impl From<tables::pool_info::PoolInfo> for PoolInfo {
    fn from(row: tables::pool_info::PoolInfo) -> Self {
        let token_ids: Option<Vec<_>> = [row.token_a.parse().ok(), row.token_b.parse().ok()]
            .iter()
            .cloned()
            .collect();
        PoolInfo {
            pool_kind: row.kind.clone(),
            token_account_ids: token_ids.unwrap_or_default(),
            amounts: vec![row.amount_a, row.amount_b],
            total_fee: row.total_fee,
            shares_total_supply: row.shares_total_supply,
            amp: row.amp,
        }
    }
}

impl From<PoolInfo> for tables::pool_info::PoolInfo {
    fn from(pool: PoolInfo) -> tables::pool_info::PoolInfo {
        let token_a = pool
            .token_account_ids
            .first()
            .map(|id| id.to_string())
            .unwrap_or_default();
        let token_b = pool
            .token_account_ids
            .get(1)
            .map(|id| id.to_string())
            .unwrap_or_default();
        let amount_a = pool.amounts.first().copied().unwrap_or_default();
        let amount_b = pool.amounts.get(1).copied().unwrap_or_default();
        tables::pool_info::PoolInfo {
            index: 0,
            kind: pool.pool_kind,
            token_a,
            token_b,
            amount_a,
            amount_b,
            total_fee: pool.total_fee,
            shares_total_supply: pool.shares_total_supply,
            amp: pool.amp,
            updated_at: chrono::Utc::now(),
        }
    }
}

pub struct PoolInfoList(pub Vec<PoolInfo>);

impl From<tables::pool_info::PoolInfoList> for PoolInfoList {
    fn from(list: tables::pool_info::PoolInfoList) -> Self {
        PoolInfoList(list.0.into_iter().map(PoolInfo::from).collect())
    }
}

impl From<PoolInfoList> for tables::pool_info::PoolInfoList {
    fn from(list: PoolInfoList) -> tables::pool_info::PoolInfoList {
        tables::pool_info::PoolInfoList(
            list.0
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let mut row: tables::pool_info::PoolInfo = v.into();
                    row.index = i as i32;
                    row
                })
                .collect(),
        )
    }
}

pub async fn get_all_from_node() -> Result<PoolInfoList> {
    let methods_name = "get_pools".to_string();

    let limit = 100;
    let mut index = 0;
    let mut pools = vec![];

    loop {
        debug!("Getting all pools"; "count" => pools.len(), "index" => index, "limit" => limit);
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
            debug!("Got pools"; "count" => count);
            pools.extend(list);
            if count < limit {
                break;
            }
        }

        index += limit;
    }

    Ok(PoolInfoList(pools))
}

pub async fn update_all(pools: PoolInfoList) -> Result<()> {
    tables::pool_info::delete_all().await?;
    tables::pool_info::insert_all(pools.into()).await?;
    Ok(())
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
