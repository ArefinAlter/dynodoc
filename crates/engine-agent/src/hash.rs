//! Hashes (protocol spec 5.4).
//!
//! Every hash is [`engine_core::richtext::fingerprint`]: SHA-256 over the canonical JSON
//! of a value (keys sorted, no whitespace). Block `h` is the fingerprint of the block's
//! `content`, so it equals `revision_hash` of `GET /documents/{id}/structure`. The other
//! hashes fingerprint a small JSON value defined below. Output is truncated to 6 hex
//! characters in text and 12 in `base` preconditions; a truncated hash is a cache key,
//! not a security control.

use engine_core::richtext::fingerprint;
use serde_json::{json, Value};

use crate::refs::Ref;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(String);

impl Hash {
    /// Fingerprint of an arbitrary JSON value.
    pub fn of(value: &Value) -> Hash {
        Hash(fingerprint(value))
    }

    /// A full 64-hex hash.
    pub fn from_hex(hex: &str) -> Option<Hash> {
        (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Hash(hex.to_ascii_lowercase()))
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// First 6 hex characters (the text form).
    pub fn h6(&self) -> &str {
        &self.0[..6]
    }

    /// First 12 hex characters (the `base` precondition form).
    pub fn h12(&self) -> &str {
        &self.0[..12]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid hash: {0}")]
pub struct HashParseError(pub &'static str);

/// A hash prefix a client holds (`have` values, `base`): 6 to 64 lowercase hex
/// characters, optionally written with a leading `#`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShortHash(String);

impl ShortHash {
    pub fn parse(s: &str) -> Result<Self, HashParseError> {
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() < 6 || s.len() > 64 {
            return Err(HashParseError("length must be 6 to 64 hex characters"));
        }
        if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(HashParseError("not hex"));
        }
        Ok(ShortHash(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `full` starts with this prefix.
    pub fn matches(&self, full: &Hash) -> bool {
        full.0.starts_with(&self.0)
    }
}

impl std::fmt::Display for ShortHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for ShortHash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Block `h`: fingerprint of the block's `content` field.
pub fn block_hash(content: &Value) -> Hash {
    Hash::of(content)
}

/// Row `h` (sheets): fingerprint of `{"cell_0": "...", ..., "cell_{n-1}": "..."}`.
/// Formats are excluded. `cells` are the row's cell strings for the sheet's used columns.
pub fn row_hash(cells: &[String]) -> Hash {
    let obj: serde_json::Map<String, Value> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| (format!("cell_{i}"), Value::String(c.clone())))
        .collect();
    Hash::of(&Value::Object(obj))
}

/// Range `h`: fingerprint of the 2-D array of cell strings in the range.
pub fn range_hash(rows: &[Vec<String>]) -> Hash {
    Hash::of(&json!(rows))
}

/// Shape `h` (slides): fingerprint of `{"kind", "role", "text"}`; `role` is `null` when
/// the shape has none.
pub fn shape_hash(kind: &str, role: Option<&str>, text: &str) -> Hash {
    Hash::of(&json!({"kind": kind, "role": role, "text": text}))
}

/// Slide notes `h`: fingerprint of `{"notes": ...}`.
pub fn notes_hash(notes: &str) -> Hash {
    Hash::of(&json!({ "notes": notes }))
}

/// Outline `oh`: fingerprint of the JSON string `"REF:h6,REF:h6,...;n=<count>"`, over the
/// blocks of the outline in order (before paging). The spec says "SHA-256 over the
/// concatenation of `ref:h` pairs in outline order, plus block count"; hashing the JSON
/// string literal rather than the raw bytes keeps one hash primitive for all objects.
pub fn outline_hash(pairs: &[(Ref, Hash)]) -> Hash {
    let mut s = String::with_capacity(pairs.len() * 12 + 16);
    for (i, (r, h)) in pairs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(r.as_str());
        s.push(':');
        s.push_str(h.h6());
    }
    s.push_str(";n=");
    s.push_str(&pairs.len().to_string());
    Hash::of(&Value::String(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_hash_is_the_engine_fingerprint() {
        let content = json!({"type": "paragraph", "content": [{"type": "text", "text": "hi"}]});
        assert_eq!(block_hash(&content).as_hex(), fingerprint(&content));
        assert_eq!(block_hash(&content).as_hex().len(), 64);
        assert_eq!(block_hash(&content).h6().len(), 6);
        assert_eq!(block_hash(&content).h12().len(), 12);
    }

    #[test]
    fn short_hash_matching() {
        let h = Hash::of(&json!("x"));
        let s = ShortHash::parse(&format!("#{}", h.h6().to_ascii_uppercase())).unwrap();
        assert!(s.matches(&h));
        assert!(ShortHash::parse(h.h12()).unwrap().matches(&h));
        assert!(!ShortHash::parse("000000").unwrap().matches(&h));
        for bad in ["", "12345", "zzzzzz", "#abcde", &"a".repeat(65)] {
            assert!(ShortHash::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn derived_hashes_are_canonical_json() {
        // Key order inside the JSON must not matter: the fingerprint sorts keys.
        let a = row_hash(&["a".into(), "b".into()]);
        assert_eq!(a, Hash::of(&json!({"cell_1": "b", "cell_0": "a"})));
        assert_ne!(a, row_hash(&["a".into(), "b".into(), String::new()]));
        assert_eq!(
            range_hash(&[vec!["1".into()]]),
            Hash::of(&json!([["1"]]))
        );
        assert_ne!(shape_hash("text", None, "x"), shape_hash("text", Some("title"), "x"));
        assert_ne!(notes_hash("a"), notes_hash("b"));
    }
}
