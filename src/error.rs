use std::fmt::{Debug, Display};

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

impl<E: Display> From<deadpool::managed::PoolError<E>> for Error {
    fn from(e: deadpool::managed::PoolError<E>) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}
