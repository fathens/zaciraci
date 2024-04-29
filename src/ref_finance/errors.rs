use std::fmt::Display;

#[derive(Debug, PartialEq)]
pub enum Error {
    // some errors
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error")
    }
}
