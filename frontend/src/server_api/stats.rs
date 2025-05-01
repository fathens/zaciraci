use crate::api_underlying::Underlying;
use anyhow::Result;
use std::sync::Arc;
use zaciraci_common::ApiResponse;
use zaciraci_common::stats::{DescribesRequest, GetValuesRequest, GetValuesResponse};

pub struct StatsApi {
    pub underlying: Arc<Underlying>,
}

impl StatsApi {
    pub async fn describes(&self, request: &DescribesRequest) -> Result<String> {
        let lines: Vec<String> = self.underlying.post("stats/describes", request).await?;
        Ok(lines.join("\n"))
    }

    pub async fn get_values(
        &self,
        request: &GetValuesRequest,
    ) -> Result<ApiResponse<GetValuesResponse, String>> {
        self.underlying.post("stats/get_values", request).await
    }
}
