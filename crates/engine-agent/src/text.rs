//! Block text (protocol spec 6.3, 6.4, 6.6): turns the ProseMirror-style JSON in a
//! block's `content` into type labels, feature counts and escaped text lines.
//!
//! Inline atoms become `⟦kind:value⟧` placeholders (spec 6.4). `hardBreak` renders as a
//! line feed (`\n`); `richtext::plain_text` drops it, which would glue words together.
//! Words and `find` use [`Inline::plain`], which equals `plain_text` for text-only
//! blocks.

use serde_json::Value;

use crate::escape::{escape_with, guard_line_start, Ctx, Dropped, EXTRA_MARKDOWN};

/// `fmt` of a `read`: `plain` drops marks and link targets, `md` renders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fmt {
    #[default]
    Plain,
    Md,
}

impl Fmt {
    pub fn as_str(self) -> &'static str {
        match self {
            Fmt::Plain => "plain",
            Fmt::Md => "md",
        }
    }

    pub fn parse(s: &str) -> Option<Fmt> {
        match s {
            "plain" => Some(Fmt::Plain),
            "md" => Some(Fmt::Md),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    /// Document text; escaped on output (with the markdown extras when `fmt=md`).
    Text(String),
    /// Document text inside a code span; escaped on output without the markdown extras.
    Code(String),
    /// Markup the server writes verbatim (`**`, `](url)`): never document-controlled.
    Markup(String),
    /// An inline object. `plain` is its contribution to plain text (`richtext::plain_text`).
    Atom {
        kind: String,
        value: String,
        plain: String,
    },
}

/// Counts of things plain text does not show (spec 6.4 braces).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Features {
    pub m: u32,
    pub lnk: u32,
    pub fn_: u32,
    pub math: u32,
    pub cm: u32,
    pub img: u32,
    pub rich: bool,
}

impl Features {
    pub fn add(&mut self, o: &Features) {
        self.m += o.m;
        self.lnk += o.lnk;
        self.fn_ += o.fn_;
        self.math += o.math;
        self.cm += o.cm;
        self.img += o.img;
        self.rich |= o.rich;
    }

    /// `{m=2,lnk=1}` in fixed order, or empty when there is nothing to show.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        for (k, v) in [
            ("m", self.m),
            ("lnk", self.lnk),
            ("fn", self.fn_),
            ("math", self.math),
            ("cm", self.cm),
            ("img", self.img),
        ] {
            if v > 0 {
                parts.push(format!("{k}={v}"));
            }
        }
        if self.rich {
            parts.push("rich=1".into());
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// A run of segments forming one logical text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inline {
    pub segs: Vec<Seg>,
}

fn atom_kind(raw: &str) -> String {
    let k: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if k.is_empty() {
        "x".into()
    } else {
        k
    }
}

impl Inline {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            segs: vec![Seg::Text(s.into())],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    pub fn push_newline(&mut self) {
        self.segs.push(Seg::Text("\n".into()));
    }

    pub fn extend(&mut self, other: Inline) {
        self.segs.extend(other.segs);
    }

    /// Plain text: text, code and the `plain` part of atoms.
    pub fn plain(&self) -> String {
        let mut out = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Text(t) | Seg::Code(t) => out.push_str(t),
                Seg::Atom { plain, .. } => out.push_str(plain),
                Seg::Markup(_) => {}
            }
        }
        out
    }

    /// Escaped text for the middle of a line.
    pub fn render(&self, md: bool) -> (String, Dropped) {
        let mut out = String::new();
        let mut dropped = Dropped::default();
        for seg in &self.segs {
            match seg {
                Seg::Text(t) => {
                    let (e, d) = escape_with(t, if md { EXTRA_MARKDOWN } else { "" });
                    out.push_str(&e);
                    dropped.add(d);
                }
                Seg::Code(t) => {
                    let (e, d) = escape_with(t, "");
                    out.push_str(&e);
                    dropped.add(d);
                }
                Seg::Markup(m) => out.push_str(m),
                Seg::Atom { kind, value, .. } => {
                    out.push('⟦');
                    out.push_str(kind);
                    if !value.is_empty() {
                        out.push(':');
                        let (e, d) = escape_with(value, "");
                        out.push_str(&e);
                        dropped.add(d);
                    }
                    out.push('⟧');
                }
            }
        }
        (out, dropped)
    }

    /// Escaped text that starts a line in `ctx`.
    pub fn render_line(&self, md: bool, ctx: Ctx) -> (String, Dropped) {
        let (text, d) = self.render(md);
        (guard_line_start(text, ctx), d)
    }

    /// First `max` characters, `…` appended when cut (spec 6.3 `preview`). Atoms count as
    /// one character and show as `⟦kind⟧`. Returns the escaped preview.
    pub fn preview(&self, max: usize) -> (String, Dropped) {
        self.preview_with(max, "")
    }

    /// [`preview`](Self::preview) that also escapes the `extra` characters (for quotes).
    pub fn preview_with(&self, max: usize, extra: &str) -> (String, Dropped) {
        let (segs, cut) = self.preview_segs(max);
        let mut text = String::new();
        let mut dropped = Dropped::default();
        for seg in &segs {
            match seg {
                Seg::Text(t) | Seg::Code(t) => {
                    let (e, d) = escape_with(t, extra);
                    text.push_str(&e);
                    dropped.add(d);
                }
                other => {
                    let (e, d) = Inline { segs: vec![other.clone()] }.render(false);
                    text.push_str(&e);
                    dropped.add(d);
                }
            }
        }
        if cut {
            text.push('…');
        }
        (text, dropped)
    }

    /// The same preview as sanitised plain text, for the typed data and the JSON mirror.
    pub fn preview_raw(&self, max: usize) -> String {
        let (segs, cut) = self.preview_segs(max);
        let mut out = String::new();
        for seg in segs {
            match seg {
                Seg::Text(t) => out.push_str(&crate::escape::sanitize(&t)),
                Seg::Atom { kind, .. } => {
                    out.push('⟦');
                    out.push_str(&kind);
                    out.push('⟧');
                }
                Seg::Code(_) | Seg::Markup(_) => {}
            }
        }
        if cut {
            out.push('…');
        }
        out
    }

    fn preview_segs(&self, max: usize) -> (Vec<Seg>, bool) {
        let mut taken: Vec<Seg> = Vec::new();
        let mut left = max;
        let mut cut = false;
        'segs: for seg in &self.segs {
            match seg {
                Seg::Text(t) | Seg::Code(t) => {
                    let clean = crate::escape::sanitize(t);
                    let n = clean.chars().count();
                    if n <= left {
                        left -= n;
                        taken.push(Seg::Text(t.clone()));
                    } else {
                        taken.push(Seg::Text(clean.chars().take(left).collect()));
                        cut = true;
                        break 'segs;
                    }
                }
                Seg::Markup(_) => {}
                Seg::Atom { kind, .. } => {
                    if left == 0 {
                        cut = true;
                        break 'segs;
                    }
                    left -= 1;
                    taken.push(Seg::Atom {
                        kind: kind.clone(),
                        value: String::new(),
                        plain: String::new(),
                    });
                }
            }
        }
        (taken, cut)
    }
}

fn percent_encode_href(href: &str) -> Option<String> {
    let lower = href.trim().to_ascii_lowercase();
    let scheme_ok = match lower.split_once(':') {
        Some((scheme, _)) if scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') => {
            matches!(scheme, "http" | "https" | "mailto")
        }
        _ => true,
    };
    if !scheme_ok {
        return None;
    }
    let mut out = String::new();
    for b in href.trim().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'*+,;=%".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    (!out.is_empty()).then_some(out)
}

fn wrap_run(text: &str, marks: &[Value], md: bool, segs: &mut Vec<Seg>, f: &mut Features) {
    let kinds: Vec<&str> = marks.iter().filter_map(|m| m["type"].as_str()).collect();
    let has = |k: &str| kinds.contains(&k);
    if kinds.contains(&"link") {
        f.lnk += 1;
    }
    if kinds.iter().any(|k| matches!(*k, "sourceComment" | "comment")) {
        f.cm += 1;
    }
    if kinds
        .iter()
        .any(|k| !matches!(*k, "link" | "sourceComment" | "comment"))
    {
        f.m += 1;
    }
    let wrap = md && !text.trim().is_empty();
    if !wrap {
        segs.push(Seg::Text(text.to_string()));
        return;
    }
    let core = text.trim_matches(|c: char| c == ' ' || c == '\t');
    let lead = &text[..text.len() - text.trim_start_matches([' ', '\t']).len()];
    let trail = &text[lead.len() + core.len()..];
    if !lead.is_empty() {
        segs.push(Seg::Text(lead.to_string()));
    }
    let mut closers: Vec<String> = Vec::new();
    if let Some(href) = marks
        .iter()
        .find(|m| m["type"] == "link")
        .and_then(|m| m["attrs"]["href"].as_str())
        .and_then(percent_encode_href)
    {
        segs.push(Seg::Markup("[".into()));
        closers.push(format!("]({href})"));
    }
    for (kind, open) in [("bold", "**"), ("italic", "_"), ("strike", "~~")] {
        if has(kind) {
            segs.push(Seg::Markup(open.into()));
            closers.push(open.into());
        }
    }
    if has("code") && !core.contains('`') {
        segs.push(Seg::Markup("`".into()));
        segs.push(Seg::Code(core.to_string()));
        segs.push(Seg::Markup("`".into()));
    } else {
        segs.push(Seg::Text(core.to_string()));
    }
    for c in closers.into_iter().rev() {
        segs.push(Seg::Markup(c));
    }
    if !trail.is_empty() {
        segs.push(Seg::Text(trail.to_string()));
    }
}

/// Inline content of a container node (paragraph, heading, code block, ...).
pub fn inline_of(container: &Value, md: bool) -> (Inline, Features) {
    let mut segs = Vec::new();
    let mut f = Features::default();
    walk_inline(container, md, &mut segs, &mut f);
    (Inline { segs }, f)
}

fn walk_inline(container: &Value, md: bool, segs: &mut Vec<Seg>, f: &mut Features) {
    let Some(children) = container["content"].as_array() else {
        return;
    };
    for child in children {
        match child["type"].as_str().unwrap_or("") {
            "text" => {
                let text = child["text"].as_str().unwrap_or("");
                let marks = child["marks"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                if marks.is_empty() {
                    segs.push(Seg::Text(text.to_string()));
                } else {
                    wrap_run(text, marks, md, segs, f);
                }
            }
            "hardBreak" => segs.push(Seg::Text("\n".into())),
            "inlineMath" | "blockMath" => {
                let latex = child["attrs"]["latex"].as_str().unwrap_or("").to_string();
                f.math += 1;
                f.rich = true;
                segs.push(Seg::Atom {
                    kind: "math".into(),
                    value: latex.clone(),
                    plain: latex,
                });
            }
            "documentNote" => {
                let id = child["attrs"]["id"].as_str().unwrap_or("").to_string();
                let kind = if child["attrs"]["kind"] == "endnote" { "en" } else { "fn" };
                f.fn_ += 1;
                f.rich = true;
                segs.push(Seg::Atom {
                    kind: kind.into(),
                    value: id,
                    plain: String::new(),
                });
            }
            "image" => {
                f.img += 1;
                f.rich = true;
                segs.push(Seg::Atom {
                    kind: "img".into(),
                    value: child["attrs"]["alt"].as_str().unwrap_or("").chars().take(40).collect(),
                    plain: String::new(),
                });
            }
            other => {
                f.rich = true;
                if child["content"].is_array() {
                    walk_inline(child, md, segs, f);
                } else {
                    segs.push(Seg::Atom {
                        kind: atom_kind(other),
                        value: String::new(),
                        plain: child["text"].as_str().unwrap_or("").to_string(),
                    });
                }
            }
        }
    }
}

fn is_list_type(t: &str) -> bool {
    matches!(t, "bulletList" | "orderedList" | "taskList")
}

fn is_text_container(t: &str) -> bool {
    matches!(t, "paragraph" | "heading" | "codeBlock")
}

/// All text under a block-level node, block children separated by a line feed.
pub fn flatten(node: &Value, md: bool, f: &mut Features) -> Inline {
    let t = node["type"].as_str().unwrap_or("");
    if is_text_container(t) {
        let (inline, nf) = inline_of(node, md);
        f.add(&nf);
        return inline;
    }
    let Some(children) = node["content"].as_array() else {
        return Inline::default();
    };
    let mut out = Inline::default();
    for (i, child) in children.iter().enumerate() {
        if i > 0 {
            out.push_newline();
        }
        out.extend(flatten(child, md, f));
    }
    out
}

/// One list item: nesting depth (0 for top level) and its own text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub depth: usize,
    pub text: Inline,
}

fn collect_items(list: &Value, depth: usize, md: bool, out: &mut Vec<ListItem>, f: &mut Features) {
    let Some(items) = list["content"].as_array() else {
        return;
    };
    for li in items {
        let mut text = Inline::default();
        let mut first = true;
        let mut nested: Vec<&Value> = Vec::new();
        for child in li["content"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            if is_list_type(child["type"].as_str().unwrap_or("")) {
                nested.push(child);
                continue;
            }
            if !first {
                text.push_newline();
            }
            first = false;
            text.extend(flatten(child, md, f));
        }
        out.push(ListItem { depth, text });
        for n in nested {
            collect_items(n, depth + 1, md, out, f);
        }
    }
}

/// The body of a block, ready to be rendered line by line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Text(Inline),
    List(Vec<ListItem>),
    Table(Vec<Vec<Inline>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockText {
    /// `P`, `H1`..`H6`, `UL<n>`, `OL<n>`, `TBL<r>x<c>`, `CODE`, `QUOTE`, `MATH`, `IMG`,
    /// `HR`, `LI`, `X`.
    pub label: String,
    pub body: Body,
    pub features: Features,
}

impl BlockText {
    /// Plain text of the whole block (items and cells separated by whitespace).
    pub fn plain(&self) -> String {
        match &self.body {
            Body::Text(i) => i.plain(),
            Body::List(items) => items
                .iter()
                .map(|i| i.text.plain())
                .collect::<Vec<_>>()
                .join("\n"),
            Body::Table(rows) => rows
                .iter()
                .map(|r| r.iter().map(Inline::plain).collect::<Vec<_>>().join("\t"))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    pub fn words(&self) -> usize {
        self.plain().split_whitespace().count()
    }

    /// The text for the outline preview: items and cells joined by a space.
    pub fn preview_inline(&self) -> Inline {
        match &self.body {
            Body::Text(i) => i.clone(),
            Body::List(items) => {
                let mut out = Inline::default();
                for (n, i) in items.iter().enumerate() {
                    if n > 0 {
                        out.segs.push(Seg::Text(" ".into()));
                    }
                    out.extend(i.text.clone());
                }
                out
            }
            Body::Table(rows) => {
                let mut out = Inline::default();
                let mut first = true;
                for cell in rows.iter().flatten() {
                    if !first {
                        out.segs.push(Seg::Text(" ".into()));
                    }
                    first = false;
                    out.extend(cell.clone());
                }
                out
            }
        }
    }
}

/// Analyse the `content` JSON of one top-level block.
pub fn analyze_block(content: &Value, md: bool) -> BlockText {
    let t = content["type"].as_str().unwrap_or("");
    let mut f = Features::default();
    let (label, body) = match t {
        "paragraph" => ("P".to_string(), Body::Text(flatten(content, md, &mut f))),
        "heading" => {
            let level = content["attrs"]["level"].as_u64().unwrap_or(1).clamp(1, 6);
            (format!("H{level}"), Body::Text(flatten(content, md, &mut f)))
        }
        "codeBlock" => ("CODE".into(), Body::Text(flatten(content, md, &mut f))),
        "blockquote" => ("QUOTE".into(), Body::Text(flatten(content, md, &mut f))),
        "blockMath" => (
            "MATH".into(),
            Body::Text(Inline::text(content["attrs"]["latex"].as_str().unwrap_or(""))),
        ),
        "image" => (
            "IMG".into(),
            Body::Text(Inline::text(content["attrs"]["alt"].as_str().unwrap_or(""))),
        ),
        "horizontalRule" => ("HR".into(), Body::Text(Inline::default())),
        "listItem" | "taskItem" => ("LI".into(), Body::Text(flatten(content, md, &mut f))),
        l if is_list_type(l) => {
            let mut items = Vec::new();
            collect_items(content, 0, md, &mut items, &mut f);
            let kind = if l == "orderedList" { "OL" } else { "UL" };
            (format!("{kind}{}", items.len()), Body::List(items))
        }
        "table" => {
            let rows: Vec<Vec<Inline>> = content["content"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .map(|row| {
                    row["content"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .map(|cell| flatten(cell, md, &mut f))
                        .collect()
                })
                .collect();
            let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
            (format!("TBL{}x{cols}", rows.len()), Body::Table(rows))
        }
        _ => {
            f.rich = true;
            let mut inner = Features::default();
            let text = flatten(content, md, &mut inner);
            f.add(&inner);
            ("X".into(), Body::Text(text))
        }
    };
    BlockText {
        label,
        body,
        features: f,
    }
}

/// Rendered lines of a block body (without the `@ref` line): one line for text blocks,
/// one per item for lists (two spaces of indentation per level, `- ` prefix) and one per
/// row for tables (`r<n>` TAB cells).
pub fn body_lines(block: &BlockText, md: bool, ctx: Ctx) -> (Vec<String>, Dropped) {
    let mut dropped = Dropped::default();
    let mut lines = Vec::new();
    match &block.body {
        Body::Text(i) => {
            let (l, d) = i.render_line(md, ctx);
            dropped.add(d);
            lines.push(l);
        }
        Body::List(items) => {
            for item in items {
                let (l, d) = item.text.render(md);
                dropped.add(d);
                lines.push(format!("{}- {l}", "  ".repeat(item.depth)));
            }
        }
        Body::Table(rows) => {
            for (n, row) in rows.iter().enumerate() {
                let mut line = format!("r{}", n + 1);
                for cell in row {
                    let (c, d) = cell.render(md);
                    dropped.add(d);
                    line.push('\t');
                    line.push_str(&c);
                }
                lines.push(line);
            }
        }
    }
    (lines, dropped)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn para(runs: Value) -> Value {
        json!({"type": "paragraph", "content": runs})
    }

    #[test]
    fn plain_equals_engine_plain_text_for_text_blocks() {
        let c = para(json!([
            {"type": "text", "text": "Hello "},
            {"type": "text", "text": "world", "marks": [{"type": "bold"}]}
        ]));
        let b = analyze_block(&c, false);
        assert_eq!(b.plain(), engine_core::richtext::plain_text(&c));
        assert_eq!(b.label, "P");
        assert_eq!(b.features.render(), "{m=1}");
    }

    #[test]
    fn atoms_become_placeholders() {
        let c = para(json!([
            {"type": "text", "text": "See "},
            {"type": "documentNote", "attrs": {"id": "N1", "kind": "footnote", "body": "x"}},
            {"type": "inlineMath", "attrs": {"latex": "a^2\\⟧"}}
        ]));
        let b = analyze_block(&c, false);
        let (lines, _) = body_lines(&b, false, Ctx::Block);
        assert_eq!(lines, ["See ⟦fn:N1⟧⟦math:a^2\\\\\\⟧⟧"]);
        assert_eq!(b.features.render(), "{fn=1,math=1,rich=1}");
    }

    #[test]
    fn markdown_marks_and_links() {
        let c = para(json!([
            {"type": "text", "text": "a "},
            {"type": "text", "text": "bold ", "marks": [{"type": "bold"}]},
            {"type": "text", "text": "site", "marks": [{"type": "link", "attrs": {"href": "https://x.test/a b)"}}]},
            {"type": "text", "text": "js", "marks": [{"type": "link", "attrs": {"href": "javascript:alert(1)"}}]},
            {"type": "text", "text": "2*3", "marks": []}
        ]));
        let b = analyze_block(&c, true);
        let (lines, _) = body_lines(&b, true, Ctx::Block);
        assert_eq!(lines, ["a **bold** [site](https://x.test/a%20b%29)js2\\*3"]);
        assert_eq!(b.features.render(), "{m=1,lnk=2}");
        let plain = analyze_block(&c, false);
        assert_eq!(body_lines(&plain, false, Ctx::Block).0, ["a bold sitejs2*3"]);
    }

    #[test]
    fn lists_tables_quotes() {
        let list = json!({"type": "bulletList", "content": [
            {"type": "listItem", "content": [para(json!([{"type": "text", "text": "one"}])),
                {"type": "orderedList", "content": [
                    {"type": "listItem", "content": [para(json!([{"type": "text", "text": "nested"}]))]}]}]},
            {"type": "listItem", "content": [para(json!([{"type": "text", "text": "two"}]))]}
        ]});
        let b = analyze_block(&list, false);
        assert_eq!(b.label, "UL3");
        assert_eq!(body_lines(&b, false, Ctx::Block).0, ["- one", "  - nested", "- two"]);
        let table = json!({"type": "table", "content": [
            {"type": "tableRow", "content": [
                {"type": "tableHeader", "content": [para(json!([{"type": "text", "text": "Name"}]))]},
                {"type": "tableHeader", "content": [para(json!([{"type": "text", "text": "Owner"}]))]}]},
            {"type": "tableRow", "content": [
                {"type": "tableCell", "content": [para(json!([{"type": "text", "text": "Intake"}]))]},
                {"type": "tableCell", "content": [para(json!([{"type": "text", "text": "a\tb"}]))]}]}
        ]});
        let b = analyze_block(&table, false);
        assert_eq!(b.label, "TBL2x2");
        assert_eq!(body_lines(&b, false, Ctx::Block).0, ["r1\tName\tOwner", "r2\tIntake\ta\\tb"]);
        let q = json!({"type": "blockquote", "content": [para(json!([{"type": "text", "text": "x"}])), para(json!([{"type": "text", "text": "y"}]))]});
        assert_eq!(body_lines(&analyze_block(&q, false), false, Ctx::Block).0, ["x\\ny"]);
    }

    #[test]
    fn preview_cuts_on_characters_and_atoms() {
        let c = para(json!([{"type": "text", "text": "héllo wörld\nnext"}]));
        let b = analyze_block(&c, false);
        assert_eq!(b.preview_inline().preview(5).0, "héllo…");
        assert_eq!(b.preview_inline().preview(11).0, "héllo wörld…");
        assert_eq!(b.preview_inline().preview(100).0, "héllo wörld\\nnext");
        assert_eq!(b.preview_inline().preview(0).0, "…");
    }

    #[test]
    fn unknown_blocks_are_x_and_rich() {
        let b = analyze_block(&json!({"type": "tableOfContents", "attrs": {"entries": []}}), false);
        assert_eq!(b.label, "X");
        assert!(b.features.rich);
    }
}
