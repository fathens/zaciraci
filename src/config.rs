use crate::Result;
use anyhow::anyhow;

pub fn get(name: &str) -> Result<String> {
    match std::env::var(name) {
        Ok(val) => {
            if val.is_empty() {
                Err(anyhow!("{} is empty", name))
            } else {
                Ok(val)
            }
        }
        Err(e) => Err(anyhow!("{}: {}", e, name)),
    }
}
