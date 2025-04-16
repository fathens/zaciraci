use anyhow::Result;
use std::sync::Arc;
use zaciraci_common::stats::DescribesRequest;

use super::Underlying;

pub struct StatsApi {
    pub underlying: Arc<Underlying>,
}

impl StatsApi {
    pub async fn describes(&self, request: &DescribesRequest) -> Result<String> {
        let lines: Vec<String> = self.underlying.post("stats/describes", request).await?;
        Ok(lines.join("\n"))
    }
}
