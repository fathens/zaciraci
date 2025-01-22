use crate::config;
use crate::logging::*;
use crate::Result;
use anyhow::anyhow;
use near_crypto::SecretKey::ED25519;
use near_crypto::{ED25519SecretKey, InMemorySigner};
use near_sdk::AccountId;
use once_cell::sync::Lazy;

const DEFAULT_HDPATH: &str = "m/44'/397'/0'";
const CURVE: slipped10::Curve = slipped10::Curve::Ed25519;
const HARDEND: u32 = 1 << 31;

pub static WALLET: Lazy<Wallet> = Lazy::new(|| {
    let log = DEFAULT.new(o!("function" => "wallet::WALLET"));
    match Wallet::new_from_config() {
        Ok(wallet) => wallet,
        Err(e) => {
            error!(log, "Failed to create wallet"; "error" => %e);
            panic!("Failed to create wallet: {}", e);
        }
    }
});

#[derive(Clone)]
pub struct Wallet {
    account_id: AccountId,
    mnemonic: bip39::Mnemonic,
    hdpath: slipped10::BIP32Path,
    signing_key: ed25519_dalek::SigningKey,
    signer: InMemorySigner,
}

impl Wallet {
    fn get_account_id() -> Result<AccountId> {
        let strval = config::get("ROOT_ACCOUNT_ID")?;
        Ok(strval.parse()?)
    }

    fn get_mnemonic() -> Result<bip39::Mnemonic> {
        let strval = config::get("ROOT_MNEMONIC")?;
        Ok(strval.parse()?)
    }

    fn get_hdpath() -> Result<slipped10::BIP32Path> {
        let strval = config::get("ROOT_HDPATH").unwrap_or(DEFAULT_HDPATH.to_string());
        strval.parse().map_err(|e| anyhow!("{}", e))
    }

    fn new(
        account_id: AccountId,
        mnemonic: bip39::Mnemonic,
        hdpath: slipped10::BIP32Path,
    ) -> Result<Wallet> {
        let log = DEFAULT.new(o!("function" => "Wallet::new"));
        debug!(log, "creating"; "hdpath" => %hdpath);
        let key = slipped10::derive_key_from_path(&mnemonic.to_seed(""), CURVE, &hdpath)
            .map_err(|e| anyhow!("{}", e))?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key.key);
        let seckey = ED25519SecretKey(signing_key.to_keypair_bytes());
        let signer = InMemorySigner::from_secret_key(account_id.clone(), ED25519(seckey));
        let wallet = Wallet {
            account_id,
            mnemonic,
            hdpath,
            signing_key,
            signer,
        };
        info!(log, "created"; "pubkey" => %wallet.pub_base58());
        Ok(wallet)
    }

    pub fn new_from_config() -> Result<Wallet> {
        let account_id = Self::get_account_id()?;
        let mnemonic = Self::get_mnemonic()?;
        let hdpath = Self::get_hdpath()?;
        Self::new(account_id, mnemonic, hdpath)
    }

    pub fn pub_base58(&self) -> String {
        bs58::encode(self.signing_key.verifying_key()).into_string()
    }

    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    pub fn derive(&self, index: i32) -> Result<Wallet> {
        let mut hdpath = self.hdpath.clone();
        hdpath.push(index as u32 + HARDEND);
        Self::new(self.account_id.clone(), self.mnemonic.clone(), hdpath)
    }

    pub fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}
