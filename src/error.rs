use std::fmt::Debug;

#[derive(Debug, PartialEq)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn missing_env_var(var: &str) -> Self {
        Error {
            message: format!("Missing environment variable: {var}"),
        }
    }
}

impl From<tokio_postgres::Error> for Error {
    fn from(e: tokio_postgres::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}
