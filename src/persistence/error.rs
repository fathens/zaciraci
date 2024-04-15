use std::fmt::Debug;

#[derive(Debug, PartialEq)]
pub struct Error {
    message: String,
}

impl From<tokio_postgres::Error> for Error {
    fn from(e: tokio_postgres::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}
