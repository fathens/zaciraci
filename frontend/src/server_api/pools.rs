use crate::server_api::Underlying;
use anyhow::Result;
use std::sync::Arc;
use zaciraci_common::pools::{TradeRequest, TradeResponse};

pub struct PoolsApi {
    pub underlying: Arc<Underlying>,
}

impl PoolsApi {
    pub async fn get_all_pools(&self) -> String {
        self.underlying.get_text("pools/get_all").await
    }

    pub async fn estimate_return(&self, pool_id: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("pools/estimate_return/{pool_id}/{amount}"))
            .await
    }

    pub async fn get_return(&self, pool_id: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("pools/get_return/{pool_id}/{amount}"))
            .await
    }

    pub async fn list_all_tokens(&self) -> String {
        self.underlying.get_text("pools/list_all_tokens").await
    }

    pub async fn list_returns(&self, token_account: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("pools/list_returns/{token_account}/{amount}"))
            .await
    }

    pub async fn pick_goals(&self, token_account: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("pools/pick_goals/{token_account}/{amount}"))
            .await
    }

    pub async fn run_swap(
        &self,
        token_in_account: &str,
        initial_value: &str,
        token_out_account: &str,
    ) -> String {
        self.underlying
            .get_text(&format!(
                "pools/run_swap/{token_in_account}/{initial_value}/{token_out_account}"
            ))
            .await
    }

    pub async fn estimate_trade(&self, request: TradeRequest) -> Result<TradeResponse> {
        self.underlying.post("pools/estimate_trade", &request).await
    }
}
