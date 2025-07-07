use crate::types::{TokenAccount, YoctoNearToken};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TradeRequest {
    pub timestamp: NaiveDateTime,
    pub token_in: TokenAccount,
    pub token_out: TokenAccount,
    pub amount_in: YoctoNearToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TradeResponse {
    pub amount_out: YoctoNearToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecordsRequest {
    pub timestamp: NaiveDateTime,
    pub pool_ids: Vec<PoolId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecordsResponse {
    pub pools: Vec<PoolRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolRecord {
    pub id: PoolId,
    pub timestamp: NaiveDateTime,
    pub bare: PoolBared,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolBared {
    pub pool_kind: String,
    pub token_account_ids: Vec<TokenAccount>,
    pub amounts: Vec<YoctoNearToken>,
    pub total_fee: u32,
    pub shares_total_supply: YoctoNearToken,
    pub amp: u64,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct PoolId(pub u32);

impl From<u32> for PoolId {
    fn from(id: u32) -> Self {
        PoolId(id)
    }
}

impl From<PoolId> for u32 {
    fn from(id: PoolId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SortPoolsRequest {
    pub timestamp: NaiveDateTime,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SortPoolsResponse {
    pub pools: Vec<PoolRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct VolatilityTokensRequest {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub limit: u32,
    pub quote_token: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct VolatilityTokensResponse {
    pub tokens: Vec<TokenAccount>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_sort_pools_request_creation() {
        let request = SortPoolsRequest {
            timestamp: Utc::now().naive_utc(),
            limit: 50,
        };

        assert_eq!(request.limit, 50);
        assert!(request.timestamp <= Utc::now().naive_utc());
    }

    #[test]
    fn test_sort_pools_request_serialization() {
        let request = SortPoolsRequest {
            timestamp: Utc::now().naive_utc(),
            limit: 100,
        };

        // Test serialization round-trip
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SortPoolsRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_sort_pools_response_creation() {
        let response = SortPoolsResponse { pools: vec![] };

        assert!(response.pools.is_empty());
    }

    #[test]
    fn test_sort_pools_response_serialization() {
        let response = SortPoolsResponse { pools: vec![] };

        // Test serialization round-trip
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SortPoolsResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response, deserialized);
    }

    #[test]
    fn test_sort_pools_request_with_different_limits() {
        let limits = [1, 10, 50, 100, 1000];

        for limit in limits {
            let request = SortPoolsRequest {
                timestamp: Utc::now().naive_utc(),
                limit,
            };

            assert_eq!(request.limit, limit);

            // Test that it can be serialized/deserialized
            let json = serde_json::to_string(&request).unwrap();
            let deserialized: SortPoolsRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request, deserialized);
        }
    }
}
