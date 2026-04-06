use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Reader,
    Writer,
}

impl Role {
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Writer)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Reader => write!(f, "reader"),
            Role::Writer => write!(f, "writer"),
        }
    }
}

impl FromStr for Role {
    type Err = ParseRoleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "reader" => Ok(Role::Reader),
            "writer" => Ok(Role::Writer),
            _ => Err(ParseRoleError),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseRoleError;

impl fmt::Display for ParseRoleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid role value")
    }
}

impl std::error::Error for ParseRoleError {}

#[cfg(test)]
mod tests;
