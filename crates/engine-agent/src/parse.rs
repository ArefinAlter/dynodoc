//! Reference parsers for DCT v1 response bodies.
//!
//! These are the other half of the conformance contract (protocol spec 17): they read the
//! text a tool wrote back into its structure, and they are strict. Any line that does not
//! fit the grammar of its tool is an error, so a test that writes hostile document text
//! and parses the result proves the text could not forge a structure line. They return
//! escaped lines as written; use [`crate::escape::unescape`] for the text.
//!
//! The parsers take the lines after the header (see [`crate::dct::split_response`]).

use crate::dct::{err, ParseError, Scanner};
use crate::escape::unescape;

fn unesc(at: usize, s: &str) -> Result<String, ParseError> {
    unescape(s).map_err(|e| ParseError {
        at,
        message: e.to_string(),
    })
}

fn num(at: usize, s: &str) -> Result<usize, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return err(at, format!("not a number: {s:?}"));
    }
    s.parse().or_else(|_| err(at, "number out of range"))
}

fn hash6(at: usize, s: &str) -> Result<String, ParseError> {
    if s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        Ok(s.to_string())
    } else {
        err(at, format!("not a 6-hex hash: {s:?}"))
    }
}

fn ref_token(at: usize, s: &str) -> Result<String, ParseError> {
    if (4..=26).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()) {
        Ok(s.to_string())
    } else {
        err(at, format!("not a ref: {s:?}"))
    }
}

// ---------------------------------------------------------------- outline: documents

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub r: String,
    pub t: String,
    pub h: String,
    pub w: usize,
    /// Unescaped preview, `…` included when cut.
    pub preview: String,
    pub under: Option<(usize, usize)>,
}

/// `# ref type hash words preview` then `REF TYPE HASH Nw [preview] [ [Nb Nw]]`.
pub fn outline_blocks(lines: &[&str], depth: u8) -> Result<Vec<OutlineRow>, ParseError> {
    if lines.first() != Some(&"# ref type hash words preview") {
        return err(0, "missing outline column header");
    }
    let mut rows = Vec::new();
    for line in &lines[1..] {
        let mut s = Scanner::new(line);
        let r = ref_token(0, s.word()?)?;
        s.space()?;
        let t = s.word()?.to_string();
        s.space()?;
        let h = hash6(0, s.word()?)?;
        s.space()?;
        let words = s.word()?;
        let w = num(0, words.strip_suffix('w').ok_or(ParseError { at: 0, message: "words need a w".into() })?)?;
        let mut rest = s.take_rest();
        let mut under = None;
        if depth == 0 {
            // The last group is the tail; a preview that looks like one is followed by the real one.
            let idx = rest.rfind(" [").or(if rest.starts_with('[') { Some(0) } else { None });
            let idx = idx.ok_or(ParseError { at: 0, message: "depth 0 row without [Nb Nw]".into() })?;
            let tail = rest[idx..].trim_start();
            let inner = tail
                .strip_prefix('[')
                .and_then(|t| t.strip_suffix(']'))
                .ok_or(ParseError { at: 0, message: "bad tail".into() })?;
            let (b, w) = inner
                .split_once(' ')
                .ok_or(ParseError { at: 0, message: "bad tail".into() })?;
            under = Some((
                num(0, b.strip_suffix('b').unwrap_or("x"))?,
                num(0, w.strip_suffix('w').unwrap_or("x"))?,
            ));
            rest = &rest[..idx];
        }
        let preview = match rest.strip_prefix(' ') {
            Some(p) => unesc(0, p)?,
            None if rest.is_empty() => String::new(),
            None => return err(0, "expected a space before the preview"),
        };
        rows.push(OutlineRow { r, t, h, w, preview, under });
    }
    Ok(rows)
}

// ---------------------------------------------------------------- outline: sheets

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetListRow {
    pub r: String,
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub h: String,
}

pub fn outline_sheets(lines: &[&str]) -> Result<Vec<SheetListRow>, ParseError> {
    if lines.first() != Some(&"# ref name rows cols hash") {
        return err(0, "missing sheet column header");
    }
    lines[1..]
        .iter()
        .map(|line| {
            let mut s = Scanner::new(line);
            let r = ref_token(0, s.word()?)?;
            s.space()?;
            let name = s.quoted()?;
            let mut kv = |key: &str| -> Result<String, ParseError> {
                s.space()?;
                let at = s.pos();
                let w = s.word()?;
                w.strip_prefix(key)
                    .map(str::to_string)
                    .ok_or(ParseError { at, message: format!("expected {key}") })
            };
            let rows = num(0, &kv("rows=")?)?;
            let cols = num(0, &kv("cols=")?)?;
            let h = hash6(0, &kv("h=")?)?;
            Ok(SheetListRow { r, name, rows, cols, h })
        })
        .collect()
}

// ---------------------------------------------------------------- outline: profile

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRow {
    pub col: String,
    pub header: String,
    pub ty: String,
    /// `key=value` stats in order (`n`, `blank`, `distinct`, `min`, `max`, `sum`,
    /// `exceptions`); the text after `=` as written.
    pub stats: Vec<(String, String)>,
    /// `top="value":count,...`, values unescaped.
    pub top: Vec<(String, usize)>,
    /// `pattern="..."`, unescaped.
    pub pattern: Option<String>,
}

fn digits_here(s: &mut Scanner) -> Result<usize, ParseError> {
    let rest = s.rest();
    let n = rest.bytes().take_while(u8::is_ascii_digit).count();
    let at = s.pos();
    let digits = rest[..n].to_string();
    s.eat(&digits);
    num(at, &digits)
}

/// Returns the sheet name and the column rows.
pub fn profile(lines: &[&str]) -> Result<(String, Vec<ProfileRow>), ParseError> {
    let Some(first) = lines.first().and_then(|l| l.strip_prefix("sheet ")) else {
        return err(0, "missing sheet line");
    };
    let mut s = Scanner::new(first);
    let name = s.quoted()?;
    if !s.is_done() {
        return err(0, "trailing text after the sheet name");
    }
    if lines.get(1) != Some(&"# col \"header\" type stats") {
        return err(0, "missing profile column header");
    }
    let mut rows = Vec::new();
    for line in &lines[2..] {
        let mut s = Scanner::new(line);
        let col = s.word()?.to_string();
        if !col.bytes().all(|b| b.is_ascii_uppercase()) {
            return err(0, "bad column letters");
        }
        s.space()?;
        let header = s.quoted()?;
        s.space()?;
        let ty = s.word()?.to_string();
        let mut row = ProfileRow { col, header, ty, stats: Vec::new(), top: Vec::new(), pattern: None };
        while !s.is_done() {
            s.space()?;
            let at = s.pos();
            let rest = s.rest();
            let eq = rest.find('=').ok_or(ParseError { at, message: "stat without =".into() })?;
            let key = rest[..eq].to_string();
            s.eat(&key);
            s.eat("=");
            match key.as_str() {
                "top" => loop {
                    let value = s.quoted()?;
                    if !s.eat(":") {
                        return err(s.pos(), "top value without a count");
                    }
                    row.top.push((value, digits_here(&mut s)?));
                    if !s.eat(",") {
                        break;
                    }
                },
                "pattern" => row.pattern = Some(s.quoted()?),
                _ => row.stats.push((key, s.word()?.to_string())),
            }
        }
        rows.push(row);
    }
    Ok((name, rows))
}

// ---------------------------------------------------------------- outline: slides

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideOutlineRow {
    pub pos: usize,
    pub r: String,
    pub layout: String,
    pub title: String,
    pub body: usize,
    pub notes_words: usize,
    /// `(count, "88KB")` when images are omitted.
    pub images: Option<(usize, String)>,
    pub hidden: bool,
    pub shapes: Vec<ShapeOutlineRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeOutlineRow {
    pub r: String,
    pub tag: String,
    pub h: Option<String>,
    pub w: Option<usize>,
    pub preview: String,
    pub omitted: Option<String>,
}

pub fn outline_slides(lines: &[&str]) -> Result<Vec<SlideOutlineRow>, ParseError> {
    let mut rows: Vec<SlideOutlineRow> = Vec::new();
    for line in lines {
        if let Some(shape) = line.strip_prefix(" .") {
            let Some(slide) = rows.last_mut() else {
                return err(0, "shape line before any slide");
            };
            let mut s = Scanner::new(shape);
            let r = ref_token(0, s.word()?)?;
            s.space()?;
            let tag = s.word()?.to_string();
            let mut row = ShapeOutlineRow { r, tag: tag.clone(), h: None, w: None, preview: String::new(), omitted: None };
            if tag == "image" {
                s.space()?;
                row.omitted = Some(s.word()?.to_string());
                if s.take_rest() != " omitted" {
                    return err(0, "image line must end with ` omitted`");
                }
            } else if tag != "shape" {
                s.space()?;
                let at = s.pos();
                row.h = Some(hash6(at, s.word()?.strip_prefix('#').unwrap_or("x"))?);
                s.space()?;
                let w = s.word()?;
                row.w = Some(num(0, w.strip_suffix('w').unwrap_or("x"))?);
                let rest = s.take_rest();
                row.preview = match rest.strip_prefix(' ') {
                    Some(p) => unesc(0, p)?,
                    None if rest.is_empty() => String::new(),
                    None => return err(0, "bad shape preview"),
                };
            }
            slide.shapes.push(row);
            continue;
        }
        let mut s = Scanner::new(line);
        let label = s.word()?;
        let pos = num(0, label.strip_prefix('s').unwrap_or("x"))?;
        s.space()?;
        let r = ref_token(0, s.word()?)?;
        s.space()?;
        let layout = s.word()?.to_string();
        s.space()?;
        let title = s.quoted()?;
        let mut row = SlideOutlineRow { pos, r, layout, title, body: 0, notes_words: 0, images: None, hidden: false, shapes: Vec::new() };
        s.space()?;
        row.body = num(0, s.word()?.strip_prefix("body=").and_then(|b| b.strip_suffix('b')).unwrap_or("x"))?;
        s.space()?;
        row.notes_words = num(0, s.word()?.strip_prefix("notes=").and_then(|b| b.strip_suffix('w')).unwrap_or("x"))?;
        while !s.is_done() {
            s.space()?;
            if s.eat("img=") {
                let at = s.pos();
                let w = s.word()?;
                let (n, rest) = w.split_once('(').ok_or(ParseError { at, message: "bad img".into() })?;
                s.space()?;
                if s.word()? != "omitted)" {
                    return err(at, "img must end with `omitted)`");
                }
                row.images = Some((num(at, n)?, rest.to_string()));
            } else if s.eat("hidden=1") {
                row.hidden = true;
            } else {
                return err(s.pos(), "unexpected slide field");
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

// ---------------------------------------------------------------- read: blocks

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBlock {
    /// `7C02`, `7C02.3` or `3B77.r2c2`.
    pub label: String,
    pub t: String,
    pub h: Option<String>,
    pub features: String,
    pub equal: bool,
    pub redacted: bool,
    /// Body lines as written (escaped).
    pub lines: Vec<String>,
}

impl ReadBlock {
    /// Unescaped text of a one-line block.
    pub fn text(&self) -> Result<String, ParseError> {
        match self.lines.as_slice() {
            [one] => unesc(0, one),
            _ => err(0, "not a one-line block"),
        }
    }
}

fn expected_lines(t: &str) -> Result<(usize, &'static str), ParseError> {
    if let Some(n) = t.strip_prefix("UL").or_else(|| t.strip_prefix("OL")) {
        return Ok((num(0, n)?, "list"));
    }
    if let Some(dims) = t.strip_prefix("TBL") {
        let (r, _) = dims.split_once('x').ok_or(ParseError { at: 0, message: "bad table type".into() })?;
        return Ok((num(0, r)?, "table"));
    }
    Ok((1, "text"))
}

/// Blocks of a document `read`: `@label TYPE #hash [{features}]` then the body lines.
pub fn read_blocks(lines: &[&str]) -> Result<Vec<ReadBlock>, ParseError> {
    let mut out: Vec<ReadBlock> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(head) = line.strip_prefix('@') {
            let mut s = Scanner::new(head);
            let label = s.word()?.to_string();
            s.space()?;
            let first = s.word()?;
            let mut block = ReadBlock {
                label,
                t: String::new(),
                h: None,
                features: String::new(),
                equal: false,
                redacted: false,
                lines: Vec::new(),
            };
            if first == "=" {
                s.space()?;
                block.equal = true;
                block.h = Some(hash6(i, s.word()?.strip_prefix('#').unwrap_or("x"))?);
            } else if first == "X" && s.rest() == " redacted" {
                block.t = "X".into();
                block.redacted = true;
                s.take_rest();
            } else {
                block.t = first.to_string();
                s.space()?;
                block.h = Some(hash6(i, s.word()?.strip_prefix('#').unwrap_or("x"))?);
                if !s.is_done() {
                    s.space()?;
                    let f = s.take_rest();
                    if !(f.starts_with('{') && f.ends_with('}')) {
                        return err(i, "bad feature braces");
                    }
                    block.features = f.to_string();
                }
            }
            if !s.is_done() {
                return err(i, "trailing text on a block line");
            }
            out.push(block);
        } else {
            match out.last_mut() {
                Some(b) => b.lines.push((*line).to_string()),
                None => return err(i, "body line before any block"),
            }
        }
    }
    for (i, b) in out.iter().enumerate() {
        if b.equal || b.redacted {
            if !b.lines.is_empty() {
                return err(i, "equal or redacted block with body lines");
            }
            continue;
        }
        let (n, kind) = if b.label.contains('.') { (1, "text") } else { expected_lines(&b.t)? };
        if b.lines.len() != n {
            return err(i, format!("block {} wants {n} lines, has {}", b.label, b.lines.len()));
        }
        match kind {
            "list" => {
                for l in &b.lines {
                    let t = l.trim_start_matches(' ');
                    if (l.len() - t.len()) % 2 != 0 || !t.starts_with("- ") {
                        return err(i, "list line without `- `");
                    }
                }
            }
            "table" => {
                for (k, l) in b.lines.iter().enumerate() {
                    if !l.starts_with(&format!("r{}", k + 1)) || l.as_bytes().get(format!("r{}", k + 1).len()).is_some_and(|c| *c != b'\t') {
                        return err(i, "table row without its r<n> prefix");
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Unescaped item texts of a list block body.
pub fn list_items(block: &ReadBlock) -> Result<Vec<(usize, String)>, ParseError> {
    block
        .lines
        .iter()
        .map(|l| {
            let t = l.trim_start_matches(' ');
            let depth = (l.len() - t.len()) / 2;
            let text = t.strip_prefix("- ").ok_or(ParseError { at: 0, message: "no `- `".into() })?;
            Ok((depth, unesc(0, text)?))
        })
        .collect()
}

/// Unescaped cells of each table row (after the `r<n>` prefix).
pub fn table_rows(block: &ReadBlock) -> Result<Vec<Vec<String>>, ParseError> {
    block
        .lines
        .iter()
        .map(|l| {
            let mut parts = l.split('\t');
            parts.next();
            parts.map(|c| unesc(0, c)).collect()
        })
        .collect()
}

// ---------------------------------------------------------------- read: range, slides

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBody {
    pub columns: Vec<String>,
    /// `(row number, optional row ref, unescaped cells)`.
    pub rows: Vec<(u32, Option<String>, Vec<String>)>,
}

pub fn range_body(lines: &[&str], row_refs: bool) -> Result<RangeBody, ParseError> {
    let Some(head) = lines.first().and_then(|l| l.strip_prefix('\t')) else {
        return err(0, "range body must start with a TAB column line");
    };
    let mut cols: Vec<&str> = head.split('\t').collect();
    if row_refs {
        if cols.first() != Some(&"") {
            return err(0, "rows=refs header needs an empty ref column");
        }
        cols.remove(0);
    }
    for c in &cols {
        if c.is_empty() || !c.bytes().all(|b| b.is_ascii_uppercase()) {
            return err(0, "bad column letters");
        }
    }
    let mut rows = Vec::new();
    for (i, line) in lines[1..].iter().enumerate() {
        let mut parts = line.split('\t');
        let n = num(i, parts.next().unwrap_or(""))? as u32;
        let r = if row_refs {
            let t = parts.next().ok_or(ParseError { at: i, message: "missing ref column".into() })?;
            (!t.is_empty()).then(|| ref_token(i, t)).transpose()?
        } else {
            None
        };
        let cells: Vec<String> = parts.map(|c| unesc(i, c)).collect::<Result<_, _>>()?;
        if cells.len() != cols.len() {
            return err(i, format!("row {n} has {} cells for {} columns", cells.len(), cols.len()));
        }
        rows.push((n, r, cells));
    }
    Ok(RangeBody {
        columns: cols.iter().map(|c| (*c).to_string()).collect(),
        rows,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeBody {
    pub r: String,
    pub tag: String,
    pub h: Option<String>,
    pub equal: bool,
    pub redacted: bool,
    pub alt: Option<String>,
    pub omitted: Option<String>,
    /// `(bulleted, unescaped text)` per line.
    pub lines: Vec<(bool, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideBody {
    pub pos: usize,
    pub r: String,
    pub layout: String,
    pub hidden: bool,
    pub redacted: bool,
    pub shapes: Vec<ShapeBody>,
    pub notes: Option<(String, String)>,
}

pub fn read_slides(lines: &[&str]) -> Result<Vec<SlideBody>, ParseError> {
    let mut out: Vec<SlideBody> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(head) = line.strip_prefix("@s") {
            let mut s = Scanner::new(head);
            let pos = num(i, s.word()?)?;
            s.space()?;
            let r = ref_token(i, s.word()?)?;
            s.space()?;
            let layout = s.word()?.to_string();
            let mut slide = SlideBody { pos, r, layout: layout.clone(), hidden: false, redacted: false, shapes: Vec::new(), notes: None };
            if layout == "X" {
                if s.take_rest() != " redacted" {
                    return err(i, "bad redacted slide");
                }
                slide.redacted = true;
                slide.layout = String::new();
            } else {
                s.space()?;
                slide.hidden = match s.word()? {
                    "hidden=0" => false,
                    "hidden=1" => true,
                    _ => return err(i, "bad hidden"),
                };
                if !s.is_done() {
                    return err(i, "trailing text on a slide line");
                }
            }
            out.push(slide);
        } else if let Some(shape) = line.strip_prefix(" .") {
            let Some(slide) = out.last_mut() else {
                return err(i, "shape before any slide");
            };
            let mut s = Scanner::new(shape);
            let r = ref_token(i, s.word()?)?;
            s.space()?;
            let tag = s.word()?.to_string();
            let mut sb = ShapeBody { r, tag: tag.clone(), h: None, equal: false, redacted: false, alt: None, omitted: None, lines: Vec::new() };
            match tag.as_str() {
                "=" => {
                    s.space()?;
                    sb.equal = true;
                    sb.h = Some(hash6(i, s.word()?.strip_prefix('#').unwrap_or("x"))?);
                }
                "X" => {
                    if s.take_rest() != " redacted" {
                        return err(i, "bad redacted shape");
                    }
                    sb.redacted = true;
                }
                "image" | "shape" => {
                    if s.rest().starts_with(" alt=") {
                        s.eat(" alt=");
                        sb.alt = Some(s.quoted()?);
                    }
                    if tag == "image" {
                        s.space()?;
                        sb.omitted = Some(s.word()?.to_string());
                        if s.take_rest() != " omitted" {
                            return err(i, "image must end with ` omitted`");
                        }
                    }
                }
                _ => {
                    s.space()?;
                    sb.h = Some(hash6(i, s.word()?.strip_prefix('#').unwrap_or("x"))?);
                }
            }
            if !s.is_done() {
                return err(i, "trailing text on a shape line");
            }
            slide.shapes.push(sb);
        } else if let Some(text) = line.strip_prefix(' ') {
            let shape = out.last_mut().and_then(|s| s.shapes.last_mut());
            let Some(shape) = shape else {
                return err(i, "text line before any shape");
            };
            match text.strip_prefix("- ") {
                Some(t) => shape.lines.push((true, unesc(i, t)?)),
                None => shape.lines.push((false, unesc(i, text)?)),
            }
        } else if let Some(notes) = line.strip_prefix("notes #") {
            let Some(slide) = out.last_mut() else {
                return err(i, "notes before any slide");
            };
            let (h, text) = notes.split_once(": ").ok_or(ParseError { at: i, message: "bad notes line".into() })?;
            slide.notes = Some((hash6(i, h)?, unesc(i, text)?));
        } else {
            return err(i, format!("unrecognised line {line:?}"));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- find, files, diff

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindHit {
    pub at: String,
    pub snippet: String,
}

pub fn find_hits(lines: &[&str]) -> Result<Vec<FindHit>, ParseError> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let body = line.strip_prefix('@').unwrap_or(line);
            let (at, snippet) = body.split_once(' ').ok_or(ParseError { at: i, message: "hit without snippet".into() })?;
            let base = at.split(['.', '!']).next().unwrap_or("");
            ref_token(i, base)?;
            Ok(FindHit { at: at.to_string(), snippet: unesc(i, snippet)? })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLine {
    pub file: String,
    pub kind: String,
    pub rev: u64,
    pub project: String,
    pub role: String,
    pub title: String,
}

pub fn files_lines(lines: &[&str]) -> Result<Vec<FileLine>, ParseError> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let mut s = Scanner::new(line);
            let file = s.word()?.to_string();
            if file.len() != 8 || !file.bytes().all(|b| b.is_ascii_hexdigit()) {
                return err(i, "file id must be 8 hex characters");
            }
            s.space()?;
            let kind = s.word()?.to_string();
            s.space()?;
            let rev = num(i, s.word()?.strip_prefix("rev=").unwrap_or("x"))? as u64;
            s.space()?;
            if !s.eat("project=") {
                return err(i, "missing project");
            }
            let project = s.value()?;
            s.space()?;
            if !s.eat("role=") {
                return err(i, "missing role");
            }
            let role = s.value()?;
            s.space()?;
            let title = s.quoted()?;
            if !s.is_done() {
                return err(i, "trailing text");
            }
            Ok(FileLine { file, kind, rev, project, role, title })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffLine {
    pub op: char,
    pub r: String,
    pub t: String,
    pub old: Option<String>,
    pub new: Option<String>,
    pub row: Option<u32>,
    pub pos: Option<usize>,
    pub scope: Option<String>,
    pub cells: Option<usize>,
    pub chars: Option<(usize, usize)>,
    pub after: Option<String>,
    pub preview: Option<String>,
    /// Detail lines as written (escaped).
    pub detail: Vec<String>,
}

pub fn diff_lines(lines: &[&str]) -> Result<Vec<DiffLine>, ParseError> {
    let mut out: Vec<DiffLine> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let sigil = line.chars().next();
        if let Some(op @ ('~' | '+' | '-' | '>')) = sigil {
            let mut s = Scanner::new(&line[1..]);
            let mut d = DiffLine { op, ..DiffLine::default() };
            d.r = ref_token(i, s.word()?)?;
            s.space()?;
            d.t = s.word()?.to_string();
            while !s.is_done() {
                s.space()?;
                let at = s.pos();
                if s.rest().starts_with('"') {
                    d.preview = Some(s.quoted()?);
                    continue;
                }
                let w = s.word()?;
                if let Some(h) = w.strip_prefix('#') {
                    match h.split_once('>') {
                        Some((a, b)) => {
                            d.old = Some(hash6(at, a)?);
                            d.new = Some(hash6(at, b)?);
                        }
                        None => {
                            let h = hash6(at, h)?;
                            if op == '-' {
                                d.old = Some(h);
                            } else {
                                d.new = Some(h);
                            }
                        }
                    }
                } else if let Some(n) = w.strip_prefix("cells=") {
                    d.cells = Some(num(at, n)?);
                } else if w == "in" {
                    s.space()?;
                    d.scope = Some(ref_token(at, s.word()?)?);
                } else if w == "after" {
                    s.space()?;
                    let a = s.word()?;
                    d.after = Some(if a == "first" { a.to_string() } else { ref_token(at, a)? });
                } else if let Some(n) = w.strip_prefix('-') {
                    let del = num(at, n)?;
                    s.space()?;
                    let ins = num(at, s.word()?.strip_prefix('+').unwrap_or("x"))?;
                    d.chars = Some((del, ins));
                } else if let Some(n) = w.strip_prefix('r') {
                    d.row = Some(num(at, n)? as u32);
                } else if let Some(n) = w.strip_prefix('s') {
                    d.pos = Some(num(at, n)?);
                } else {
                    return err(at, format!("unexpected diff field {w:?}"));
                }
            }
            out.push(d);
        } else {
            match out.last_mut() {
                Some(d) => d.detail.push((*line).to_string()),
                None => return err(i, "detail line before any entry"),
            }
        }
    }
    Ok(out)
}
