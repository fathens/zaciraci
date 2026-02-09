#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenIndex(usize);

impl TokenIndex {
    pub fn as_usize(&self) -> usize {
        self.0
    }

    pub fn as_u8(&self) -> u8 {
        self.0 as u8
    }

    pub fn max(self, other: Self) -> Self {
        if self.0 > other.0 { self } else { other }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_index_from_usize() {
        let idx = TokenIndex::from(5);
        assert_eq!(idx.as_usize(), 5);
        assert_eq!(idx.as_u8(), 5);
    }

    #[test]
    fn test_token_index_display() {
        let idx = TokenIndex::from(42);
        assert_eq!(format!("{idx}"), "42");
    }

    #[test]
    fn test_token_index_max() {
        let a = TokenIndex::from(3);
        let b = TokenIndex::from(7);
        assert_eq!(a.max(b), b);
        assert_eq!(b.max(a), b);
        assert_eq!(a.max(a), a);
    }

    #[test]
    fn test_token_index_ord() {
        let a = TokenIndex::from(1);
        let b = TokenIndex::from(2);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, TokenIndex::from(1));
    }

    #[test]
    fn test_token_in_from_usize() {
        let token_in = TokenIn::from(3);
        assert_eq!(token_in.as_usize(), 3);
        assert_eq!(token_in.as_index(), TokenIndex::from(3));
    }

    #[test]
    fn test_token_in_from_token_index() {
        let idx = TokenIndex::from(5);
        let token_in = TokenIn::from(idx);
        assert_eq!(token_in.as_index(), idx);
    }

    #[test]
    fn test_token_out_from_usize() {
        let token_out = TokenOut::from(2);
        assert_eq!(token_out.as_usize(), 2);
        assert_eq!(token_out.as_index(), TokenIndex::from(2));
    }

    #[test]
    fn test_token_out_from_token_index() {
        let idx = TokenIndex::from(7);
        let token_out = TokenOut::from(idx);
        assert_eq!(token_out.as_index(), idx);
    }
}
