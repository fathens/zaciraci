use near_jsonrpc_client::errors::JsonRpcError;
use near_jsonrpc_primitives::errors::RpcError;
use near_primitives::account::id::ParseAccountError;
use std::fmt::{Debug, Display};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Plain(String),
    EnvironmentVariable {
        env_name: String,
        err: std::env::VarError,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Plain(msg) => write!(f, "{}", msg),
            Error::EnvironmentVariable { env_name, err } => {
                write!(f, "{}: {}", err, env_name)
            }
        }
    }
}

impl From<crate::ref_finance::errors::Error> for Error {
    fn from(e: crate::ref_finance::errors::Error) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<deadpool::managed::PoolError<deadpool_diesel::Error>> for Error {
    fn from(e: deadpool::managed::PoolError<deadpool_diesel::Error>) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<ParseAccountError> for Error {
    fn from(e: ParseAccountError) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<diesel::result::Error> for Error {
    fn from(e: diesel::result::Error) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<deadpool_diesel::InteractError> for Error {
    fn from(e: deadpool_diesel::InteractError) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<RpcError> for Error {
    fn from(e: RpcError) -> Error {
        Error::Plain(e.to_string())
    }
}

impl<E: Display> From<JsonRpcError<E>> for Error {
    fn from(e: JsonRpcError<E>) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<bip39::Error> for Error {
    fn from(e: bip39::Error) -> Error {
        Error::Plain(e.to_string())
    }
}

impl From<slipped10::Error> for Error {
    fn from(value: slipped10::Error) -> Self {
        Error::Plain(value.to_string())
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Error {
        Error::Plain(e.to_string())
    }
}
