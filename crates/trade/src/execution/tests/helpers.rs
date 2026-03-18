use common::types::TokenAccount;

pub fn ta(s: &str) -> TokenAccount {
    s.parse().unwrap()
}
