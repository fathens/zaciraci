use near_crypto::{KeyType, PublicKey, Signature};
use near_primitives::hash::CryptoHash;
use near_primitives::views::{
    ExecutionMetadataView, ExecutionOutcomeView, ExecutionOutcomeWithIdView, ExecutionStatusView,
    FinalExecutionOutcomeView, FinalExecutionStatus, SignedTransactionView,
};
use near_sdk::{AccountId, NearToken};

/// Create a dummy `FinalExecutionOutcomeView` with the given success value.
///
/// `success_value` is the bytes stored in `FinalExecutionStatus::SuccessValue`.
/// For REF Finance swap, this is typically a JSON-encoded `U128` like `b"\"12345\""`.
pub fn dummy_final_outcome(success_value: Vec<u8>) -> FinalExecutionOutcomeView {
    let account_id: AccountId = "mock.near".parse().expect("valid account id");
    let outcome = ExecutionOutcomeView {
        logs: vec![],
        receipt_ids: vec![],
        gas_burnt: near_primitives::types::Gas::from_gas(0),
        tokens_burnt: NearToken::from_yoctonear(0),
        executor_id: account_id.clone(),
        status: ExecutionStatusView::SuccessValue(success_value.clone()),
        metadata: ExecutionMetadataView {
            version: 1,
            gas_profile: None,
        },
    };
    FinalExecutionOutcomeView {
        status: FinalExecutionStatus::SuccessValue(success_value),
        transaction: SignedTransactionView {
            signer_id: account_id.clone(),
            public_key: PublicKey::empty(KeyType::ED25519),
            nonce: 0,
            receiver_id: account_id,
            actions: vec![],
            priority_fee: 0,
            signature: Signature::default(),
            hash: CryptoHash::default(),
        },
        transaction_outcome: ExecutionOutcomeWithIdView {
            proof: vec![],
            block_hash: CryptoHash::default(),
            id: CryptoHash::default(),
            outcome,
        },
        receipts_outcome: vec![],
    }
}
