#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenIndex(usize);

impl TokenIndex {
    pub fn as_usize(&self) -> usize {
        self.0
    }

    pub fn max(self, other: Self) -> Self {
        if self.0 > other.0 {
            self
        } else {
            other
        }
    }
}

impl std::fmt::Display for TokenIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for TokenIndex {
    fn from(value: usize) -> Self {
        TokenIndex(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenIn(TokenIndex);

impl TokenIn {
    pub fn as_usize(&self) -> usize {
        self.0.as_usize()
    }

    pub fn as_index(&self) -> TokenIndex {
        self.0
    }
}

impl From<usize> for TokenIn {
    fn from(value: usize) -> Self {
        TokenIn(TokenIndex(value))
    }
}

impl From<TokenIndex> for TokenIn {
    fn from(value: TokenIndex) -> Self {
        TokenIn(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenOut(TokenIndex);

impl TokenOut {
    pub fn as_usize(&self) -> usize {
        self.0.as_usize()
    }

    pub fn as_index(&self) -> TokenIndex {
        self.0
    }
}

impl From<usize> for TokenOut {
    fn from(value: usize) -> Self {
        TokenOut(TokenIndex(value))
    }
}

impl From<TokenIndex> for TokenOut {
    fn from(value: TokenIndex) -> Self {
        TokenOut(value)
    }
}
