use std::fmt::Debug;

#[derive(Debug, PartialEq)]
pub struct Error {
    message: String,
}

impl From<speedb::Error> for Error {
    fn from(e: speedb::Error) -> Error {
        Error {
            message: e.to_string(),
        }
    }
}
