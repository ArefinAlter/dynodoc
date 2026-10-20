//! The operation grammar of protocol spec 8, parsed strictly from JSON values.
//!
//! An agent sends `propose.ops` as a flat list of loosely typed objects (spec 7.1: "schemas
//! are intentionally loose, validation is strict"). This module is the strict half: every op
//! becomes a typed [`Op`] or the call fails with the precise field in the error. Nothing
//! here looks at a document; every size and count limit of spec 11 is enforced before the
//! compiler does any work.
//!
//! Rules that hold for every op:
//!
//! * an unknown field, or a field the op does not take, is `bad_request` naming the field
//!   (`unknown_field` when the name is not one the grammar knows at all, because error
//!   payloads never carry free text);
//! * a wrong JSON type is `bad_request` naming the field;
//! * the op names of the 7.7 schema that this version does not compile (`fill`,
//!   `add_slide`, `delete_slide`, `macro`) and any other name are `unsupported_op`;
//! * agent text (`text`, `with`, `find`, cell values, `why`, new block text) is normalised
//!   (CRLF, CR, NEL, LS and PS become LF) and then refused when it holds a control character
//!   or one of the hidden or bidirectional characters of AI-AGENT-SECURITY P3: an agent may
//!   not write text a reviewer cannot see. The read path drops these characters; the write
//!   path rejects them, so a proposal never contains them.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::a1::{column_index, A1Range, CellRef, MAX_COLS, MAX_RANGE_CELLS, MAX_ROWS};
use crate::budget::MAX_REFS;
use crate::error::{AgentError, CompileError};
use crate::escape::escape;
use crate::hash::ShortHash;
use crate::refs::{Address, Ref, SlideSel, Tail};

/// `ops` per `propose` (spec 11).
pub const MAX_OPS: usize = 200;
/// Unicode scalars of one `text`, `with` or `find` (spec 11, the engine's `MAX_SCALARS`).
pub const MAX_TEXT_SCALARS: usize = 100_000;
/// Request body (spec 11: 1 MiB).
pub const MAX_BODY_BYTES: usize = 1 << 20;
/// Blocks of one `insert` (spec 8.2).
pub const MAX_INSERT_BLOCKS: usize = 50;
/// Characters of `id` (spec 8.1).
pub const MAX_ID_CHARS: usize = 40;
/// Characters of `why` (spec 8.1).
pub const MAX_WHY_CHARS: usize = 280;
/// Characters of the proposal `name` (spec 7.7).
pub const MAX_NAME_CHARS: usize = 200;
/// Characters of `find`. The spec sets none; `find` the tool allows 200 (`MAX_QUERY_CHARS`),
/// a sentence to replace needs more.
pub const MAX_FIND_CHARS: usize = 2_000;
/// Characters of a sheet name or sheet ref in `sheet`.
const MAX_SHEET_CHARS: usize = 100;

/// Every field name the grammar knows (the schema of spec 7.7 plus `fmt` of spec 8.2).
const FIELDS: [&str; 28] = [
    "op", "id", "base", "why", "ref", "text", "fmt", "find", "with", "nth", "all", "expect",
    "args", "after", "before", "into", "blocks", "t", "sheet", "at", "range", "values", "from",
    "to", "dir", "slide", "shape", "name",
];

fn field_name(key: &str) -> &'static str {
    FIELDS
        .iter()
        .copied()
        .find(|f| *f == key)
        .unwrap_or("unknown_field")
}

fn bad(field: &'static str) -> AgentError {
    AgentError::BadRequest { field }
}

type R<T> = Result<T, AgentError>;

/// A ref, or the block created earlier in the same call (`$1` is the first block any
/// `insert` of the call creates, `$2` the second, in creation order; spec 8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    Ref(Ref),
    /// 1-based creation number.
    New(usize),
}

/// What `ref` names: a block, or a text container inside it (`ref.N`, `ref.rRcC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub block: Anchor,
    /// Only `ref.N` and `ref.rRcC` are accepted; `ref.shape` is a slide address.
    pub tail: Option<Tail>,
}

/// `after` or `before` of `insert` and `move`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    After(Anchor),
    Before(Anchor),
}

/// A block type of `insert` (`p`, `h1`..`h6`, `li`, `code`) or of `set_type` (`p`, `h1`..`h6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Paragraph,
    Heading(u8),
    /// `li`: a one-item bullet list.
    ListItem,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBlock {
    pub t: BlockType,
    pub text: String,
}

/// Which matches of a `replace` to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// Neither `nth` nor `all`: only valid when exactly one match exists or `expect` says
    /// how many there are.
    Unspecified,
    Nth(u32),
    All,
}

/// Where `replace_all` looks (`args.scope`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Every block of the file.
    Everywhere,
    /// The section that starts at a heading.
    Under(Anchor),
    Refs(Vec<Anchor>),
    /// Type labels of the outline: `P`, `H1`..`H6`, `H`, `UL`, `OL`, `TBL`, `CODE`, `QUOTE`,
    /// `MATH`, `IMG`, `HR`, `LI`, `X` (uppercase, as parsed).
    Types(Vec<String>),
}

/// `at` of `set_cells`: an A1 cell, or a row ref and a column (`7C02:E`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellAt {
    A1(CellRef),
    Row { row: Ref, col: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpKind {
    /// `fmt` is always plain: `fmt=md` is refused with `bad_request field=fmt`.
    SetText {
        at: Target,
        text: String,
    },
    Replace {
        at: Target,
        find: String,
        with: String,
        which: Which,
        expect: Option<u64>,
    },
    ReplaceAll {
        find: String,
        with: String,
        expect: Option<u64>,
        scope: Scope,
        word: bool,
        /// `true`: match case. Differs from `find`, which ignores case unless asked.
        case: bool,
    },
    Insert {
        place: Place,
        blocks: Vec<NewBlock>,
    },
    Delete {
        at: Target,
    },
    Move {
        at: Target,
        place: Place,
    },
    SetType {
        at: Target,
        to: BlockType,
    },
    SetCells {
        sheet: String,
        at: CellAt,
        values: Vec<Vec<String>>,
    },
    Clear {
        sheet: Option<String>,
        range: A1Range,
    },
    SetShape {
        slide: SlideSel,
        shape: Ref,
        text: String,
    },
    SetNotes {
        slide: SlideSel,
        text: String,
    },
}

impl OpKind {
    /// The `op` name as the agent wrote it.
    pub fn name(&self) -> &'static str {
        match self {
            OpKind::SetText { .. } => "set_text",
            OpKind::Replace { .. } => "replace",
            OpKind::ReplaceAll { .. } => "replace_all",
            OpKind::Insert { .. } => "insert",
            OpKind::Delete { .. } => "delete",
            OpKind::Move { .. } => "move",
            OpKind::SetType { .. } => "set_type",
            OpKind::SetCells { .. } => "set_cells",
            OpKind::Clear { .. } => "clear",
            OpKind::SetShape { .. } => "set_shape",
            OpKind::SetNotes { .. } => "set_notes",
        }
    }
}

/// One parsed op with its common fields (spec 8.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    /// 0-based position in the `ops` array.
    pub index: usize,
    /// The caller's label: 1 to 40 characters of `A-Za-z0-9_.:-`, unique in the call. The
    /// spec allows any string of up to 40; the restriction keeps it safe to echo in an
    /// error line.
    pub id: Option<String>,
    /// The hash the target had when the agent read it (6 to 64 hex; the spec says 12, and a
    /// 6-character hash from `read` is what a model has at hand).
    pub base: Option<ShortHash>,
    /// Reason shown to the reviewer: untrusted text of at most 280 characters.
    pub why: Option<String>,
    pub kind: OpKind,
}

/// `return` of `propose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReturnKind {
    #[default]
    Summary,
    Diff,
}

/// The arguments of `propose` (spec 7.7), parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposeArgs {
    pub file: Uuid,
    pub rev: u64,
    pub request_id: Uuid,
    pub proposal: Option<Uuid>,
    pub name: Option<String>,
    pub ops: Vec<Op>,
    pub dry_run: bool,
    pub strict: bool,
    pub ret: ReturnKind,
}

// ------------------------------------------------------------------ field access

/// Text from the agent: normalised line breaks, no control or hidden characters, at most
/// [`MAX_TEXT_SCALARS`] scalars.
fn clean(field: &'static str, s: &str) -> R<String> {
    let n = s.chars().count();
    if n > MAX_TEXT_SCALARS {
        return Err(AgentError::LimitExceeded {
            field,
            limit: MAX_TEXT_SCALARS as u64,
            value: n as u64,
        });
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{85}' | '\u{2028}' | '\u{2029}' => out.push('\n'),
            c => out.push(c),
        }
    }
    let (_, dropped) = escape(&out);
    if dropped.ctrl > 0 || dropped.hidden > 0 {
        return Err(bad(field));
    }
    Ok(out)
}

struct Fields<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    /// Refuse every key that is not in `allowed`.
    fn only(&self, allowed: &[&'static str]) -> R<()> {
        match self.map.keys().find(|k| !allowed.contains(&k.as_str())) {
            Some(k) => Err(bad(field_name(k))),
            None => Ok(()),
        }
    }

    fn get(&self, name: &'static str) -> Option<&'a Value> {
        self.map.get(name)
    }

    fn str_opt(&self, name: &'static str) -> R<Option<&'a str>> {
        match self.get(name) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(bad(name)),
        }
    }

    fn str_req(&self, name: &'static str) -> R<&'a str> {
        self.str_opt(name)?.ok_or(bad(name))
    }

    fn text_req(&self, name: &'static str) -> R<String> {
        clean(name, self.str_req(name)?)
    }

    fn bool_opt(&self, name: &'static str) -> R<Option<bool>> {
        match self.get(name) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(bad(name)),
        }
    }

    fn u64_opt(&self, name: &'static str) -> R<Option<u64>> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v.as_u64().map(Some).ok_or(bad(name)),
        }
    }
}

fn parse_id(s: &str) -> R<String> {
    let n = s.chars().count();
    if n > MAX_ID_CHARS {
        return Err(AgentError::LimitExceeded {
            field: "id",
            limit: MAX_ID_CHARS as u64,
            value: n as u64,
        });
    }
    let ok = n > 0
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'));
    if ok {
        Ok(s.to_string())
    } else {
        Err(bad("id"))
    }
}

/// `$3` or a ref; `field` names the request field in errors.
fn parse_anchor(field: &'static str, s: &str) -> R<Anchor> {
    if let Some(n) = s.strip_prefix('$') {
        let ok = !n.is_empty()
            && n.len() <= 5
            && !n.starts_with('0')
            && n.bytes().all(|b| b.is_ascii_digit());
        return match n.parse::<usize>() {
            Ok(n) if ok => Ok(Anchor::New(n)),
            _ => Err(bad(field)),
        };
    }
    Ref::parse(s).map(Anchor::Ref).map_err(|_| bad(field))
}

fn parse_target(field: &'static str, s: &str) -> R<Target> {
    if s.starts_with('$') {
        return Ok(Target {
            block: parse_anchor(field, s)?,
            tail: None,
        });
    }
    match s.parse::<Address>().map_err(|_| bad(field))? {
        Address::Block(r) => Ok(Target {
            block: Anchor::Ref(r),
            tail: None,
        }),
        // `ref.shape` names a slide shape; documents have list items and table cells.
        Address::Sub {
            base,
            tail: tail @ (Tail::Item(_) | Tail::Cell { .. }),
        } => Ok(Target {
            block: Anchor::Ref(base),
            tail: Some(tail),
        }),
        Address::Sub { .. } | Address::SheetCell { .. } => Err(bad(field)),
    }
}

/// A block without a sub-address (`delete`, `move`, `set_type`).
fn parse_block(field: &'static str, s: &str) -> R<Target> {
    let t = parse_target(field, s)?;
    if t.tail.is_some() {
        return Err(bad(field));
    }
    Ok(t)
}

fn parse_place(f: &Fields) -> R<Place> {
    match (f.str_opt("after")?, f.str_opt("before")?) {
        (Some(a), None) => Ok(Place::After(parse_anchor("after", a)?)),
        (None, Some(b)) => Ok(Place::Before(parse_anchor("before", b)?)),
        (Some(_), Some(_)) => Err(bad("before")),
        (None, None) => Err(bad("after")),
    }
}

fn parse_block_type(field: &'static str, s: &str) -> R<BlockType> {
    match s {
        "p" => Ok(BlockType::Paragraph),
        "li" => Ok(BlockType::ListItem),
        "code" => Ok(BlockType::Code),
        h if h.len() == 2 && h.starts_with('h') => match h.as_bytes()[1] {
            n @ b'1'..=b'6' => Ok(BlockType::Heading(n - b'0')),
            _ => Err(bad(field)),
        },
        _ => Err(bad(field)),
    }
}

fn parse_blocks(f: &Fields) -> R<Vec<NewBlock>> {
    let Some(Value::Array(items)) = f.get("blocks") else {
        return Err(bad("blocks"));
    };
    if items.is_empty() {
        return Err(bad("blocks"));
    }
    if items.len() > MAX_INSERT_BLOCKS {
        return Err(AgentError::LimitExceeded {
            field: "blocks",
            limit: MAX_INSERT_BLOCKS as u64,
            value: items.len() as u64,
        });
    }
    let mut total = 0usize;
    items
        .iter()
        .map(|item| {
            let map = item.as_object().ok_or(bad("blocks"))?;
            let b = Fields { map };
            b.only(&["t", "text"])?;
            let text = b.text_req("text")?;
            total += text.chars().count();
            if total > MAX_TEXT_SCALARS {
                return Err(AgentError::LimitExceeded {
                    field: "text",
                    limit: MAX_TEXT_SCALARS as u64,
                    value: total as u64,
                });
            }
            Ok(NewBlock {
                t: parse_block_type("t", b.str_req("t")?)?,
                text,
            })
        })
        .collect()
}

/// A formula (`=...`) is not evaluated, only checked for balanced brackets outside
/// quotes (spec 8.4).
fn brackets_balance(formula: &str) -> bool {
    let mut stack: Vec<char> = Vec::new();
    let mut quote: Option<char> = None;
    for c in formula.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => stack.push(c),
            ')' if stack.pop() != Some('(') => return false,
            ']' if stack.pop() != Some('[') => return false,
            '}' if stack.pop() != Some('{') => return false,
            _ => {}
        }
    }
    quote.is_none() && stack.is_empty()
}

fn parse_values(f: &Fields) -> R<Vec<Vec<String>>> {
    let Some(Value::Array(rows)) = f.get("values") else {
        return Err(bad("values"));
    };
    if rows.is_empty() {
        return Err(bad("values"));
    }
    if rows.len() > MAX_ROWS as usize {
        return Err(AgentError::LimitExceeded {
            field: "values",
            limit: u64::from(MAX_ROWS),
            value: rows.len() as u64,
        });
    }
    let (mut cells, mut scalars) = (0usize, 0usize);
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Value::Array(items) = row else {
            return Err(bad("values"));
        };
        if items.len() > usize::from(MAX_COLS) {
            return Err(AgentError::LimitExceeded {
                field: "values",
                limit: u64::from(MAX_COLS),
                value: items.len() as u64,
            });
        }
        cells += items.len();
        if cells as u64 > MAX_RANGE_CELLS {
            return Err(AgentError::LimitExceeded {
                field: "values",
                limit: MAX_RANGE_CELLS,
                value: cells as u64,
            });
        }
        let mut r = Vec::with_capacity(items.len());
        for item in items {
            let Value::String(s) = item else {
                return Err(bad("values"));
            };
            let cell = clean("values", s)?;
            scalars += cell.chars().count();
            if scalars > MAX_TEXT_SCALARS {
                return Err(AgentError::LimitExceeded {
                    field: "values",
                    limit: MAX_TEXT_SCALARS as u64,
                    value: scalars as u64,
                });
            }
            if cell.starts_with('=') && !brackets_balance(&cell) {
                return Err(bad("values"));
            }
            r.push(cell);
        }
        out.push(r);
    }
    if cells == 0 {
        return Err(bad("values"));
    }
    Ok(out)
}

fn parse_cell_at(s: &str) -> R<CellAt> {
    if let Ok(c) = s.parse::<CellRef>() {
        return Ok(CellAt::A1(c));
    }
    match s.split_once(':') {
        Some((row, col)) => Ok(CellAt::Row {
            row: Ref::parse(row).map_err(|_| bad("at"))?,
            col: column_index(col).map_err(|_| bad("at"))?,
        }),
        None => Err(bad("at")),
    }
}

fn parse_sheet(s: &str) -> R<String> {
    let n = s.chars().count();
    if n == 0 || n > MAX_SHEET_CHARS || s.chars().any(char::is_control) {
        return Err(bad("sheet"));
    }
    Ok(s.to_string())
}

const TYPE_LABELS: [&str; 17] = [
    "P", "H", "H1", "H2", "H3", "H4", "H5", "H6", "UL", "OL", "TBL", "CODE", "QUOTE", "MATH",
    "IMG", "HR", "LI",
];

/// `args` of `replace_all`: `{scope, word, case}`.
fn parse_replace_all_args(args: Option<&Value>) -> R<(Scope, bool, bool)> {
    let Some(args) = args else {
        return Ok((Scope::Everywhere, false, true));
    };
    let map = args.as_object().ok_or(bad("args"))?;
    let a = Fields { map };
    a.only(&["scope", "word", "case"])
        .map_err(|_| bad("args"))?;
    let flag = |name: &'static str, default: bool| -> R<bool> {
        match map.get(name) {
            None => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(bad("args")),
        }
    };
    let word = flag("word", false)?;
    let case = flag("case", true)?;
    let scope = match map.get("scope") {
        None => Scope::Everywhere,
        Some(v) => {
            let s = v.as_object().ok_or(bad("args"))?;
            if s.len() != 1 {
                return Err(bad("args"));
            }
            let (key, val) = s.iter().next().expect("one entry");
            match (key.as_str(), val) {
                ("under", Value::String(r)) => Scope::Under(parse_anchor("args", r)?),
                ("refs", Value::Array(items)) => {
                    if items.is_empty() {
                        return Err(bad("args"));
                    }
                    if items.len() > MAX_REFS {
                        return Err(AgentError::LimitExceeded {
                            field: "refs",
                            limit: MAX_REFS as u64,
                            value: items.len() as u64,
                        });
                    }
                    Scope::Refs(
                        items
                            .iter()
                            .map(|i| parse_anchor("args", i.as_str().ok_or(bad("args"))?))
                            .collect::<R<Vec<_>>>()?,
                    )
                }
                ("types", Value::Array(items)) => {
                    if items.is_empty() || items.len() > TYPE_LABELS.len() {
                        return Err(bad("args"));
                    }
                    let mut types = BTreeSet::new();
                    for i in items {
                        let t = i.as_str().ok_or(bad("args"))?.to_ascii_uppercase();
                        if !TYPE_LABELS.contains(&t.as_str()) {
                            return Err(bad("args"));
                        }
                        types.insert(t);
                    }
                    Scope::Types(types.into_iter().collect())
                }
                _ => return Err(bad("args")),
            }
        }
    };
    Ok((scope, word, case))
}

fn parse_find(f: &Fields) -> R<String> {
    let find = f.str_req("find")?;
    let n = find.chars().count();
    if n == 0 {
        return Err(bad("find"));
    }
    if n > MAX_FIND_CHARS {
        return Err(AgentError::LimitExceeded {
            field: "find",
            limit: MAX_FIND_CHARS as u64,
            value: n as u64,
        });
    }
    // U+FFFC stands for an inline object while matching; it can never be searched for.
    if find.contains('\u{FFFC}') {
        return Err(bad("find"));
    }
    clean("find", find)
}

fn parse_with(f: &Fields) -> R<String> {
    let with = f.text_req("with")?;
    if with.contains('\u{FFFC}') {
        return Err(bad("with"));
    }
    Ok(with)
}

// ------------------------------------------------------------------ the ops

fn parse_kind(name: &str, f: &Fields) -> R<OpKind> {
    // Common fields first, then the ones of the op.
    macro_rules! only {
        ($($field:literal),*) => {
            f.only(&["op", "id", "base", "why", $($field),*])?
        };
    }
    match name {
        "set_text" => {
            only!("ref", "text", "fmt");
            match f.str_opt("fmt")? {
                None | Some("plain") => {}
                // Inline markdown needs the inverse of the `read fmt=md` renderer.
                Some(_) => return Err(bad("fmt")),
            }
            Ok(OpKind::SetText {
                at: parse_target("ref", f.str_req("ref")?)?,
                text: f.text_req("text")?,
            })
        }
        "replace" => {
            only!("ref", "find", "with", "nth", "all", "expect");
            let all = f.bool_opt("all")?.unwrap_or(false);
            let nth = match f.u64_opt("nth")? {
                None => None,
                Some(n) if (1..=1_000_000).contains(&n) => Some(n as u32),
                Some(_) => return Err(bad("nth")),
            };
            let which = match (nth, all) {
                (Some(_), true) => return Err(bad("all")),
                (Some(n), false) => Which::Nth(n),
                (None, true) => Which::All,
                (None, false) => Which::Unspecified,
            };
            Ok(OpKind::Replace {
                at: parse_target("ref", f.str_req("ref")?)?,
                find: parse_find(f)?,
                with: parse_with(f)?,
                which,
                expect: f.u64_opt("expect")?,
            })
        }
        "replace_all" => {
            only!("find", "with", "expect", "args");
            let (scope, word, case) = parse_replace_all_args(f.get("args"))?;
            Ok(OpKind::ReplaceAll {
                find: parse_find(f)?,
                with: parse_with(f)?,
                expect: f.u64_opt("expect")?,
                scope,
                word,
                case,
            })
        }
        "insert" => {
            only!("after", "before", "blocks", "into");
            // `into` (a slide ref) needs the layout geometry of presentation.ts in Rust.
            if f.get("into").is_some() {
                return Err(AgentError::UnsupportedOp);
            }
            Ok(OpKind::Insert {
                place: parse_place(f)?,
                blocks: parse_blocks(f)?,
            })
        }
        "delete" => {
            only!("ref", "sheet");
            // Deleting a sheet deletes its rows: not compiled in this version.
            if f.get("sheet").is_some() {
                return Err(AgentError::UnsupportedOp);
            }
            Ok(OpKind::Delete {
                at: parse_block("ref", f.str_req("ref")?)?,
            })
        }
        "move" => {
            only!("ref", "after", "before");
            Ok(OpKind::Move {
                at: parse_block("ref", f.str_req("ref")?)?,
                place: parse_place(f)?,
            })
        }
        "set_type" => {
            only!("ref", "t");
            let to = parse_block_type("t", f.str_req("t")?)?;
            if !matches!(to, BlockType::Paragraph | BlockType::Heading(_)) {
                return Err(bad("t"));
            }
            Ok(OpKind::SetType {
                at: parse_block("ref", f.str_req("ref")?)?,
                to,
            })
        }
        "set_cells" => {
            only!("sheet", "at", "values");
            Ok(OpKind::SetCells {
                sheet: parse_sheet(f.str_req("sheet")?)?,
                at: parse_cell_at(f.str_req("at")?)?,
                values: parse_values(f)?,
            })
        }
        "clear" => {
            only!("sheet", "range");
            let range: A1Range = f.str_req("range")?.parse().map_err(|_| bad("range"))?;
            let sheet = f.str_opt("sheet")?.map(parse_sheet).transpose()?;
            if sheet.is_none() && range.sheet.is_none() {
                return Err(bad("sheet"));
            }
            if range.cells() > MAX_RANGE_CELLS {
                return Err(AgentError::LimitExceeded {
                    field: "range",
                    limit: MAX_RANGE_CELLS,
                    value: range.cells(),
                });
            }
            Ok(OpKind::Clear { sheet, range })
        }
        "set_shape" => {
            only!("slide", "shape", "text");
            Ok(OpKind::SetShape {
                slide: f.str_req("slide")?.parse().map_err(|_| bad("slide"))?,
                shape: Ref::parse(f.str_req("shape")?).map_err(|_| bad("shape"))?,
                text: f.text_req("text")?,
            })
        }
        "set_notes" => {
            only!("slide", "text");
            Ok(OpKind::SetNotes {
                slide: f.str_req("slide")?.parse().map_err(|_| bad("slide"))?,
                text: f.text_req("text")?,
            })
        }
        // Named in the 7.7 schema, not compiled by this version.
        _ => Err(AgentError::UnsupportedOp),
    }
}

fn parse_op(index: usize, value: &Value) -> Result<Op, CompileError> {
    let at = |id: Option<&str>, e: AgentError| CompileError::at(index, id, e);
    let map = value.as_object().ok_or_else(|| at(None, bad("ops")))?;
    let f = Fields { map };
    let id = f
        .str_opt("id")
        .and_then(|s| s.map(parse_id).transpose())
        .map_err(|e| at(None, e))?;
    let id_ref = id.as_deref();
    let inner = || -> R<Op> {
        let name = f.str_req("op")?;
        let kind = parse_kind(name, &f)?;
        let base = f
            .str_opt("base")?
            .map(|s| ShortHash::parse(s).map_err(|_| bad("base")))
            .transpose()?;
        let why = match f.str_opt("why")? {
            None => None,
            Some(w) => {
                let n = w.chars().count();
                if n > MAX_WHY_CHARS {
                    return Err(AgentError::LimitExceeded {
                        field: "why",
                        limit: MAX_WHY_CHARS as u64,
                        value: n as u64,
                    });
                }
                Some(clean("why", w)?)
            }
        };
        Ok(Op {
            index,
            id: id.clone(),
            base,
            why,
            kind,
        })
    };
    inner().map_err(|e| at(id_ref, e))
}

/// Parse the `ops` array of a `propose` call. `value` is the JSON the agent sent.
pub fn parse_ops(value: &Value) -> Result<Vec<Op>, CompileError> {
    let items = value
        .as_array()
        .ok_or_else(|| CompileError::from(bad("ops")))?;
    if items.is_empty() {
        return Err(bad("ops").into());
    }
    if items.len() > MAX_OPS {
        return Err(AgentError::LimitExceeded {
            field: "ops",
            limit: MAX_OPS as u64,
            value: items.len() as u64,
        }
        .into());
    }
    let body = serde_json::to_vec(value).map_or(usize::MAX, |b| b.len());
    if body > MAX_BODY_BYTES {
        return Err(AgentError::LimitExceeded {
            field: "body",
            limit: MAX_BODY_BYTES as u64,
            value: body as u64,
        }
        .into());
    }
    let mut ops = Vec::with_capacity(items.len());
    let mut seen = BTreeSet::new();
    for (i, item) in items.iter().enumerate() {
        let op = parse_op(i, item)?;
        if let Some(id) = &op.id {
            if !seen.insert(id.clone()) {
                return Err(CompileError::at(i, Some(id), bad("id")));
            }
        }
        ops.push(op);
    }
    Ok(ops)
}

fn uuid_field(f: &Fields, name: &'static str) -> R<Uuid> {
    Uuid::parse_str(f.str_req(name)?).map_err(|_| bad(name))
}

/// Parse the whole argument object of `propose` (spec 7.7).
pub fn parse_propose(value: &Value) -> Result<ProposeArgs, CompileError> {
    let map = value
        .as_object()
        .ok_or_else(|| CompileError::from(bad("body")))?;
    let f = Fields { map };
    f.only(&[
        "file",
        "rev",
        "request_id",
        "proposal",
        "name",
        "ops",
        "dry_run",
        "strict",
        "return",
    ])?;
    let body = serde_json::to_vec(value).map_or(usize::MAX, |b| b.len());
    if body > MAX_BODY_BYTES {
        return Err(AgentError::LimitExceeded {
            field: "body",
            limit: MAX_BODY_BYTES as u64,
            value: body as u64,
        }
        .into());
    }
    let file = uuid_field(&f, "file")?;
    let rev = f.u64_opt("rev")?.ok_or(bad("rev"))?;
    let request_id = uuid_field(&f, "request_id")?;
    let proposal = match f.str_opt("proposal")? {
        None => None,
        Some(s) => Some(Uuid::parse_str(s).map_err(|_| bad("proposal"))?),
    };
    let name = match f.str_opt("name")? {
        None => None,
        Some(n) => {
            let c = n.chars().count();
            if c > MAX_NAME_CHARS {
                return Err(AgentError::LimitExceeded {
                    field: "name",
                    limit: MAX_NAME_CHARS as u64,
                    value: c as u64,
                }
                .into());
            }
            Some(clean("name", n)?)
        }
    };
    let ret = match map.get("return") {
        None => ReturnKind::Summary,
        Some(Value::String(s)) if s == "summary" => ReturnKind::Summary,
        Some(Value::String(s)) if s == "diff" => ReturnKind::Diff,
        Some(_) => return Err(bad("return").into()),
    };
    let ops = parse_ops(f.get("ops").ok_or_else(|| CompileError::from(bad("ops")))?)?;
    Ok(ProposeArgs {
        file,
        rev,
        request_id,
        proposal,
        name,
        ops,
        dry_run: f.bool_opt("dry_run")?.unwrap_or(false),
        strict: f.bool_opt("strict")?.unwrap_or(false),
        ret,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn one(v: Value) -> Result<Op, CompileError> {
        parse_ops(&json!([v])).map(|mut o| o.remove(0))
    }

    fn field(r: Result<Op, CompileError>) -> &'static str {
        match r.unwrap_err().error {
            AgentError::BadRequest { field } => field,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_supported_op_parses() {
        let ops = parse_ops(&json!([
            {"op": "set_text", "ref": "7c02", "text": "New", "id": "a1", "base": "#b7a210", "why": "tone"},
            {"op": "set_text", "ref": "7C02.2", "text": "Item", "fmt": "plain"},
            {"op": "replace", "ref": "7C02", "find": "policy", "with": "procedure", "nth": 2},
            {"op": "replace", "ref": "7C02.r1c2", "find": "a", "with": "b", "all": true, "expect": 3},
            {"op": "replace_all", "find": "x", "with": "y", "expect": 4,
             "args": {"scope": {"types": ["p", "h2"]}, "word": true, "case": false}},
            {"op": "insert", "after": "7C02", "blocks": [{"t": "h2", "text": "A"}, {"t": "li", "text": "B"}]},
            {"op": "insert", "before": "$1", "blocks": [{"t": "code", "text": "x"}]},
            {"op": "delete", "ref": "7C02"},
            {"op": "move", "ref": "7C02", "before": "AAAA"},
            {"op": "set_type", "ref": "7C02", "t": "h3"},
            {"op": "set_cells", "sheet": "Budget", "at": "B2", "values": [["1", "=SUM(A1:A2)"], ["x"]]},
            {"op": "set_cells", "sheet": "R91W", "at": "7C02:E", "values": [["1"]]},
            {"op": "clear", "sheet": "Budget", "range": "A1:C3"},
            {"op": "clear", "range": "Budget!A1"},
            {"op": "set_shape", "slide": "s2", "shape": "W2SK", "text": "T"},
            {"op": "set_notes", "slide": "W2SK", "text": "N"},
        ]))
        .unwrap();
        assert_eq!(ops.len(), 16);
        assert_eq!(ops[0].id.as_deref(), Some("a1"));
        assert_eq!(ops[0].base.as_ref().unwrap().as_str(), "b7a210");
        assert_eq!(
            ops[2].kind,
            OpKind::Replace {
                at: Target {
                    block: Anchor::Ref(Ref::parse("7C02").unwrap()),
                    tail: None
                },
                find: "policy".into(),
                with: "procedure".into(),
                which: Which::Nth(2),
                expect: None
            }
        );
        assert!(matches!(
            &ops[4].kind,
            OpKind::ReplaceAll { scope: Scope::Types(t), word: true, case: false, expect: Some(4), .. }
                if t == &["H2".to_string(), "P".to_string()]
        ));
        assert!(matches!(
            &ops[6].kind,
            OpKind::Insert {
                place: Place::Before(Anchor::New(1)),
                ..
            }
        ));
        assert_eq!(
            ops.iter().map(|o| o.kind.name()).collect::<Vec<_>>()[10],
            "set_cells"
        );
    }

    #[test]
    fn unknown_and_misplaced_fields_are_named() {
        // Unknown to the grammar altogether.
        assert_eq!(
            field(one(json!({"op": "delete", "ref": "7C02", "force": true}))),
            "unknown_field"
        );
        // Known, but not a field of this op.
        assert_eq!(
            field(one(json!({"op": "delete", "ref": "7C02", "text": "x"}))),
            "text"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_text", "ref": "7C02", "text": "x", "nth": 1})
            )),
            "nth"
        );
        // Missing and mistyped.
        assert_eq!(field(one(json!({"op": "set_text", "text": "x"}))), "ref");
        assert_eq!(
            field(one(json!({"op": "set_text", "ref": 7, "text": "x"}))),
            "ref"
        );
        assert_eq!(
            field(one(json!({"op": "set_text", "ref": "7C02", "text": null}))),
            "text"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "", "with": "x"})
            )),
            "find"
        );
        assert_eq!(field(one(json!({"op": 5}))), "op");
        assert_eq!(field(one(json!({}))), "op");
        assert_eq!(field(one(json!("set_text"))), "ops");
        // The error names the op and its label.
        let e = one(json!({"op": "delete", "id": "gone", "ref": "!!"})).unwrap_err();
        assert_eq!((e.op, e.id.as_deref()), (Some(0), Some("gone")));
        assert!(e.to_dct().contains(" op=0 id=gone "));
    }

    #[test]
    fn unsupported_ops_and_options() {
        for name in ["fill", "add_slide", "delete_slide", "macro", "explode", ""] {
            let e = one(json!({"op": name})).unwrap_err();
            assert_eq!(e.error, AgentError::UnsupportedOp, "{name}");
        }
        for v in [
            json!({"op": "insert", "into": "W2SK", "blocks": [{"t": "p", "text": "x"}]}),
            json!({"op": "delete", "sheet": "Budget"}),
        ] {
            assert_eq!(one(v).unwrap_err().error, AgentError::UnsupportedOp);
        }
        // fmt=md would need an inline markdown parser.
        assert_eq!(
            field(one(
                json!({"op": "set_text", "ref": "7C02", "text": "x", "fmt": "md"})
            )),
            "fmt"
        );
    }

    #[test]
    fn selectors_and_types_are_validated() {
        assert_eq!(
            field(one(
                json!({"op": "insert", "blocks": [{"t": "p", "text": "x"}]})
            )),
            "after"
        );
        assert_eq!(
            field(one(
                json!({"op": "insert", "after": "7C02", "before": "7C03", "blocks": [{"t": "p", "text": "x"}]})
            )),
            "before"
        );
        assert_eq!(
            field(one(
                json!({"op": "insert", "after": "7C02", "blocks": [{"t": "h7", "text": "x"}]})
            )),
            "t"
        );
        assert_eq!(
            field(one(
                json!({"op": "insert", "after": "7C02", "blocks": [{"t": "p"}]})
            )),
            "text"
        );
        assert_eq!(
            field(one(json!({"op": "insert", "after": "7C02", "blocks": []}))),
            "blocks"
        );
        assert_eq!(
            field(one(json!({"op": "set_type", "ref": "7C02", "t": "li"}))),
            "t"
        );
        assert_eq!(
            field(one(json!({"op": "set_type", "ref": "7C02.2", "t": "p"}))),
            "ref"
        );
        assert_eq!(
            field(one(json!({"op": "move", "ref": "7C02", "after": "$0"}))),
            "after"
        );
        assert_eq!(
            field(one(json!({"op": "move", "ref": "7C02", "after": "$x"}))),
            "after"
        );
        assert_eq!(field(one(json!({"op": "delete", "ref": "AB"}))), "ref");
        assert_eq!(field(one(json!({"op": "delete", "ref": "R91W!B7"}))), "ref");
        assert_eq!(
            field(one(
                json!({"op": "set_text", "ref": "7C02.W2SK", "text": "x"})
            )),
            "ref"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "a", "with": "b", "nth": 0})
            )),
            "nth"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "a", "with": "b", "nth": 1, "all": true})
            )),
            "all"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "a", "with": "b", "expect": -1})
            )),
            "expect"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace_all", "find": "a", "with": "b", "args": {"scope": {"types": ["ZZ"]}}})
            )),
            "args"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace_all", "find": "a", "with": "b", "args": {"scope": {"under": "7C02", "refs": ["7C03"]}}})
            )),
            "args"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace_all", "find": "a", "with": "b", "args": {"flavour": 1}})
            )),
            "args"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_cells", "sheet": "S", "at": "B", "values": [["x"]]})
            )),
            "at"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": [[1]]})
            )),
            "values"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": [["=SUM(A1"]]})
            )),
            "values"
        );
        assert_eq!(
            field(one(json!({"op": "clear", "range": "A1:B2"}))),
            "sheet"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_shape", "slide": "s0", "shape": "W2SK", "text": "x"})
            )),
            "slide"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_notes", "slide": "s1", "text": "x", "shape": "W2SK"})
            )),
            "shape"
        );
        assert_eq!(
            field(one(
                json!({"op": "set_text", "ref": "7C02", "text": "x", "base": "xyz"})
            )),
            "base"
        );
    }

    #[test]
    fn limits_come_before_any_work() {
        let many = Value::Array(
            (0..201)
                .map(|_| json!({"op": "delete", "ref": "7C02"}))
                .collect(),
        );
        let e = parse_ops(&many).unwrap_err();
        assert_eq!(
            e.error,
            AgentError::LimitExceeded {
                field: "ops",
                limit: 200,
                value: 201
            }
        );
        assert_eq!(parse_ops(&json!([])).unwrap_err().error, bad("ops"));
        let long = "x".repeat(MAX_TEXT_SCALARS + 1);
        let e = one(json!({"op": "set_text", "ref": "7C02", "text": long})).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded { field: "text", .. }
        ));
        assert_eq!(e.error.http_status(), 413);
        let e = one(json!({"op": "set_text", "ref": "7C02", "text": "x", "why": "w".repeat(281)}))
            .unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "why",
                limit: 280,
                value: 281
            }
        ));
        let e = one(json!({"op": "set_text", "ref": "7C02", "text": "x", "id": "i".repeat(41)}))
            .unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded { field: "id", .. }
        ));
        let blocks: Vec<Value> = (0..51).map(|_| json!({"t": "p", "text": "x"})).collect();
        let e = one(json!({"op": "insert", "after": "7C02", "blocks": blocks})).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "blocks",
                limit: 50,
                value: 51
            }
        ));
        let rows: Vec<Value> = (0..3001).map(|_| json!(["x"])).collect();
        let e =
            one(json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": rows})).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "values",
                limit: 3000,
                ..
            }
        ));
        let rows: Vec<Value> = (0..21)
            .map(|_| Value::Array(vec![json!("x"); 100]))
            .collect();
        let e =
            one(json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": rows})).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "values",
                limit: 2000,
                ..
            }
        ));
        let wide = vec![json!("x"); 101];
        let e = one(json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": [wide]}))
            .unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "values",
                limit: 100,
                ..
            }
        ));
        let e = one(json!({"op": "clear", "sheet": "S", "range": "A1:CV30"})).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "range",
                limit: 2000,
                ..
            }
        ));
        // Over 1 MiB in total.
        let big = "y".repeat(MAX_TEXT_SCALARS);
        let ops: Vec<Value> = (0..11)
            .map(|_| json!({"op": "set_text", "ref": "7C02", "text": big}))
            .collect();
        let e = parse_ops(&Value::Array(ops)).unwrap_err();
        assert!(matches!(
            e.error,
            AgentError::LimitExceeded { field: "body", .. }
        ));
    }

    #[test]
    fn labels_are_unique_and_safe() {
        let e = parse_ops(&json!([
            {"op": "delete", "ref": "7C02", "id": "a"},
            {"op": "delete", "ref": "7C03", "id": "a"},
        ]))
        .unwrap_err();
        assert_eq!((e.op, e.error), (Some(1), bad("id")));
        for id in ["", "has space", "quote\"", "new\nline", "é"] {
            assert_eq!(
                field(one(json!({"op": "delete", "ref": "7C02", "id": id}))),
                "id",
                "{id:?}"
            );
        }
        assert!(one(json!({"op": "delete", "ref": "7C02", "id": "a-1_b.c:d"})).is_ok());
    }

    #[test]
    fn agent_text_is_normalised_and_hidden_text_refused() {
        let op = one(json!({"op": "set_text", "ref": "7C02", "text": "a\r\nb\rc\u{2028}d\u{85}e"}))
            .unwrap();
        assert!(matches!(&op.kind, OpKind::SetText { text, .. } if text == "a\nb\nc\nd\ne"));
        for bad_text in [
            "nul\u{0}",
            "esc\u{1b}[31m",
            "del\u{7f}",
            "c1\u{9b}",
            "zero\u{200b}width",
            "bom\u{feff}",
            "bidi\u{202e}txt",
            "isolate\u{2066}x\u{2069}",
            "tag\u{e0049}\u{e0067}",
            "word\u{2060}joiner",
        ] {
            assert_eq!(
                field(one(
                    json!({"op": "set_text", "ref": "7C02", "text": bad_text})
                )),
                "text",
                "{bad_text:?}"
            );
            assert_eq!(
                field(one(
                    json!({"op": "replace", "ref": "7C02", "find": bad_text, "with": "x"})
                )),
                "find"
            );
            assert_eq!(
                field(one(
                    json!({"op": "replace", "ref": "7C02", "find": "x", "with": bad_text})
                )),
                "with"
            );
            assert_eq!(
                field(one(
                    json!({"op": "set_text", "ref": "7C02", "text": "x", "why": bad_text})
                )),
                "why"
            );
            assert_eq!(
                field(one(
                    json!({"op": "set_cells", "sheet": "S", "at": "A1", "values": [[bad_text]]})
                )),
                "values"
            );
        }
        // Bengali and emoji sequences need U+200C and U+200D; tab and LF are text.
        assert!(one(
            json!({"op": "set_text", "ref": "7C02", "text": "ক্\u{200d}ষ \u{200c} a\tb\nc"})
        )
        .is_ok());
        // U+FFFC is the internal stand-in for an inline object.
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "a\u{fffc}", "with": "x"})
            )),
            "find"
        );
        assert_eq!(
            field(one(
                json!({"op": "replace", "ref": "7C02", "find": "a", "with": "\u{fffc}"})
            )),
            "with"
        );
    }

    #[test]
    fn formulas_must_balance() {
        assert!(brackets_balance(
            "=SUM(A1:B2)+IF(A1>0,\"a)\",'My (Sheet)'!A1)"
        ));
        assert!(brackets_balance("={1,2;3,4}"));
        assert!(brackets_balance("=\"it\"\"s\""));
        for f in ["=SUM(A1", "=A1)", "=(]", "=\"open", "=SUM('S!A1)", "={1,2"] {
            assert!(!brackets_balance(f), "{f}");
        }
    }

    #[test]
    fn the_propose_arguments() {
        let v = json!({
            "file": "7034d450-0000-4000-8000-000000000001",
            "rev": 412,
            "request_id": "5c1d0f4e-0000-4000-8000-000000000002",
            "name": "Terminology",
            "ops": [{"op": "replace_all", "find": "policy", "with": "procedure", "expect": 3}],
            "strict": true,
            "return": "diff"
        });
        let p = parse_propose(&v).unwrap();
        assert_eq!(
            (p.rev, p.strict, p.dry_run, p.ret),
            (412, true, false, ReturnKind::Diff)
        );
        assert_eq!(p.name.as_deref(), Some("Terminology"));
        assert_eq!(p.ops.len(), 1);
        for (key, value, field) in [
            ("rev", json!(-1), "rev"),
            ("rev", json!("412"), "rev"),
            ("file", json!("nope"), "file"),
            ("request_id", json!(1), "request_id"),
            ("proposal", json!("x"), "proposal"),
            ("strict", json!("yes"), "strict"),
            ("return", json!("everything"), "return"),
            ("unknown", json!(1), "unknown_field"),
        ] {
            let mut bad_args = v.clone();
            bad_args[key] = value;
            let e = parse_propose(&bad_args).unwrap_err();
            assert_eq!(e.error, bad(field), "{key}");
            assert_eq!(e.op, None);
        }
        let mut no_ops = v.clone();
        no_ops.as_object_mut().unwrap().remove("ops");
        assert_eq!(parse_propose(&no_ops).unwrap_err().error, bad("ops"));
        let mut long_name = v.clone();
        long_name["name"] = json!("n".repeat(201));
        assert!(matches!(
            parse_propose(&long_name).unwrap_err().error,
            AgentError::LimitExceeded { field: "name", .. }
        ));
    }
}
