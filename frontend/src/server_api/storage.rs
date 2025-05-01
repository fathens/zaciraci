use crate::api_underlying::Underlying;
use std::sync::Arc;

pub struct StorageApi {
    pub underlying: Arc<Underlying>,
}

impl StorageApi {
    pub async fn deposit_min(&self) -> String {
        self.underlying.get_text("storage/deposit_min").await
    }

    pub async fn deposit(&self, amount: &str) -> String {
        self.underlying
            .get_text(&format!("storage/deposit/{amount}"))
            .await
    }

    pub async fn unregister_token(&self, token_account: &str) -> String {
        self.underlying
            .get_text(&format!("storage/unregister/{token_account}"))
            .await
    }

    pub async fn amounts_list(&self) -> String {
        self.underlying.get_text("storage/amounts/list").await
    }

    pub async fn amounts_wrap(&self, amount: &str) -> String {
        self.underlying
            .get_text(&format!("storage/amounts/wrap/{amount}"))
            .await
    }

    pub async fn amounts_unwrap(&self, amount: &str) -> String {
        self.underlying
            .get_text(&format!("storage/amounts/unwrap/{amount}"))
            .await
    }

    pub async fn amounts_deposit(&self, token_account: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!("storage/amounts/deposit/{token_account}/{amount}"))
            .await
    }

    pub async fn amounts_withdraw(&self, token_account: &str, amount: &str) -> String {
        self.underlying
            .get_text(&format!(
                "storage/amounts/withdraw/{token_account}/{amount}"
            ))
            .await
    }
}
