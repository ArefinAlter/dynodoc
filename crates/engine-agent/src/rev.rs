//! Revision tokens (protocol spec 5.2).
//!
//! `rev` is the event `seq` a read was materialised at (`0` is the empty initial state).
//! `chain` is the first 8 hex characters of that event's `chain_hash`. Together they form
//! the HTTP ETag `"<rev>:<chain8>"`.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid revision token: {0}")]
pub struct RevParseError(pub &'static str);

/// Eight lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chain8(String);

impl Chain8 {
    /// The all-zero chain used at `rev=0`, the genesis `prev_chain_hash` of the log.
    pub fn zero() -> Self {
        Chain8("0".repeat(8))
    }

    /// First 8 hex characters of a (possibly full) hex chain hash.
    pub fn from_hex(hex: &str) -> Result<Self, RevParseError> {
        let head = hex
            .get(..8)
            .ok_or(RevParseError("chain shorter than 8 hex characters"))?;
        if !head.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(RevParseError("chain is not hex"));
        }
        Ok(Chain8(head.to_ascii_lowercase()))
    }

    /// First 4 bytes of a raw chain hash.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RevParseError> {
        let head = bytes
            .get(..4)
            .ok_or(RevParseError("chain shorter than 4 bytes"))?;
        Ok(Chain8(head.iter().map(|b| format!("{b:02x}")).collect()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Chain8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `(rev, chain8)`: identifies one materialisation of a file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevToken {
    pub rev: u64,
    pub chain: Chain8,
}

impl RevToken {
    pub fn new(rev: u64, chain: Chain8) -> Self {
        Self { rev, chain }
    }

    /// `"412:9f3a1c04"` including the quotes (the HTTP ETag form).
    pub fn etag(&self) -> String {
        format!("\"{self}\"")
    }

    /// Parse the quoted ETag form or the bare `412:9f3a1c04` form.
    pub fn parse_etag(s: &str) -> Result<Self, RevParseError> {
        let inner = match s.strip_prefix('"') {
            Some(rest) => rest
                .strip_suffix('"')
                .ok_or(RevParseError("unterminated quote"))?,
            None => s,
        };
        inner.parse()
    }
}

impl fmt::Display for RevToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.rev, self.chain)
    }
}

impl FromStr for RevToken {
    type Err = RevParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (rev, chain) = s
            .split_once(':')
            .ok_or(RevParseError("missing ':' between rev and chain"))?;
        if rev.is_empty() || (rev.len() > 1 && rev.starts_with('0')) {
            return Err(RevParseError("rev must be a plain decimal"));
        }
        if !rev.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RevParseError("rev must be a plain decimal"));
        }
        let rev = rev.parse().map_err(|_| RevParseError("rev out of range"))?;
        if chain.len() != 8 {
            return Err(RevParseError("chain must be exactly 8 hex characters"));
        }
        Ok(RevToken {
            rev,
            chain: Chain8::from_hex(chain)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_forms() {
        let t = RevToken::new(412, Chain8::from_hex("9F3A1C04deadbeef").unwrap());
        assert_eq!(t.etag(), "\"412:9f3a1c04\"");
        assert_eq!(RevToken::parse_etag(&t.etag()).unwrap(), t);
        assert_eq!(RevToken::parse_etag("412:9f3a1c04").unwrap(), t);
        assert_eq!(RevToken::new(0, Chain8::zero()).to_string(), "0:00000000");
    }

    #[test]
    fn rejects_malformed() {
        for bad in ["", "412", "412:", "412:9f3a1c0", "412:9f3a1c0g", "-1:9f3a1c04", "0412:9f3a1c04", "\"412:9f3a1c04", "412:9f3a1c04:1"] {
            assert!(RevToken::parse_etag(bad).is_err(), "{bad}");
        }
    }
}
