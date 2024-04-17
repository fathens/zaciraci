use std::env::VarError;
use std::fmt::{Debug, Display};

#[derive(Debug, PartialEq)]
pub struct Error {
    message: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<VarError> for Error {
    fn from(e: VarError) -> Error {
        Error {
            message: e.to_string(),
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
