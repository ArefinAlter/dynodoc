//! Position keys (protocol spec 8.2): the Rust port of `between(left, right)` in
//! `frontend/researcher/src/lib/workspace/changes.ts`.
//!
//! A `pos` is a lexicographic string; a new node gets a key between its neighbours without
//! moving any other node. Two writers (the web editor and this crate) must derive the same
//! key for the same intent, so the function is a transliteration, not a redesign:
//! [`between`] returns, for every input, the string the TypeScript function returns
//! (`tests/vectors/between.json` holds 10,000 pairs and five long chains produced by RUNNING
//! the real function, see `tests/vectors/between-gen.mjs`).
//!
//! The algorithm walks both keys one UTF-16 code unit at a time. A missing or NUL unit of
//! `left` counts as 32 (space), a missing or NUL unit of `right` as 126 (`~`). At the first
//! position where the two units are more than one apart it returns the prefix so far plus
//! their midpoint. After 200 positions without such a gap it returns `left + "P"`.
//!
//! Known properties of the original, kept on purpose:
//!
//! * for keys of printable ASCII without a space (U+0021..U+007E) and `left < right`, a key
//!   found inside the loop is strictly between the two, and at most 200 characters long;
//! * the fallback `left + "P"` is strictly below `right` only when the two keys differ within
//!   their first 200 characters; for two keys that share 200 or more it can be at or above
//!   `right`. It is also longer than 200 characters when `left` already has 200 or more;
//! * `right` with a unit at or below 32 is outside that domain: `between("a", "a ")` is
//!   `"a O"`, which sorts after `"a "`.
//!
//! [`key_between`] is what the compiler calls: it takes the same key and refuses it unless it
//! really sorts strictly between the neighbours in the order the engine uses (`(pos, id)`,
//! byte order of the UTF-8 strings) and is at most 200 characters long. A hostile or odd
//! stored key can therefore never make the compiler emit an order the reader would not
//! honour, and repeated inserts at one spot cannot grow a key without bound (the original's
//! fallback appends one character per call).

/// The loop of the original examines at most this many positions.
pub const MAX_KEY_STEPS: usize = 200;

/// The key the original `between(left, right)` returns.
///
/// `right` is `None` for "no right neighbour" (`undefined` in TypeScript); `Some("")` behaves
/// the same. The original works on UTF-16 code units and can return a lone surrogate, which
/// is not a Rust `str`; such a result (it needs two stored keys with units on both sides of
/// the surrogate block) comes back with U+FFFD in its place. Use [`try_between`] to learn
/// about it, or [`key_between`], which refuses it.
pub fn between(left: &str, right: Option<&str>) -> String {
    String::from_utf16_lossy(&units(left, right))
}

/// [`between`], or `None` when the original's result is not a well-formed string.
pub fn try_between(left: &str, right: Option<&str>) -> Option<String> {
    String::from_utf16(&units(left, right)).ok()
}

fn units(left: &str, right: Option<&str>) -> Vec<u16> {
    let l: Vec<u16> = left.encode_utf16().collect();
    let r: Vec<u16> = right
        .map(|r| r.encode_utf16().collect())
        .unwrap_or_default();
    let mut prefix: Vec<u16> = Vec::new();
    for i in 0..MAX_KEY_STEPS {
        let a = l.get(i).copied().filter(|u| *u != 0).map_or(32, i32::from);
        let b = r.get(i).copied().filter(|u| *u != 0).map_or(126, i32::from);
        if b - a > 1 {
            // `a < b`, both below 65,536: the midpoint is a code unit.
            prefix.push(((a + b) / 2) as u16);
            return prefix;
        }
        prefix.push(a as u16);
    }
    let mut out: Vec<u16> = l;
    out.push(u16::from(b'P'));
    out
}

/// A key for a new node between `left` and `right` (`None`: the start or the end of the
/// list), or `None` when the original function would not give a key that sorts strictly
/// between them in the order the engine uses, or one longer than [`MAX_KEY_STEPS`] characters.
pub fn key_between(left: Option<&str>, right: Option<&str>) -> Option<String> {
    let key = try_between(left.unwrap_or(""), right)?;
    let above = left.is_none_or(|l| l < key.as_str());
    let below = right.is_none_or(|r| key.as_str() < r);
    (above && below && key.chars().count() <= MAX_KEY_STEPS).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_examples() {
        assert_eq!(between("", None), "O");
        assert_eq!(between("a", Some("b")), "aO");
        // The sheet and import keys are zero-padded numbers.
        assert_eq!(between("00000010", Some("00000020")), "00000010O");
        assert_eq!(between("00000099", None), "W");
    }

    #[test]
    fn nul_and_missing_units_default() {
        // A NUL in `left` counts as a space, a NUL in `right` as `~`.
        assert_eq!(between("\u{0}", None), "O");
        assert_eq!(between("a", Some("\u{0}")), "o");
        assert_eq!(between("", Some("")), "O");
    }

    #[test]
    fn utf16_units_not_scalars() {
        // U+1F600 is the surrogate pair D83D DE00; the key keeps both units.
        assert_eq!(between("\u{1F600}", None), "\u{1F600}O");
        assert_eq!(between("a", Some("\u{1F600}")), "\u{6C4F}");
    }

    #[test]
    fn the_fallback_after_200_positions() {
        let left = "}".repeat(200);
        assert_eq!(between(&left, None), format!("{left}P"));
        // 199 positions with no gap and a 200th that has one: still found in the loop.
        let left = "}".repeat(199);
        assert_eq!(between(&left, None), format!("{left}O"));
    }

    #[test]
    fn lone_surrogates_are_reported() {
        // Units D7FF and E000 have the surrogate block between them: midpoint DBFF.
        assert_eq!(try_between("\u{D7FF}", Some("\u{E000}")), None);
        assert!(between("\u{D7FF}", Some("\u{E000}")).contains('\u{FFFD}'));
        assert_eq!(key_between(Some("\u{D7FF}"), Some("\u{E000}")), None);
    }

    #[test]
    fn key_between_checks_the_order() {
        assert_eq!(key_between(None, None).as_deref(), Some("O"));
        assert_eq!(key_between(Some("a"), Some("b")).as_deref(), Some("aO"));
        // The mirrored quirk: "a O" sorts after "a ", so it is refused.
        assert_eq!(between("a", Some("a ")), "a O");
        assert_eq!(key_between(Some("a"), Some("a ")), None);
        // Nothing sorts below the empty key.
        assert_eq!(key_between(None, Some("")), None);
        // Equal neighbours leave no room.
        assert_eq!(key_between(Some("a"), Some("a")), None);
    }
}
