use crate::{Error, Result};

pub fn get(name: &str) -> Result<String> {
    std::env::var(name).or(Err(Error::missing_env_var(name)))
}
