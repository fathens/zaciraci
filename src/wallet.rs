use crate::config;
use crate::logging::*;
use crate::Result;

const DEFAULT_HDPATH: &str = "m/44'/397'/0'";
const CURVE: slipped10::Curve = slipped10::Curve::Ed25519;
const HARDEND: u32 = 1 << 31;

pub struct Wallet {
    mnemonic: bip39::Mnemonic,
    hdpath: slipped10::BIP32Path,
    signing_key: ed25519_dalek::SigningKey,
}

impl Wallet {
    fn get_mnemonic() -> Result<bip39::Mnemonic> {
        let strval = config::get("ROOT_MNEMONIC")?;
        Ok(strval.parse()?)
    }

    fn get_hdpath() -> Result<slipped10::BIP32Path> {
        let strval = config::get("ROOT_HDPATH").unwrap_or(DEFAULT_HDPATH.to_string());
        Ok(strval.parse()?)
    }

    fn new(mnemonic: bip39::Mnemonic, hdpath: slipped10::BIP32Path) -> Result<Wallet> {
        let log = DEFAULT.new(o!("function" => "Wallet::new"));
        debug!(log, "create"; "hdpath" => %hdpath);
        let key = slipped10::derive_key_from_path(&mnemonic.to_seed(""), CURVE, &hdpath)?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key.key);
        Ok(Wallet {
            mnemonic,
            hdpath,
            signing_key,
        })
    }

    pub fn new_from_config() -> Result<Wallet> {
        let mnemonic = Self::get_mnemonic()?;
        let hdpath = Self::get_hdpath()?;
        Self::new(mnemonic, hdpath)
    }

    pub fn pub_base58(&self) -> String {
        bs58::encode(self.signing_key.verifying_key()).into_string()
    }

    #[allow(dead_code)]
    pub fn derive(&self, index: i32) -> Result<Wallet> {
        let mut hdpath = self.hdpath.clone();
        hdpath.push(HARDEND);
        hdpath.push(index as u32 + HARDEND);
        Self::new(self.mnemonic.clone(), hdpath)
    }
}
