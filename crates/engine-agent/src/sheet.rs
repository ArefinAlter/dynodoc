//! Spreadsheets (protocol spec 5.5, 6.7): the sheet view, column profile and range grid.
//!
//! Everything here summarises the stored strings. Formulas are not evaluated
//! (`calc=none`): there is no server-side formula engine yet (spec section 3).

use std::collections::BTreeMap;

use engine_core::materializer::MaterializedNode;
use serde::Serialize;

use crate::a1::{column_name, MAX_COLS};
use crate::budget::Unit;
use crate::dct::quote_counted;
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::hash::{range_hash, Hash};
use crate::model::{field_string, Tree};
use crate::refs::{Ref, RefIndex, ResolveError};

/// One sheet: its `section` node, rows in order and the number of used columns.
pub struct SheetView<'a> {
    pub node: &'a MaterializedNode,
    pub name: String,
    pub rows: Vec<&'a MaterializedNode>,
    /// Highest non-empty `cell_N` index plus one (the stored `columns` field is the
    /// editor's grid width and is ignored).
    pub cols: usize,
}

impl<'a> SheetView<'a> {
    pub fn new(tree: &Tree<'a>, node: &'a MaterializedNode) -> Self {
        let rows: Vec<&MaterializedNode> = tree
            .children(&node.id)
            .iter()
            .copied()
            .filter(|n| n.node_type == "item")
            .collect();
        let mut cols = 0usize;
        for row in &rows {
            if let Some(obj) = row.current_fields.as_object() {
                for (k, v) in obj {
                    let Some(n) = k.strip_prefix("cell_").and_then(|n| n.parse::<usize>().ok()) else {
                        continue;
                    };
                    if n < usize::from(MAX_COLS) && !field_string(v).is_empty() {
                        cols = cols.max(n + 1);
                    }
                }
            }
        }
        Self {
            node,
            name: field_string(&node.current_fields["label"]),
            rows,
            cols,
        }
    }

    pub fn cell(&self, row0: usize, col0: usize) -> String {
        self.rows
            .get(row0)
            .map(|r| field_string(&r.current_fields[&format!("cell_{col0}")]))
            .unwrap_or_default()
    }

    /// The row's strings for the used columns.
    pub fn row_cells(&self, row0: usize) -> Vec<String> {
        (0..self.cols).map(|c| self.cell(row0, c)).collect()
    }

    pub fn grid(&self) -> Vec<Vec<String>> {
        (0..self.rows.len()).map(|r| self.row_cells(r)).collect()
    }

    /// Hash over every used cell (the `h` of a whole-sheet range).
    pub fn hash(&self) -> Hash {
        range_hash(&self.grid())
    }

    /// `(frozen rows, frozen columns)` from `sheet_layout`.
    pub fn freeze(&self) -> (u64, u64) {
        let l = &self.node.current_fields["sheet_layout"];
        (
            l["frozenRows"].as_u64().unwrap_or(0).min(10),
            l["frozenColumns"].as_u64().unwrap_or(0).min(5),
        )
    }

    pub fn merges(&self) -> usize {
        self.node.current_fields["sheet_layout"]["merges"]
            .as_array()
            .map_or(0, Vec::len)
    }
}

/// A sheet by name (exact) or by ref. A name wins over a ref spelled the same.
pub fn select_sheet<'a>(
    tree: &Tree<'a>,
    idx: &RefIndex,
    qualifier: &str,
    field: &'static str,
) -> Result<&'a MaterializedNode, AgentError> {
    let sheets = tree.sheets();
    let by_name: Vec<_> = sheets
        .iter()
        .copied()
        .filter(|s| field_string(&s.current_fields["label"]) == qualifier)
        .collect();
    match by_name.as_slice() {
        [one] => return Ok(one),
        [_, _, ..] => return Err(AgentError::BadRequest { field }),
        [] => {}
    }
    let r = Ref::parse(qualifier).map_err(|_| AgentError::BadRequest { field })?;
    match idx.resolve(&r) {
        Ok(id) => sheets
            .iter()
            .copied()
            .find(|s| s.id == id)
            .ok_or(AgentError::UnknownRef { refs: vec![r] }),
        Err(ResolveError::Unknown) => Err(AgentError::UnknownRef { refs: vec![r] }),
        Err(ResolveError::Ambiguous(c)) => Err(AgentError::AmbiguousRef { refs: c }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cat {
    Num,
    Text,
    Date,
    Bool,
    Formula,
}

impl Cat {
    fn name(self) -> &'static str {
        match self {
            Cat::Num => "num",
            Cat::Text => "text",
            Cat::Date => "date",
            Cat::Bool => "bool",
            Cat::Formula => "formula",
        }
    }
}

/// Plain decimal numbers only: optional sign, digits with an optional `.`, optional
/// exponent. No locale parsing, no thousands separators.
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty()
        || !t.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
        || !t.bytes().any(|b| b.is_ascii_digit())
        || t.starts_with(['e', 'E'])
    {
        return None;
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = &s[r];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

fn categorize(s: &str) -> Option<Cat> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else if t.starts_with('=') {
        Some(Cat::Formula)
    } else if parse_number(t).is_some() {
        Some(Cat::Num)
    } else if is_iso_date(t) {
        Some(Cat::Date)
    } else if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        Some(Cat::Bool)
    } else {
        Some(Cat::Text)
    }
}

/// Normalise a formula to the pattern of its column: references on the formula's own row
/// become `{r}`, other relative rows `{r+k}` or `{r-k}`, absolute rows stay; string
/// literals and quoted sheet names are untouched. Columns stay as letters.
pub fn normalize_formula(formula: &str, row: u32) -> String {
    let c: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0;
    while i < c.len() {
        if c[i] == '"' || c[i] == '\'' {
            let q = c[i];
            out.push(q);
            i += 1;
            while i < c.len() {
                out.push(c[i]);
                if c[i] == q {
                    if c.get(i + 1) == Some(&q) {
                        out.push(q);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let prev_blocks = i > 0 && (c[i - 1].is_alphanumeric() || matches!(c[i - 1], '_' | '.'));
        if !prev_blocks {
            if let Some((len, text)) = reference_at(&c, i, row) {
                out.push_str(&text);
                i += len;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// `$?[A-Z]{1,3}$?\d+` at `i`, not part of a longer name or call.
fn reference_at(c: &[char], i: usize, row: u32) -> Option<(usize, String)> {
    let mut j = i;
    let abs_col = c.get(j) == Some(&'$');
    if abs_col {
        j += 1;
    }
    let l0 = j;
    while c.get(j).is_some_and(char::is_ascii_alphabetic) {
        j += 1;
    }
    let letters: String = c[l0..j].iter().collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let abs_row = c.get(j) == Some(&'$');
    if abs_row {
        j += 1;
    }
    let d0 = j;
    while c.get(j).is_some_and(char::is_ascii_digit) {
        j += 1;
    }
    let digits: String = c[d0..j].iter().collect();
    if digits.is_empty() || digits.len() > 7 {
        return None;
    }
    if c.get(j).is_some_and(|n| n.is_alphanumeric() || matches!(n, '_' | '(' | '!' | '.' | '\'')) {
        return None;
    }
    let mut text = String::new();
    if abs_col {
        text.push('$');
    }
    text.push_str(&letters.to_ascii_uppercase());
    if abs_row {
        text.push('$');
        text.push_str(&digits);
    } else {
        let n: i64 = digits.parse().ok()?;
        match n - i64::from(row) {
            0 => text.push_str("{r}"),
            d if d > 0 => text.push_str(&format!("{{r+{d}}}")),
            d => text.push_str(&format!("{{r-{}}}", -d)),
        }
    }
    Some((j - i, text))
}

/// Numbers as the profile prints them: integers without a decimal point, otherwise up to
/// ten decimals with trailing zeros trimmed.
pub fn format_number(x: f64) -> String {
    if x == 0.0 {
        "0".into()
    } else if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{x:.0}")
    } else if x.abs() >= 1e15 || x.abs() < 1e-9 {
        format!("{x:e}")
    } else {
        let s = format!("{x:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColumnProfile {
    pub col: String,
    pub header: String,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub n: usize,
    pub blank: usize,
    /// Distinct non-empty strings; `None` for numeric and formula columns.
    pub distinct: Option<usize>,
    /// More than 1,000 distinct values (counting stopped).
    pub distinct_over: bool,
    /// Up to 5 most frequent values with counts, only when `distinct <= 20`.
    pub top: Vec<(String, usize)>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub sum: Option<f64>,
    pub pattern: Option<String>,
    /// Row numbers (first 10) whose cell does not follow the pattern.
    pub exceptions: Vec<u32>,
    pub exceptions_more: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    /// `"1"` (frozen row), `"guess"` (inferred: row 1) or `"0"` (none).
    pub header: String,
    pub freeze: (u64, u64),
    pub merges: usize,
    pub columns: Vec<ColumnProfile>,
}

const DISTINCT_CAP: usize = 1_000;
const TOP_MAX_DISTINCT: usize = 20;
const EXCEPTIONS_MAX: usize = 10;

/// Column statistics of one sheet (spec 6.7).
pub fn profile(view: &SheetView) -> Profile {
    let grid = view.grid();
    let rows = grid.len();
    let (frozen, frozen_cols) = view.freeze();
    let all_text = |r: &Vec<String>| {
        let cats: Vec<Cat> = r.iter().filter_map(|s| categorize(s)).collect();
        !cats.is_empty() && cats.iter().all(|c| *c == Cat::Text)
    };
    let (header_row, header) = if frozen >= 1 && frozen as usize <= rows {
        (frozen as usize, frozen.to_string())
    } else if rows >= 2 && all_text(&grid[0]) && !all_text(&grid[1]) && grid[1].iter().any(|s| !s.trim().is_empty()) {
        (1, "guess".to_string())
    } else {
        (0, "0".to_string())
    };
    let mut columns = Vec::with_capacity(view.cols);
    for c in 0..view.cols {
        let mut cats: Vec<Cat> = Vec::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut over = false;
        let (mut n, mut nums, mut sum) = (0usize, 0usize, 0.0f64);
        let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut formulas: Vec<(u32, String)> = Vec::new();
        let mut literals: Vec<u32> = Vec::new();
        for (r, row) in grid.iter().enumerate().skip(header_row) {
            let s = &row[c];
            let Some(cat) = categorize(s) else { continue };
            n += 1;
            if !cats.contains(&cat) {
                cats.push(cat);
            }
            let row_no = r as u32 + 1;
            match cat {
                Cat::Num => {
                    let x = parse_number(s).unwrap_or(0.0);
                    nums += 1;
                    sum += x;
                    min = min.min(x);
                    max = max.max(x);
                    literals.push(row_no);
                }
                Cat::Formula => formulas.push((row_no, normalize_formula(s.trim(), row_no))),
                _ => literals.push(row_no),
            }
            if !over {
                *counts.entry(s.trim().to_string()).or_insert(0) += 1;
                if counts.len() > DISTINCT_CAP {
                    over = true;
                }
            }
        }
        let ty = match cats.as_slice() {
            [] => "text",
            [one] => one.name(),
            _ => "mixed",
        };
        let textual = matches!(ty, "text" | "date" | "bool" | "mixed");
        let distinct = (textual && !over).then_some(counts.len());
        let mut top = Vec::new();
        if textual && !over && counts.len() <= TOP_MAX_DISTINCT {
            let mut all: Vec<(String, usize)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
            all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            top = all.into_iter().take(5).collect();
        }
        let (mut pattern, mut exceptions, mut more) = (None, Vec::new(), 0);
        if let Some((_, first)) = formulas.first() {
            let pat = first.clone();
            let mut bad: Vec<u32> = formulas
                .iter()
                .filter(|(_, f)| *f != pat)
                .map(|(r, _)| *r)
                .chain(literals.iter().copied())
                .collect();
            bad.sort_unstable();
            more = bad.len().saturating_sub(EXCEPTIONS_MAX);
            bad.truncate(EXCEPTIONS_MAX);
            exceptions = bad;
            pattern = Some(pat);
        }
        columns.push(ColumnProfile {
            col: column_name(c as u16),
            header: if header_row > 0 { grid[header_row - 1][c].clone() } else { String::new() },
            ty,
            n,
            blank: (rows - header_row).saturating_sub(n),
            distinct,
            distinct_over: over,
            top,
            min: (nums > 0 && matches!(ty, "num" | "mixed")).then_some(min),
            max: (nums > 0 && matches!(ty, "num" | "mixed")).then_some(max),
            sum: (nums > 0 && matches!(ty, "num" | "mixed")).then_some(sum),
            pattern,
            exceptions,
            exceptions_more: more,
        });
    }
    Profile {
        name: view.name.clone(),
        rows,
        cols: view.cols,
        header,
        freeze: (frozen, frozen_cols),
        merges: view.merges(),
        columns,
    }
}

/// One profile line: `A "Region" text n=479 distinct=4 top="North":131,...`.
pub fn profile_unit(col: &ColumnProfile) -> Unit {
    let mut dropped = Dropped::default();
    let mut q = |s: &str| {
        let (t, d) = quote_counted(s);
        dropped.add(d);
        t
    };
    let mut line = format!("{} {} {} n={}", col.col, q(&col.header), col.ty, col.n);
    if col.blank > 0 {
        line.push_str(&format!(" blank={}", col.blank));
    }
    if col.distinct_over {
        line.push_str(" distinct=>1000");
    } else if let Some(d) = col.distinct {
        line.push_str(&format!(" distinct={d}"));
    }
    if !col.top.is_empty() {
        let top: Vec<String> = col.top.iter().map(|(v, n)| format!("{}:{n}", q(v))).collect();
        line.push_str(&format!(" top={}", top.join(",")));
    }
    if let (Some(min), Some(max), Some(sum)) = (col.min, col.max, col.sum) {
        line.push_str(&format!(" min={} max={} sum={}", format_number(min), format_number(max), format_number(sum)));
    }
    if let Some(p) = &col.pattern {
        let ex = if col.exceptions.is_empty() {
            "-".to_string()
        } else {
            let mut e: Vec<String> = col.exceptions.iter().map(|r| format!("r{r}")).collect();
            if col.exceptions_more > 0 {
                e.push(format!("+{}", col.exceptions_more));
            }
            e.join(",")
        };
        line.push_str(&format!(" pattern={} exceptions={ex}", q(p)));
    }
    line.push('\n');
    Unit {
        text: line,
        dropped,
        ids: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_dates() {
        for ok in ["12", "-3.5", "+.5", "1e3", "5.", " 7 "] {
            assert!(parse_number(ok).is_some(), "{ok}");
        }
        for bad in ["", "e5", "1,234", "inf", "nan", "0x10", "1-2", "--1", "."] {
            assert!(parse_number(bad).is_none(), "{bad}");
        }
        assert!(is_iso_date("2024-02-29"));
        assert!(!is_iso_date("2023-02-29"));
        assert!(!is_iso_date("2024-13-01"));
        assert!(!is_iso_date("2024-1-01"));
        assert_eq!(format_number(22127388.0), "22127388");
        assert_eq!(format_number(0.30000000000000004), "0.3");
        assert_eq!(format_number(-1.5), "-1.5");
    }

    #[test]
    fn formula_patterns() {
        assert_eq!(normalize_formula("=SUM(D2:G2)", 2), "=SUM(D{r}:G{r})");
        assert_eq!(normalize_formula("=A2*B3-C1", 2), "=A{r}*B{r+1}-C{r-1}");
        assert_eq!(normalize_formula("=$A$1+B2", 2), "=$A$1+B{r}");
        assert_eq!(normalize_formula("=LOG10(A2)", 2), "=LOG10(A{r})");
        assert_eq!(normalize_formula("=\"A1\"&A2", 2), "=\"A1\"&A{r}");
        assert_eq!(normalize_formula("='My Sheet'!A2+Other!B2", 2), "='My Sheet'!A{r}+Other!B{r}");
        assert_eq!(normalize_formula("=SUM(D2:G2)", 5), "=SUM(D{r-3}:G{r-3})");
    }
}
