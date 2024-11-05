use crate::errors::Error;
use crate::Result;

pub fn get(name: &str) -> Result<String> {
    std::env::var(name).map_err(|err| Error::EnvironmentVariable {
        env_name: name.to_string(),
        err,
    })
}
