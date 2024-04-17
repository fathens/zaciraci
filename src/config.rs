use crate::Result;

pub fn get(name: &str) -> Result<String> {
    Ok(std::env::var(name)?)
}
