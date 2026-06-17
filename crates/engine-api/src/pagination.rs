//! Cursor pagination (docs/18 §5).
//!
//! Offset pagination breaks under concurrent writes: a page boundary shifts when a row
//! is inserted before it, so a client misses or repeats items. A cursor is a stable
//! reference to a position in an ordered stream — `(last_seq, last_id)` — so paging is
//! correct even while the underlying list grows.
//!
//! The cursor is **opaque** to clients: it is the URL-safe base64 of a small JSON
//! `(last_seq, last_id)` tuple. Clients pass back `next_cursor` verbatim; they never
//! construct or interpret it. A list response is `{ items, next_cursor }`, with
//! `next_cursor` null on the last page.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// The default page size when `?limit=` is absent.
pub const DEFAULT_LIMIT: i64 = 50;
/// The hard ceiling on `?limit=` (docs/18: events page max 1000).
pub const MAX_LIMIT: i64 = 1000;

/// A stable position in an ordered result stream. `last_seq` is the primary order key
/// (event seq, or a row's natural order); `last_id` disambiguates ties and pins the
/// exact row so the next page starts strictly after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub last_seq: i64,
    pub last_id: String,
}

impl Cursor {
    /// Encode to the opaque URL-safe base64 form returned as `next_cursor`.
    pub fn encode(&self) -> String {
        // serializing a two-field struct is infallible.
        let json = serde_json::to_vec(self).expect("cursor serialization is infallible");
        URL_SAFE_NO_PAD.encode(json)
    }

    /// Decode a client-supplied cursor. Returns `None` for any malformed input (the
    /// handler treats that as a 400, never a panic).
    pub fn decode(raw: &str) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(raw.as_bytes()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// A client supplied a cursor that is not valid base64-JSON (⇒ 400 at the boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("malformed cursor")]
pub struct CursorError;

/// Common list query parameters: an opaque `cursor` and a bounded `limit`.
#[derive(Debug, Default, Deserialize)]
pub struct PageParams {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

impl PageParams {
    /// The effective page size: the requested `limit` clamped to `1..=MAX_LIMIT`,
    /// defaulting to [`DEFAULT_LIMIT`].
    pub fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// Decode the cursor, distinguishing "absent" (`Ok(None)`) from "present but
    /// malformed" (`Err`) so the handler can 400 the latter.
    pub fn decoded_cursor(&self) -> Result<Option<Cursor>, CursorError> {
        match self.cursor.as_deref() {
            None | Some("") => Ok(None),
            Some(raw) => Cursor::decode(raw).map(Some).ok_or(CursorError),
        }
    }
}

/// A uniform paginated list response: the page of `items` plus the cursor to fetch the
/// next page (null when the stream is exhausted).
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// Build a page. `next` is the cursor to the row after the last returned item, set
    /// only when the page was full (so there may be more).
    pub fn new(items: Vec<T>, next: Option<Cursor>) -> Self {
        Page {
            items,
            next_cursor: next.map(|c| c.encode()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_through_opaque_encoding() {
        let c = Cursor {
            last_seq: 42,
            last_id: "01ABCXYZ".into(),
        };
        let encoded = c.encode();
        // Opaque: no raw seq/id visible, URL-safe (no '+', '/', '=').
        assert!(!encoded.contains('='));
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert_eq!(Cursor::decode(&encoded), Some(c));
    }

    #[test]
    fn malformed_cursor_decodes_to_none() {
        assert_eq!(Cursor::decode("!!!not-base64!!!"), None);
        assert_eq!(Cursor::decode("YWJj"), None); // valid base64, not a cursor
    }

    #[test]
    fn limit_is_clamped_to_bounds() {
        assert_eq!(PageParams::default().effective_limit(), DEFAULT_LIMIT);
        assert_eq!(
            PageParams {
                cursor: None,
                limit: Some(0)
            }
            .effective_limit(),
            1
        );
        assert_eq!(
            PageParams {
                cursor: None,
                limit: Some(99_999)
            }
            .effective_limit(),
            MAX_LIMIT
        );
    }

    #[test]
    fn decoded_cursor_distinguishes_absent_from_malformed() {
        assert_eq!(PageParams::default().decoded_cursor(), Ok(None));
        let bad = PageParams {
            cursor: Some("###".into()),
            limit: None,
        };
        assert_eq!(bad.decoded_cursor(), Err(CursorError));
    }
}
