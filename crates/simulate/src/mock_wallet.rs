use blockchain::wallet::Wallet;
use near_crypto::InMemorySigner;
use near_sdk::AccountId;

pub struct SimulationWallet {
    account_id: AccountId,
    signer: InMemorySigner,
}

impl SimulationWallet {
    pub fn new() -> Self {
        let account_id: AccountId = "sim.near".parse().unwrap();
        let signer_result = InMemorySigner::from_seed(
            account_id.clone(),
            near_crypto::KeyType::ED25519,
            "sim.near",
        );
        let signer = match signer_result {
            near_crypto::Signer::InMemory(signer) => signer,
            _ => panic!("Expected InMemorySigner"),
        };
        Self { account_id, signer }
    }
}

impl Wallet for SimulationWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}
