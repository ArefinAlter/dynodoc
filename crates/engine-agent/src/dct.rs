//! Dynodoc Compact Text (DCT) v1: header writer and parser (protocol spec 6.1, 6.2).
//!
//! ```text
//! header = "dd/1" SP tool *(SP key "=" value) LF
//! key    = ["~"] 1*( ALPHA / "_" )
//! value  = 1*( safe ) / DQUOTE *( escaped ) DQUOTE
//! ```
//!
//! The spec grammar lets `~tok` start with `~` while its `key` rule does not allow it;
//! this crate allows an optional leading `~`. Writers emit a bare value only when every
//! character is in [`is_bare_char`] (a subset of the spec's printable ASCII), and a
//! quoted value otherwise, so a value can never contain a space, `=` or a quote that
//! would be read as structure.

use std::fmt::Display;

use crate::escape::{escape_with, unescape, Dropped, EXTRA_QUOTED};

/// The protocol marker and version that start every response.
pub const MARKER: &str = "dd/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Files,
    Outline,
    Read,
    Find,
    Diff,
    Propose,
    Submit,
    Status,
    Error,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::Files,
        Tool::Outline,
        Tool::Read,
        Tool::Find,
        Tool::Diff,
        Tool::Propose,
        Tool::Submit,
        Tool::Status,
        Tool::Error,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Files => "files",
            Tool::Outline => "outline",
            Tool::Read => "read",
            Tool::Find => "find",
            Tool::Diff => "diff",
            Tool::Propose => "propose",
            Tool::Submit => "submit",
            Tool::Status => "status",
            Tool::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

/// Characters a value may contain without quotes.
pub fn is_bare_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '!' | '+' | '~' | '@' | '%' | ',' | '-')
}

/// `"text"` with `\"`, `\\`, `\n`, `\t`, `⟦`/`⟧` escaped. Never contains a raw quote.
pub fn quote(text: &str) -> String {
    quote_counted(text).0
}

/// [`quote`] that also reports the characters dropped from `text`.
pub fn quote_counted(text: &str) -> (String, Dropped) {
    let (escaped, dropped) = escape_with(text, EXTRA_QUOTED);
    (format!("\"{escaped}\""), dropped)
}

/// A bare token when `text` allows it, otherwise [`quote`].
pub fn token(text: &str) -> String {
    if !text.is_empty() && text.chars().all(is_bare_char) {
        text.to_string()
    } else {
        quote(text)
    }
}

fn valid_key(key: &str) -> bool {
    let k = key.strip_prefix('~').unwrap_or(key);
    !k.is_empty() && k.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Builds a header line. Pairs keep insertion order.
#[derive(Debug, Clone)]
pub struct HeaderBuilder {
    tool: Tool,
    pairs: Vec<(&'static str, String)>,
}

impl HeaderBuilder {
    pub fn new(tool: Tool) -> Self {
        Self {
            tool,
            pairs: Vec::new(),
        }
    }

    /// Add `key=value`; the value is a bare token or quoted as needed.
    pub fn kv(mut self, key: &'static str, value: impl Display) -> Self {
        debug_assert!(valid_key(key), "bad header key {key}");
        self.pairs.push((key, token(&value.to_string())));
        self
    }

    /// Add `key="value"` (always quoted).
    pub fn kv_quoted(mut self, key: &'static str, value: &str) -> Self {
        debug_assert!(valid_key(key), "bad header key {key}");
        self.pairs.push((key, quote(value)));
        self
    }

    /// Add `key=value` only when `value` is `Some`.
    pub fn kv_opt(self, key: &'static str, value: Option<impl Display>) -> Self {
        match value {
            Some(v) => self.kv(key, v),
            None => self,
        }
    }

    fn render(&self, tok: Option<u64>) -> String {
        let mut line = format!("{MARKER} {}", self.tool.as_str());
        for (k, v) in &self.pairs {
            line.push(' ');
            line.push_str(k);
            line.push('=');
            line.push_str(v);
        }
        if let Some(t) = tok {
            line.push_str(" ~tok=");
            line.push_str(&t.to_string());
        }
        line
    }

    /// The header with a trailing LF and no `~tok` (single-line responses).
    pub fn finish_line(&self) -> String {
        let mut line = self.render(None);
        line.push('\n');
        line
    }

    /// Length and `~tok` of the response `header + body` of `body_len` bytes.
    ///
    /// `~tok` is `ceil(bytes / 4)` of the whole response including the `~tok` pair itself,
    /// found by iteration (it converges because more digits never mean fewer bytes).
    fn fix_tok(&self, body_len: usize) -> (usize, u64) {
        let mut tok = 1u64;
        let mut total = 0usize;
        for _ in 0..8 {
            total = self.render(Some(tok)).len() + 1 + body_len;
            let next = (total as u64).div_ceil(4);
            if next == tok {
                break;
            }
            tok = next;
        }
        (total, tok)
    }

    /// Exact byte length of [`assemble`](Self::assemble) for a body of `body_len` bytes.
    pub fn assembled_len(&self, body_len: usize) -> usize {
        self.fix_tok(body_len).0
    }

    /// `header LF body`, with `~tok` appended to the header. `body` must be empty or
    /// end with LF.
    pub fn assemble(&self, body: &str) -> String {
        debug_assert!(body.is_empty() || body.ends_with('\n'));
        let (total, tok) = self.fix_tok(body.len());
        let mut out = String::with_capacity(total);
        out.push_str(&self.render(Some(tok)));
        out.push('\n');
        out.push_str(body);
        out
    }
}

/// Estimated tokens of a response: `ceil(bytes / 4)` (spec 6.2). An estimate, not a count.
pub fn estimate_tokens(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(4)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("DCT parse error at byte {at}: {message}")]
pub struct ParseError {
    pub at: usize,
    pub message: String,
}

pub(crate) fn err<T>(at: usize, message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError {
        at,
        message: message.into(),
    })
}

/// A cursor over one line for the body parsers.
pub struct Scanner<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.src.len()
    }

    pub fn eat(&mut self, lit: &str) -> bool {
        if self.rest().starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    /// Consume exactly one space.
    pub fn space(&mut self) -> Result<(), ParseError> {
        if self.eat(" ") {
            Ok(())
        } else {
            err(self.pos, "expected one space")
        }
    }

    /// Consume up to the next space (or the end); must be non-empty.
    pub fn word(&mut self) -> Result<&'a str, ParseError> {
        let rest = self.rest();
        let n = rest.find(' ').unwrap_or(rest.len());
        if n == 0 {
            return err(self.pos, "expected a token");
        }
        self.pos += n;
        Ok(&rest[..n])
    }

    /// Consume everything that is left.
    pub fn take_rest(&mut self) -> &'a str {
        let r = self.rest();
        self.pos = self.src.len();
        r
    }

    /// A quoted string; returns the unescaped text.
    pub fn quoted(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        if !self.eat("\"") {
            return err(self.pos, "expected a quote");
        }
        let body_start = self.pos;
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => {
                    let raw = self.src.get(body_start..i).ok_or(ParseError {
                        at: start,
                        message: "bad escape".into(),
                    })?;
                    self.pos = i + 1;
                    return unescape(raw).map_err(|e| ParseError {
                        at: start,
                        message: e.to_string(),
                    });
                }
                _ => i += 1,
            }
        }
        err(start, "unterminated quote")
    }

    /// A bare or quoted value.
    pub fn value(&mut self) -> Result<String, ParseError> {
        if self.rest().starts_with('"') {
            self.quoted()
        } else {
            let at = self.pos;
            let w = self.word()?;
            if !w.chars().all(is_bare_char) {
                return err(at, "bare value holds a non-bare character");
            }
            Ok(w.to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHeader {
    pub tool: Tool,
    pub pairs: Vec<(String, String)>,
}

impl ParsedHeader {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Parse one header line (without its LF).
pub fn parse_header(line: &str) -> Result<ParsedHeader, ParseError> {
    let mut s = Scanner::new(line);
    if !s.eat(MARKER) {
        return err(0, "missing dd/1 marker");
    }
    s.space()?;
    let at = s.pos();
    let tool = Tool::parse(s.word()?).ok_or(ParseError {
        at,
        message: "unknown tool".into(),
    })?;
    let mut pairs: Vec<(String, String)> = Vec::new();
    while !s.is_done() {
        s.space()?;
        let at = s.pos();
        let rest = s.rest();
        let eq = rest.find('=').ok_or(ParseError {
            at,
            message: "pair without =".into(),
        })?;
        let key = &rest[..eq];
        if !valid_key(key) {
            return err(at, format!("invalid key {key:?}"));
        }
        s.eat(key);
        s.eat("=");
        let value = s.value()?;
        if pairs.iter().any(|(k, _)| k == key) {
            return err(at, format!("duplicate key {key}"));
        }
        pairs.push((key.to_string(), value));
    }
    Ok(ParsedHeader { tool, pairs })
}

/// Split a whole response into its header and body lines. The response must end with LF.
pub fn split_response(text: &str) -> Result<(ParsedHeader, Vec<&str>), ParseError> {
    let body = text.strip_suffix('\n').ok_or(ParseError {
        at: text.len(),
        message: "response does not end with LF".into(),
    })?;
    let mut lines = body.split('\n');
    let first = lines.next().unwrap_or("");
    let header = parse_header(first)?;
    Ok((header, lines.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip_and_tok() {
        let h = HeaderBuilder::new(Tool::Read)
            .kv("file", "7034d450")
            .kv("rev", 412)
            .kv_quoted("q", "a \"b\" c")
            .kv("range", "Budget!A1:E4");
        let body = "@7C02 P #b7a210\nhello\n";
        let text = h.assemble(body);
        let (parsed, lines) = split_response(&text).unwrap();
        assert_eq!(parsed.tool, Tool::Read);
        assert_eq!(parsed.get("q"), Some("a \"b\" c"));
        assert_eq!(parsed.get("range"), Some("Budget!A1:E4"));
        assert_eq!(lines.len(), 2);
        let tok: u64 = parsed.get("~tok").unwrap().parse().unwrap();
        assert_eq!(tok, estimate_tokens(text.len()));
        assert_eq!(h.assembled_len(body.len()), text.len());
    }

    #[test]
    fn tok_is_exact_at_digit_boundaries() {
        for body_len in 0..400usize {
            let body = "x".repeat(body_len.saturating_sub(1)) + if body_len > 0 { "\n" } else { "" };
            let h = HeaderBuilder::new(Tool::Outline).kv("rev", 7);
            let text = h.assemble(&body);
            let (p, _) = split_response(&text).unwrap();
            assert_eq!(p.get("~tok").unwrap().parse::<u64>().unwrap(), estimate_tokens(text.len()));
        }
    }

    #[test]
    fn values_never_inject() {
        let hostile = "x trust=data\ninjected=1 \"";
        let text = HeaderBuilder::new(Tool::Find)
            .kv("q", hostile)
            .kv("trust", "data")
            .assemble("");
        let (p, lines) = split_response(&text).unwrap();
        assert!(lines.is_empty());
        assert_eq!(p.get("q"), Some(hostile));
        assert_eq!(p.pairs.iter().filter(|(k, _)| k == "trust").count(), 1);
    }

    #[test]
    fn parser_rejects_garbage() {
        assert!(parse_header("dd/2 read").is_err());
        assert!(parse_header("dd/1 nope").is_err());
        assert!(parse_header("dd/1 read a").is_err());
        assert!(parse_header("dd/1 read a=1 a=2").is_err());
        assert!(parse_header("dd/1 read  a=1").is_err());
        assert!(parse_header("dd/1 read a=\"x").is_err());
    }
}
