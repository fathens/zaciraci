use crate::Result;
use anyhow::anyhow;

pub fn get(name: &str) -> Result<String> {
    std::env::var(name).map_err(|err| anyhow!("{}: {}", err, name))
}
