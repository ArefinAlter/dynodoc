//! Refs and addresses (protocol spec 5.3, 5.5, 5.6).
//!
//! A [`Ref`] is the shortest suffix of a node ULID that is unique among all nodes of a
//! file at a revision (tombstones included), at least 4 characters, uppercase. The suffix
//! is used because ULIDs created in one import share their 10-character timestamp prefix.
//! There is no ref table: [`RefIndex`] resolves a ref by suffix match over the nodes that
//! existed at the revision, so a node created later can never match.

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Bound;
use std::str::FromStr;

use engine_core::materializer::DocumentState;
use engine_shared::NodeId;

use crate::a1::CellRef;

pub const MIN_REF_LEN: usize = 4;
pub const MAX_REF_LEN: usize = 26;
/// Longest list of candidates in an `ambiguous_ref` error.
pub const MAX_CANDIDATES: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid ref: {0}")]
pub struct RefParseError(pub &'static str);

/// An uppercase alphanumeric string of 4 to 26 characters. Real refs are suffixes of
/// ULIDs, so they use the Crockford alphabet, but any ASCII alphanumeric is accepted on
/// input and simply fails to resolve (`unknown_ref`) when no node matches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ref(String);

impl Ref {
    /// Parse (case-insensitive) and uppercase.
    pub fn parse(s: &str) -> Result<Ref, RefParseError> {
        if s.len() < MIN_REF_LEN || s.len() > MAX_REF_LEN {
            return Err(RefParseError("length must be 4 to 26 characters"));
        }
        if !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(RefParseError("only letters and digits are allowed"));
        }
        Ok(Ref(s.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Ref {
    type Err = RefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ref::parse(s)
    }
}

/// Why a ref did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Unknown,
    /// More than one node ends with the ref; shortest unique candidates (at most
    /// [`MAX_CANDIDATES`]).
    Ambiguous(Vec<Ref>),
}

fn reversed_upper(id: &str) -> String {
    id.chars().rev().flat_map(char::to_uppercase).collect()
}

fn addressable(id: &str) -> bool {
    id.len() >= MIN_REF_LEN
        && id.len() <= MAX_REF_LEN
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Suffix index over node ids. Keys are `(reversed uppercase id, id)`, so nodes that
/// share a suffix are adjacent and the minimal unique suffix length is decided by the
/// two neighbours.
#[derive(Debug, Clone, Default)]
pub struct RefIndex {
    keys: BTreeSet<(String, String)>,
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

impl RefIndex {
    /// Index the given ids. Ids that are not 4 to 26 ASCII alphanumerics are not
    /// addressable and are skipped (real node ids are 26-character ULIDs).
    pub fn build<'a>(ids: impl IntoIterator<Item = &'a NodeId>) -> Self {
        Self {
            keys: ids
                .into_iter()
                .filter(|id| addressable(&id.0))
                .map(|id| (reversed_upper(&id.0), id.0.clone()))
                .collect(),
        }
    }

    /// Every node of the state, deleted ones included.
    pub fn from_state(state: &DocumentState) -> Self {
        Self::build(state.nodes.keys())
    }

    /// Union of several states, so a ref is unique in each of them (used by `diff`).
    pub fn from_states(states: &[&DocumentState]) -> Self {
        Self::build(states.iter().flat_map(|s| s.nodes.keys()))
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The shortest unique suffix of `id` (at least 4 characters), or `None` when the id
    /// is unknown to the index or not addressable.
    pub fn ref_of(&self, id: &NodeId) -> Option<Ref> {
        let key = (reversed_upper(&id.0), id.0.clone());
        if !self.keys.contains(&key) {
            return None;
        }
        let prev = self.keys.range(..key.clone()).next_back();
        let next = self
            .keys
            .range((Bound::Excluded(key.clone()), Bound::Unbounded))
            .next();
        let shared = [prev, next]
            .into_iter()
            .flatten()
            .map(|(k, _)| common_prefix(&key.0, k))
            .max()
            .unwrap_or(0);
        let len = (shared + 1).max(MIN_REF_LEN).min(key.0.len());
        let upper: String = key.0.chars().rev().collect();
        Some(Ref(upper[upper.len() - len..].to_string()))
    }

    /// Resolve a ref (or a full ULID) to the node it names at this revision.
    pub fn resolve(&self, r: &Ref) -> Result<NodeId, ResolveError> {
        let target = reversed_upper(r.as_str());
        let hits: Vec<&(String, String)> = self
            .keys
            .range((target.clone(), String::new())..)
            .take_while(|(k, _)| k.starts_with(&target))
            .take(MAX_CANDIDATES + 1)
            .collect();
        if let Some((_, id)) = hits.iter().find(|(k, _)| *k == target) {
            return Ok(NodeId(id.clone()));
        }
        match hits.as_slice() {
            [] => Err(ResolveError::Unknown),
            [(_, id)] => Ok(NodeId(id.clone())),
            many => Err(ResolveError::Ambiguous(
                many.iter()
                    .take(MAX_CANDIDATES)
                    .filter_map(|(_, id)| self.ref_of(&NodeId(id.clone())))
                    .collect(),
            )),
        }
    }
}

/// The part after the dot of `ref.tail`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tail {
    /// `ref.N`: 1-based list item (depth-first, line order of the rendering).
    Item(u32),
    /// `ref.rRcC`: 1-based table row and cell position.
    Cell { row: u32, col: u32 },
    /// `slideRef.shapeRef`.
    Shape(Ref),
}

impl Tail {
    /// For `Cell` tails that were really a shape ref spelled like `R1C1` (a context
    /// decided by the node type): the ref text. `None` for other tails.
    pub fn as_ref_text(&self) -> Option<Ref> {
        match self {
            Tail::Cell { row, col } => Ref::parse(&format!("R{row}C{col}")).ok(),
            Tail::Shape(r) => Some(r.clone()),
            Tail::Item(_) => None,
        }
    }
}

fn plain_number(s: &str, max: u32) -> Option<u32> {
    (!s.is_empty() && !s.starts_with('0') && s.len() <= 5 && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
        .filter(|n| *n <= max)
}

impl FromStr for Tail {
    type Err = RefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() <= 3 && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
            return plain_number(s, 999)
                .map(Tail::Item)
                .ok_or(RefParseError("list item numbers start at 1"));
        }
        let b = s.as_bytes();
        if b.len() >= 4 && b[0].eq_ignore_ascii_case(&b'r') {
            if let Some(c) = s.bytes().position(|x| x.eq_ignore_ascii_case(&b'c')) {
                if let (Some(row), Some(col)) = (
                    plain_number(&s[1..c], 100_000),
                    plain_number(&s[c + 1..], 1_000),
                ) {
                    return Ok(Tail::Cell { row, col });
                }
            }
        }
        Ref::parse(s).map(Tail::Shape)
    }
}

impl fmt::Display for Tail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tail::Item(n) => write!(f, "{n}"),
            Tail::Cell { row, col } => write!(f, "r{row}c{col}"),
            Tail::Shape(r) => write!(f, "{r}"),
        }
    }
}

/// Any address a `read` can take (spec 5.3 sub-address table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// `7C02`
    Block(Ref),
    /// `7C02.3`, `3B77.r2c2`, `W2SK.A1B2`
    Sub { base: Ref, tail: Tail },
    /// `R91W!B7`: a cell of the sheet with that ref.
    SheetCell { sheet: Ref, at: CellRef },
}

impl FromStr for Address {
    type Err = RefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((sheet, cell)) = s.split_once('!') {
            let at = cell
                .parse::<CellRef>()
                .map_err(|_| RefParseError("bad cell after '!'"))?;
            return Ok(Address::SheetCell {
                sheet: Ref::parse(sheet)?,
                at,
            });
        }
        match s.split_once('.') {
            Some((base, tail)) => Ok(Address::Sub {
                base: Ref::parse(base)?,
                tail: tail.parse()?,
            }),
            None => Ok(Address::Block(Ref::parse(s)?)),
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Address::Block(r) => write!(f, "{r}"),
            Address::Sub { base, tail } => write!(f, "{base}.{tail}"),
            Address::SheetCell { sheet, at } => write!(f, "{sheet}!{at}"),
        }
    }
}

impl Address {
    pub fn base(&self) -> &Ref {
        match self {
            Address::Block(r) => r,
            Address::Sub { base, .. } => base,
            Address::SheetCell { sheet, .. } => sheet,
        }
    }
}

/// A slide selector: `s2` (1-based position at the revision) or a slide ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlideSel {
    Position(u32),
    Ref(Ref),
}

impl FromStr for SlideSel {
    type Err = RefParseError;

    /// Lowercase `s` plus 1 to 4 digits without a leading zero is a position label;
    /// anything else must be a ref. A ref typed in lowercase that happens to be `s` and
    /// digits is read as a position; the reader falls back to the ref when the position
    /// is out of range.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(d) = s.strip_prefix('s') {
            if let Some(n) = plain_number(d, 9_999).filter(|_| d.len() <= 4) {
                return Ok(SlideSel::Position(n));
            }
        }
        Ref::parse(s).map(SlideSel::Ref)
    }
}

impl fmt::Display for SlideSel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlideSel::Position(n) => write!(f, "s{n}"),
            SlideSel::Ref(r) => write!(f, "{r}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<NodeId> {
        list.iter().map(|s| NodeId((*s).into())).collect()
    }

    #[test]
    fn minimal_unique_suffix_with_floor_of_four() {
        let all = ids(&[
            "01J9ZQ0000000000000000AB12",
            "01J9ZQ0000000000000000CD12",
            "01J9ZQ0000000000000000EF34",
        ]);
        let idx = RefIndex::build(&all);
        // AB12 and CD12 share "12" only; EF34 needs no more than the floor.
        assert_eq!(idx.ref_of(&all[0]).unwrap().as_str(), "AB12");
        assert_eq!(idx.ref_of(&all[1]).unwrap().as_str(), "CD12");
        assert_eq!(idx.ref_of(&all[2]).unwrap().as_str(), "EF34");
        let tight = ids(&["01J9ZQ00000000000000XAB123", "01J9ZQ00000000000000YAB123"]);
        let idx = RefIndex::build(&tight);
        assert_eq!(idx.ref_of(&tight[0]).unwrap().as_str(), "XAB123");
        assert_eq!(idx.ref_of(&tight[1]).unwrap().as_str(), "YAB123");
    }

    #[test]
    fn resolve_unknown_ambiguous_exact() {
        let all = ids(&[
            "01J9ZQ00000000000000XAB123",
            "01J9ZQ00000000000000YAB123",
            "01J9ZQ0000000000000000ZZZZ",
        ]);
        let idx = RefIndex::build(&all);
        assert_eq!(
            idx.resolve(&Ref::parse("zzzz").unwrap()),
            Ok(all[2].clone())
        );
        assert_eq!(
            idx.resolve(&Ref::parse("QQQQ").unwrap()),
            Err(ResolveError::Unknown)
        );
        match idx.resolve(&Ref::parse("AB123").unwrap()) {
            Err(ResolveError::Ambiguous(c)) => {
                let c: Vec<_> = c.iter().map(Ref::as_str).collect();
                assert_eq!(c, ["XAB123", "YAB123"]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            idx.resolve(&Ref::parse("XAB123").unwrap()),
            Ok(all[0].clone())
        );
        assert_eq!(
            idx.resolve(&Ref::parse("01J9ZQ00000000000000XAB123").unwrap()),
            Ok(all[0].clone())
        );
    }

    #[test]
    fn nodes_created_after_a_revision_never_match() {
        let before = ids(&["01J9ZQ0000000000000000AB12"]);
        let mut after = before.clone();
        after.push(NodeId("01J9ZQ0000000000000000XB12".into()));
        let old = RefIndex::build(&before);
        let new = RefIndex::build(&after);
        let r = old.ref_of(&before[0]).unwrap();
        assert_eq!(r.as_str(), "AB12");
        assert_eq!(
            old.resolve(&Ref::parse("B12X").unwrap()),
            Err(ResolveError::Unknown)
        );
        // The later node did not exist at the old revision, so it cannot be matched there.
        assert_eq!(
            old.resolve(&Ref::parse("XB12").unwrap()),
            Err(ResolveError::Unknown)
        );
        assert_eq!(new.resolve(&r), Ok(before[0].clone()));
    }

    #[test]
    fn tombstones_count_for_uniqueness() {
        use engine_core::materializer::MaterializedNode;
        let mut state = DocumentState::default();
        for (id, deleted) in [
            ("01J9ZQ00000000000000XAB123", false),
            ("01J9ZQ00000000000000YAB123", true),
        ] {
            state.nodes.insert(
                NodeId(id.into()),
                MaterializedNode {
                    id: NodeId(id.into()),
                    parent_id: None,
                    node_type: "item".into(),
                    pos: "a".into(),
                    current_fields: serde_json::json!({}),
                    var_name: None,
                    deleted,
                },
            );
        }
        let idx = RefIndex::from_state(&state);
        assert_eq!(
            idx.ref_of(&NodeId("01J9ZQ00000000000000XAB123".into()))
                .unwrap()
                .as_str(),
            "XAB123"
        );
    }

    #[test]
    fn addresses() {
        let cases = [
            ("7C02", "7C02"),
            ("7c02.3", "7C02.3"),
            ("3B77.R2C2", "3B77.r2c2"),
            ("w2sk.a1b2", "W2SK.A1B2"),
            ("r91w!b7", "R91W!B7"),
            ("W2SK.1234", "W2SK.1234"),
        ];
        for (input, expected) in cases {
            let a: Address = input.parse().unwrap();
            assert_eq!(a.to_string(), expected, "{input}");
            assert_eq!(a.to_string().parse::<Address>().unwrap(), a);
        }
        assert_eq!(
            "7C02.3".parse::<Address>().unwrap(),
            Address::Sub {
                base: Ref::parse("7C02").unwrap(),
                tail: Tail::Item(3)
            }
        );
        assert!(matches!(
            "W2SK.1234".parse::<Address>().unwrap(),
            Address::Sub {
                tail: Tail::Shape(_),
                ..
            }
        ));
        // A shape ref spelled like a cell stays recoverable.
        let t: Tail = "R1C1".parse().unwrap();
        assert_eq!(t.as_ref_text().unwrap().as_str(), "R1C1");
        // `r0c1` is not a cell (rows start at 1) so it reads as a shape ref.
        assert!(matches!(
            "7C02.r0c1".parse::<Address>().unwrap(),
            Address::Sub {
                tail: Tail::Shape(_),
                ..
            }
        ));
        for bad in [
            "", "abc", "7C02.", "7C02.0", "!B7", "R91W!", "R91W!B0", "7C02.3.4", "7C0 2",
        ] {
            assert!(bad.parse::<Address>().is_err(), "{bad}");
        }
    }

    #[test]
    fn slide_selectors() {
        assert_eq!("s2".parse::<SlideSel>().unwrap(), SlideSel::Position(2));
        assert_eq!(
            "W2SK".parse::<SlideSel>().unwrap(),
            SlideSel::Ref(Ref::parse("W2SK").unwrap())
        );
        assert_eq!(
            "S123".parse::<SlideSel>().unwrap(),
            SlideSel::Ref(Ref::parse("S123").unwrap())
        );
        assert!("s0".parse::<SlideSel>().is_err());
        assert!("s".parse::<SlideSel>().is_err());
    }
}
