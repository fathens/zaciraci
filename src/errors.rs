use near_jsonrpc_client::errors::JsonRpcError;
use near_jsonrpc_primitives::errors::RpcError;
use near_primitives::account::id::ParseAccountError;
use std::env::VarError;
use std::fmt::{Debug, Display};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<crate::ref_finance::errors::Error> for Error {
    fn from(e: crate::ref_finance::errors::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<VarError> for Error {
    fn from(e: VarError) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<deadpool::managed::PoolError<deadpool_diesel::Error>> for Error {
    fn from(e: deadpool::managed::PoolError<deadpool_diesel::Error>) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<ParseAccountError> for Error {
    fn from(e: ParseAccountError) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<diesel::result::Error> for Error {
    fn from(e: diesel::result::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<deadpool_diesel::InteractError> for Error {
    fn from(e: deadpool_diesel::InteractError) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<RpcError> for Error {
    fn from(e: RpcError) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl<E: Display> From<JsonRpcError<E>> for Error {
    fn from(e: JsonRpcError<E>) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}
