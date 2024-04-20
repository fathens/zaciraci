use crate::logging::*;
use crate::ref_finance::{Result, CLIENT, CONTRACT_ADDRESS};
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::{BlockReference, Finality, FunctionArgs};
use near_primitives::views::QueryRequest;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde::Deserialize;
use serde_json::{from_slice, json};

/**
  sample json:
  {
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
  }
*/
#[derive(Debug, Deserialize)]
pub struct PoolInfo {
    pub pool_kind: String,
    pub token_account_ids: Vec<AccountId>,
    pub amounts: Vec<U128>,
    pub total_fee: u32,
    pub shares_total_supply: U128,
    pub amp: u64,
}

pub async fn get_all() -> Result<Vec<PoolInfo>> {
    let methods_name = "get_pools".to_string();

    let limit = 100;
    let mut index = 0;
    let mut pools = vec![];

    loop {
        info!(DEFAULT,"Getting all pools"; "count" => pools.len(), "index" => index, "limit" => limit);
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
            debug!(DEFAULT, "Got pools"; "count" => count);
            pools.extend(list);
            if count < limit {
                break;
            }
        }

        index += limit;
    }

    Ok(pools)
}
