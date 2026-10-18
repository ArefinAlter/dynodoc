//! Escaping and prompt-injection hygiene (protocol spec 6.5).
//!
//! Document text is untrusted. Everything taken from a document passes through this
//! module before it reaches a DCT line, and the result has these properties, which the
//! property tests assert for arbitrary input:
//!
//! * it contains no LF, no TAB and no control character, so it can never start a new
//!   line or a new TAB column by itself;
//! * `\` always starts an escape, so a document backslash cannot complete one;
//! * `⟦` and `⟧` are escaped, so a document cannot forge a server placeholder;
//! * with [`escape_line`], the first character of a line is never one a structure line
//!   starts with.
//!
//! Dropped or normalised input (not reversible, counted in `warn=`):
//!
//! * `ctrl`: every control character (Unicode `Cc`) except LF and TAB. This extends the
//!   spec's U+0000..U+001F to DEL and the C1 controls. U+0085 (NEL), U+2028 and U+2029
//!   are treated as line feeds instead of dropped.
//! * `hidden`: characters that render as nothing or reorder text, the Trojan-Source and
//!   tag-character injection vectors of AI-AGENT-SECURITY P3: U+200B, U+2060..U+2064,
//!   U+FEFF, the bidirectional embeddings, overrides and isolates (U+202A..U+202E,
//!   U+2066..U+2069) and the tag block U+E0000..U+E007F. U+200C and U+200D are kept
//!   because Bengali, Persian and emoji sequences need them, as are U+200E, U+200F and
//!   U+061C (directional marks used by legitimate right-to-left text).

/// Counts of dropped characters; accumulate across a response and report in `warn=`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dropped {
    pub ctrl: usize,
    pub hidden: usize,
}

impl Dropped {
    pub fn add(&mut self, other: Dropped) {
        self.ctrl += other.ctrl;
        self.hidden += other.hidden;
    }
}

/// Where escaped text will sit; decides which line-start characters need a `\`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ctx {
    /// A line of `read` or `outline` body text: `@` always, and `#`, `=`, `-` when they
    /// stand alone or are followed by a space.
    Block,
    /// A slide shape text line: [`Ctx::Block`] plus `.` (shape lines start with ` .`).
    Slide,
    /// A `diff detail=text` line: [`Ctx::Block`] plus `~`, `+`, `>` and `-` always.
    Diff,
}

/// Extra characters escaped with a backslash in some contexts.
pub const EXTRA_MARKDOWN: &str = "*_`~[]";
pub const EXTRA_DIFF: &str = "[]{}";
pub const EXTRA_QUOTED: &str = "\"";

fn is_hidden(c: char) -> bool {
    matches!(c,
        '\u{200B}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
        | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        | '\u{E0000}'..='\u{E007F}')
}

/// Escape `input` for the middle of a line. `extra` lists more characters to
/// backslash-escape (see the `EXTRA_*` constants).
pub fn escape_with(input: &str, extra: &str) -> (String, Dropped) {
    let mut out = String::with_capacity(input.len() + 8);
    let mut dropped = Dropped::default();
    for c in input.chars() {
        match c {
            '\n' | '\u{0085}' | '\u{2028}' | '\u{2029}' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '⟦' => out.push_str("\\⟦"),
            '⟧' => out.push_str("\\⟧"),
            c if c.is_control() => dropped.ctrl += 1,
            c if is_hidden(c) => dropped.hidden += 1,
            c if extra.contains(c) => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    (out, dropped)
}

/// [`escape_with`] without extra characters.
pub fn escape(input: &str) -> (String, Dropped) {
    escape_with(input, "")
}

/// The sanitised text that [`unescape`]`(`[`escape`]`(x))` returns.
pub fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\n' | '\u{0085}' | '\u{2028}' | '\u{2029}' => out.push('\n'),
            '\t' => out.push('\t'),
            c if c.is_control() || is_hidden(c) => {}
            c => out.push(c),
        }
    }
    out
}

fn needs_line_escape(escaped: &str, ctx: Ctx) -> bool {
    let mut chars = escaped.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let lone_or_space = matches!(chars.next(), None | Some(' '));
    match first {
        '@' => true,
        '#' | '=' => lone_or_space,
        '-' => lone_or_space || ctx == Ctx::Diff,
        '.' => ctx == Ctx::Slide,
        '~' | '+' | '>' => ctx == Ctx::Diff,
        _ => false,
    }
}

/// Prefix already-escaped text with `\` when its first character could begin a
/// structure line in `ctx`.
pub fn guard_line_start(mut escaped: String, ctx: Ctx) -> String {
    if needs_line_escape(&escaped, ctx) {
        escaped.insert(0, '\\');
    }
    escaped
}

/// Escape text that starts a line: [`escape_with`] plus a leading `\` when the first
/// character could begin a structure line in `ctx`.
pub fn escape_line(input: &str, ctx: Ctx, extra: &str) -> (String, Dropped) {
    let (text, dropped) = escape_with(input, extra);
    (guard_line_start(text, ctx), dropped)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnescapeError {
    #[error("dangling backslash at the end")]
    Dangling,
    #[error("invalid escape \\{0}")]
    Invalid(char),
    #[error("raw control character {0:?}")]
    Raw(char),
}

/// Inverse of [`escape_with`] and [`escape_line`] (any `extra` set).
///
/// Accepts `\n`, `\t`, and a backslash before any ASCII punctuation, `⟦` or `⟧`. Raw
/// LF, TAB and other control characters are errors: escaped text never contains them.
pub fn unescape(input: &str) -> Result<String, UnescapeError> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                None => return Err(UnescapeError::Dangling),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(n) if n.is_ascii_punctuation() || n == '⟦' || n == '⟧' => out.push(n),
                Some(n) => return Err(UnescapeError::Invalid(n)),
            }
        } else if c.is_control() {
            return Err(UnescapeError::Raw(c));
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_table() {
        assert_eq!(escape("a\\b").0, "a\\\\b");
        assert_eq!(escape("a\nb").0, "a\\nb");
        assert_eq!(escape("a\tb").0, "a\\tb");
        assert_eq!(escape("⟦x⟧").0, "\\⟦x\\⟧");
        let (t, d) = escape("a\u{0}b\u{1b}c");
        assert_eq!((t.as_str(), d.ctrl), ("abc", 2));
    }

    #[test]
    fn line_start() {
        let e = |s: &str, c| escape_line(s, c, "").0;
        assert_eq!(e("@AB12 P #000000", Ctx::Block), "\\@AB12 P #000000");
        assert_eq!(e("# x", Ctx::Block), "\\# x");
        assert_eq!(e("#x", Ctx::Block), "#x");
        assert_eq!(e("#", Ctx::Block), "\\#");
        assert_eq!(e("= x", Ctx::Block), "\\= x");
        assert_eq!(e("- x", Ctx::Block), "\\- x");
        assert_eq!(e("-5", Ctx::Block), "-5");
        assert_eq!(e(".NET", Ctx::Block), ".NET");
        assert_eq!(e(".A1B2 title", Ctx::Slide), "\\.A1B2 title");
        assert_eq!(e("~7C02", Ctx::Diff), "\\~7C02");
        assert_eq!(e("+9F1A", Ctx::Diff), "\\+9F1A");
        assert_eq!(e("-9F1A", Ctx::Diff), "\\-9F1A");
        assert_eq!(e(">9F1A", Ctx::Diff), "\\>9F1A");
        assert_eq!(e(">9F1A", Ctx::Block), ">9F1A");
        assert_eq!(e("\\@", Ctx::Block), "\\\\@");
    }

    #[test]
    fn hidden_and_separators() {
        let (t, d) = escape("a\u{200B}b\u{202E}c\u{E0041}d\u{200D}e");
        assert_eq!(t, "abcd\u{200D}e");
        assert_eq!(d.hidden, 3);
        assert_eq!(escape("a\u{2028}b\u{0085}c").0, "a\\nb\\nc");
    }

    #[test]
    fn unescape_rejects_malformed() {
        assert_eq!(unescape("a\\"), Err(UnescapeError::Dangling));
        assert_eq!(unescape("a\\q"), Err(UnescapeError::Invalid('q')));
        assert_eq!(unescape("a\nb"), Err(UnescapeError::Raw('\n')));
        assert_eq!(unescape("\\@a").unwrap(), "@a");
    }
}
