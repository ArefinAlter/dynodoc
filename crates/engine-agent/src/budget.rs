//! Truncation and the byte budget (protocol spec 6.10, 11).
//!
//! Every list-like response stops at the first of the caller's `limit`, the caller's
//! `max_bytes` or the server cap, then ends with `more=<cursor>` and `warn=truncated`.
//! The budget counts the bytes of the whole response (header included). A response never
//! holds a partial block: if not even the first unit fits, the call fails with
//! `limit_exceeded field=max_bytes` instead of returning nothing and a cursor that
//! would never advance.

use std::fmt;
use std::str::FromStr;

use crate::dct::HeaderBuilder;
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::refs::Ref;
use engine_shared::NodeId;

pub const DEFAULT_MAX_BYTES: usize = 16_000;
pub const MIN_MAX_BYTES: usize = 500;
pub const MAX_MAX_BYTES: usize = 64_000;
pub const DIFF_DEFAULT_MAX_BYTES: usize = 8_000;
pub const OUTLINE_DEFAULT_LIMIT: usize = 200;
pub const OUTLINE_MAX_LIMIT: usize = 1_000;
pub const FIND_DEFAULT_LIMIT: usize = 20;
pub const FIND_MAX_LIMIT: usize = 100;
pub const FILES_DEFAULT_LIMIT: usize = 20;
pub const FILES_MAX_LIMIT: usize = 50;
pub const MAX_REFS: usize = 100;
pub const MAX_SLIDES: usize = 50;
pub const PREVIEW_DEFAULT: usize = 48;
pub const PREVIEW_MAX: usize = 120;
pub const CONTEXT_DEFAULT: usize = 30;
pub const CONTEXT_MAX: usize = 120;
pub const MAX_QUERY_CHARS: usize = 200;

/// Validate a caller-supplied `max_bytes` against the spec range (500 to 64,000).
pub fn resolve_max_bytes(requested: Option<usize>, default: usize) -> Result<usize, AgentError> {
    match requested {
        None => Ok(default),
        Some(n) if n < MIN_MAX_BYTES => Err(AgentError::BadRequest { field: "max_bytes" }),
        Some(n) if n > MAX_MAX_BYTES => Err(AgentError::LimitExceeded {
            field: "max_bytes",
            limit: MAX_MAX_BYTES as u64,
            value: n as u64,
        }),
        Some(n) => Ok(n),
    }
}

/// Validate a `limit`: `1..=max`, default when absent.
pub fn resolve_limit(
    requested: Option<usize>,
    default: usize,
    max: usize,
) -> Result<usize, AgentError> {
    match requested {
        None => Ok(default),
        Some(0) => Err(AgentError::BadRequest { field: "limit" }),
        Some(n) if n > max => Err(AgentError::LimitExceeded {
            field: "limit",
            limit: max as u64,
            value: n as u64,
        }),
        Some(n) => Ok(n),
    }
}

/// An opaque continuation: `(rev, position)` plus one tool-specific number (the `from`
/// revision of a `diff`). Not secret and not authenticated: it carries no authority, and
/// every call re-authorises (invariant I3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor {
    pub rev: u64,
    pub pos: u64,
    pub aux: Option<u64>,
}

impl Cursor {
    pub fn new(rev: u64, pos: u64) -> Self {
        Self {
            rev,
            pos,
            aux: None,
        }
    }

    /// A cursor pins its revision: a continuation never mixes revisions. The caller reads
    /// the state at [`Cursor::rev`]; a mismatch is a caller error.
    pub fn check_rev(&self, rev: u64) -> Result<(), AgentError> {
        if self.rev == rev {
            Ok(())
        } else {
            Err(AgentError::BadRequest { field: "cursor" })
        }
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "c_{:x}.{:x}", self.rev, self.pos)?;
        if let Some(a) = self.aux {
            write!(f, ".{a:x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid cursor")]
pub struct CursorParseError;

impl FromStr for Cursor {
    type Err = CursorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Canonical lowercase hex without leading zeros, so each cursor has one spelling.
        fn hex(part: &str) -> Result<u64, CursorParseError> {
            let canonical = !part.is_empty()
                && part.len() <= 16
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && (part == "0" || !part.starts_with('0'));
            if !canonical {
                return Err(CursorParseError);
            }
            u64::from_str_radix(part, 16).map_err(|_| CursorParseError)
        }
        let rest = s.strip_prefix("c_").ok_or(CursorParseError)?;
        let parts: Vec<&str> = rest.split('.').collect();
        match parts.as_slice() {
            [rev, pos] => Ok(Cursor::new(hex(rev)?, hex(pos)?)),
            [rev, pos, aux] => Ok(Cursor {
                rev: hex(rev)?,
                pos: hex(pos)?,
                aux: Some(hex(aux)?),
            }),
            _ => Err(CursorParseError),
        }
    }
}

/// Parse an optional cursor string and pin it to `rev`; returns the start position.
pub fn start_position(
    cursor: Option<&str>,
    rev: u64,
) -> Result<(usize, Option<Cursor>), AgentError> {
    match cursor {
        None => Ok((0, None)),
        Some(raw) => {
            let c: Cursor = raw
                .parse()
                .map_err(|_| AgentError::BadRequest { field: "cursor" })?;
            c.check_rev(rev)?;
            let pos =
                usize::try_from(c.pos).map_err(|_| AgentError::BadRequest { field: "cursor" })?;
            Ok((pos, Some(c)))
        }
    }
}

/// One atomic piece of a response body (a block, a table row group, a hit).
#[derive(Debug, Clone, Default)]
pub struct Unit {
    /// Complete lines, each ending with LF.
    pub text: String,
    /// Characters dropped from document text while rendering this unit.
    pub dropped: Dropped,
    /// Refs shown in this unit and the node ids they name (for `_meta.ids`).
    pub ids: Vec<(Ref, NodeId)>,
}

/// How many of `units[start..]` fit: at most `limit` of them, and the whole response
/// (header from `header(taken, more)`, `fixed` body bytes that are always present such
/// as a column-header line, plus the units) at most `max_bytes`. `header` must build the
/// exact header of a response holding `taken` units where `more` says whether units
/// remain after them.
pub fn fit(
    units: &[Unit],
    start: usize,
    limit: usize,
    max_bytes: usize,
    fixed: usize,
    header: impl Fn(usize, bool) -> HeaderBuilder,
) -> Result<usize, AgentError> {
    let avail = units.get(start..).unwrap_or(&[]);
    let mut body = fixed;
    let mut taken = 0usize;
    for (i, unit) in avail.iter().take(limit).enumerate() {
        let k = i + 1;
        let more = start + k < units.len();
        let len = header(k, more).assembled_len(body + unit.text.len());
        if len > max_bytes {
            if taken == 0 {
                return Err(AgentError::LimitExceeded {
                    field: "max_bytes",
                    limit: max_bytes as u64,
                    value: len as u64,
                });
            }
            break;
        }
        body += unit.text.len();
        taken = k;
    }
    Ok(taken)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dct::Tool;

    #[test]
    fn cursor_round_trip_and_strictness() {
        for c in [
            Cursor::new(0, 0),
            Cursor::new(412, 200),
            Cursor {
                rev: 9,
                pos: 3,
                aux: Some(7),
            },
        ] {
            assert_eq!(c.to_string().parse::<Cursor>(), Ok(c));
        }
        assert_eq!(Cursor::new(412, 200).to_string(), "c_19c.c8");
        for bad in [
            "",
            "c_",
            "c_1",
            "c_1.",
            "c_01.2",
            "c_1.2.3.4",
            "x_1.2",
            "c_g.1",
            "c_1.-2",
            "c_1.2.",
        ] {
            assert!(bad.parse::<Cursor>().is_err(), "{bad}");
        }
    }

    fn unit(n: usize) -> Unit {
        Unit {
            text: format!("{}\n", "x".repeat(n - 1)),
            ..Unit::default()
        }
    }

    fn hdr(taken: usize, more: bool) -> HeaderBuilder {
        let h = HeaderBuilder::new(Tool::Outline).kv("n", taken);
        if more {
            h.kv("more", Cursor::new(1, taken as u64))
                .kv("warn", "truncated")
        } else {
            h
        }
    }

    #[test]
    fn fit_is_exact_at_the_boundary() {
        let units: Vec<Unit> = (0..10).map(|_| unit(100)).collect();
        // The largest budget that fits k units is exactly the assembled length.
        for k in 1..=9usize {
            let exact = hdr(k, true).assembled_len(k * 100);
            assert_eq!(fit(&units, 0, 50, exact, 0, hdr).unwrap(), k, "k={k} exact");
            match fit(&units, 0, 50, exact - 1, 0, hdr) {
                Ok(n) => assert_eq!(n, k - 1, "k={k} minus one"),
                Err(e) => assert!(k == 1 && matches!(e, AgentError::LimitExceeded { .. })),
            }
        }
        // All ten fit with the complete header (no `more`).
        let all = hdr(10, false).assembled_len(1_000);
        assert_eq!(fit(&units, 0, 50, all, 0, hdr).unwrap(), 10);
    }

    #[test]
    fn fit_reports_a_unit_that_never_fits() {
        let units = vec![unit(2_000)];
        match fit(&units, 0, 10, 500, 0, hdr) {
            Err(AgentError::LimitExceeded {
                field,
                limit,
                value,
            }) => {
                assert_eq!((field, limit), ("max_bytes", 500));
                assert!(value > 2_000);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(fit(&[], 0, 10, 500, 0, hdr), Ok(0));
        assert_eq!(fit(&units, 5, 10, 500, 0, hdr), Ok(0));
    }

    #[test]
    fn fit_respects_limit_and_start() {
        let units: Vec<Unit> = (0..10).map(|_| unit(10)).collect();
        assert_eq!(fit(&units, 0, 3, 64_000, 0, hdr), Ok(3));
        assert_eq!(fit(&units, 8, 5, 64_000, 0, hdr), Ok(2));
    }

    #[test]
    fn max_bytes_range() {
        assert_eq!(resolve_max_bytes(None, 16_000), Ok(16_000));
        assert_eq!(resolve_max_bytes(Some(500), 1), Ok(500));
        assert_eq!(resolve_max_bytes(Some(64_000), 1), Ok(64_000));
        assert!(matches!(
            resolve_max_bytes(Some(499), 1),
            Err(AgentError::BadRequest { .. })
        ));
        assert!(matches!(
            resolve_max_bytes(Some(64_001), 1),
            Err(AgentError::LimitExceeded { .. })
        ));
    }
}
