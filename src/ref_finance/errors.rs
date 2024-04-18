use near_jsonrpc_client::errors::JsonRpcError;
use near_jsonrpc_primitives::errors::RpcError;
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
