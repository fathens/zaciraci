use crate::Result;
use anyhow::anyhow;
use common::config::ConfigAccess;
use logging::*;
use near_crypto::SecretKey::ED25519;
use near_crypto::{ED25519SecretKey, InMemorySigner};
use near_sdk::AccountId;

const CURVE: slipped10::Curve = slipped10::Curve::Ed25519;
const HARDEND: u32 = 1 << 31;

pub fn new_wallet(cfg: &impl ConfigAccess) -> StandardWallet {
    StandardWallet::new_from_config(cfg).expect("Failed to create wallet from config")
}

pub trait Wallet {
    fn account_id(&self) -> &AccountId;
    fn signer(&self) -> &InMemorySigner;
}

#[derive(Clone)]
pub struct StandardWallet {
    account_id: AccountId,
    mnemonic: bip39::Mnemonic,
    hdpath: slipped10::BIP32Path,
    signing_key: ed25519_dalek::SigningKey,
    signer: InMemorySigner,
}

impl StandardWallet {
    fn get_account_id(cfg: &impl ConfigAccess) -> Result<AccountId> {
        let strval = cfg.root_account_id()?;
        Ok(strval.parse()?)
    }

    fn get_mnemonic(cfg: &impl ConfigAccess) -> Result<bip39::Mnemonic> {
        let strval = cfg.root_mnemonic()?;
        Ok(strval.parse()?)
    }

    fn get_hdpath(cfg: &impl ConfigAccess) -> Result<slipped10::BIP32Path> {
        let strval = cfg.root_hdpath();
        strval.parse().map_err(|e| anyhow!("{}", e))
    }

    fn new(
        account_id: AccountId,
        mnemonic: bip39::Mnemonic,
        hdpath: slipped10::BIP32Path,
    ) -> Result<StandardWallet> {
        let log = DEFAULT.new(o!(
            "function" => "StandardWallet::new",
            "account_id" => format!("{}", account_id),
        ));
        debug!(log, "creating"; "hdpath" => %hdpath);
        let key = slipped10::derive_key_from_path(&mnemonic.to_seed(""), CURVE, &hdpath)
            .map_err(|e| anyhow!("{}", e))?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key.key);
        let seckey = ED25519SecretKey(signing_key.to_keypair_bytes());
        let signer_result = InMemorySigner::from_secret_key(account_id.clone(), ED25519(seckey));
        let signer = match signer_result {
            near_crypto::Signer::InMemory(signer) => signer,
            _ => return Err(anyhow!("Expected InMemorySigner")),
        };
        let wallet = StandardWallet {
            account_id,
            mnemonic,
            hdpath,
            signing_key,
            signer,
        };
        info!(log, "created"; "pubkey" => %wallet.pub_base58());
        Ok(wallet)
    }

    pub fn new_from_config(cfg: &impl ConfigAccess) -> Result<StandardWallet> {
        let account_id = Self::get_account_id(cfg)?;
        let mnemonic = Self::get_mnemonic(cfg)?;
        let hdpath = Self::get_hdpath(cfg)?;
        Self::new(account_id, mnemonic, hdpath)
    }

    pub fn pub_base58(&self) -> String {
        bs58::encode(self.signing_key.verifying_key()).into_string()
    }

    pub fn derive(&self, index: i32) -> Result<StandardWallet> {
        let mut hdpath = self.hdpath.clone();
        hdpath.push(index as u32 + HARDEND);
        Self::new(self.account_id.clone(), self.mnemonic.clone(), hdpath)
    }
}

impl Wallet for StandardWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}
