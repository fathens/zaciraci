use std::sync::Arc;
use zaciraci_common::stats::DescribesRequest;

use super::Underlying;

pub struct StatsApi {
    pub underlying: Arc<Underlying>,
}

impl StatsApi {
    pub async fn describes(&self, request: &DescribesRequest) -> String {
        self.underlying.post("stats/describes", request).await.unwrap_or_default()
    }
}