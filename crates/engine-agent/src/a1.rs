//! Sheet addressing (protocol spec 5.5): column letters, A1 cells and sheet-qualified
//! ranges such as `Budget!A1:F40`.
//!
//! Rows are 1-based positions in `pos` order at the revision and columns are 0-based
//! indexes shown as letters (`A` is 0). Limits are the editor's: 100 columns, 3,000 rows.

use std::fmt;
use std::str::FromStr;

pub const MAX_COLS: u16 = 100;
pub const MAX_ROWS: u32 = 3_000;
/// Cells per range read (the editor's own fill limit).
pub const MAX_RANGE_CELLS: u64 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid A1 address: {0}")]
pub struct A1Error(pub &'static str);

/// Column letters of a 0-based column index: 0 is `A`, 26 is `AA`, 99 is `CV`.
pub fn column_name(col: u16) -> String {
    let mut n = u32::from(col) + 1;
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    s.reverse();
    String::from_utf8(s).expect("ASCII letters")
}

/// Column index of letters (case-insensitive), at most [`MAX_COLS`] columns.
pub fn column_index(letters: &str) -> Result<u16, A1Error> {
    if letters.is_empty() || letters.len() > 3 {
        return Err(A1Error("column letters must be 1 to 3 letters"));
    }
    let mut n: u32 = 0;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return Err(A1Error("column must be letters"));
        }
        n = n * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1;
    }
    let idx = n - 1;
    if idx >= u32::from(MAX_COLS) {
        return Err(A1Error("column beyond the 100-column limit"));
    }
    Ok(idx as u16)
}

/// One cell: 0-based column, 1-based row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellRef {
    pub col: u16,
    pub row: u32,
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", column_name(self.col), self.row)
    }
}

impl FromStr for CellRef {
    type Err = A1Error;

    /// `B7`, `b7`, `$B$7`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s: String = s.chars().filter(|&c| c != '$').collect();
        let split = s
            .find(|c: char| !c.is_ascii_alphabetic())
            .ok_or(A1Error("missing row number"))?;
        let (letters, digits) = s.split_at(split);
        let col = column_index(letters)?;
        if digits.is_empty()
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || digits.starts_with('0')
            || digits.len() > 4
        {
            return Err(A1Error("row must be a positive number without leading zeros"));
        }
        let row: u32 = digits.parse().map_err(|_| A1Error("bad row"))?;
        if row > MAX_ROWS {
            return Err(A1Error("row beyond the 3,000-row limit"));
        }
        Ok(CellRef { col, row })
    }
}

/// A range with an optional sheet qualifier. `from` is the top-left, `to` the
/// bottom-right cell after normalisation. The qualifier is the sheet's name or its ref,
/// exactly as written (the reader decides which it is).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A1Range {
    pub sheet: Option<String>,
    pub from: CellRef,
    pub to: CellRef,
}

impl A1Range {
    pub fn rows(&self) -> u32 {
        self.to.row - self.from.row + 1
    }

    pub fn cols(&self) -> u32 {
        u32::from(self.to.col - self.from.col) + 1
    }

    pub fn cells(&self) -> u64 {
        u64::from(self.rows()) * u64::from(self.cols())
    }
}

fn bare_sheet_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

impl fmt::Display for A1Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.sheet {
            if bare_sheet_name(name) {
                write!(f, "{name}!")?;
            } else {
                write!(f, "'{}'!", name.replace('\'', "''"))?;
            }
        }
        if self.from == self.to {
            write!(f, "{}", self.from)
        } else {
            write!(f, "{}:{}", self.from, self.to)
        }
    }
}

impl FromStr for A1Range {
    type Err = A1Error;

    /// `A1`, `A1:F40`, `Budget!A1:F40`, `'My ''Q3'' sheet'!B2`. Absolute markers (`$`)
    /// are accepted and dropped; reversed corners are normalised.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (sheet, rest) = if let Some(quoted) = s.strip_prefix('\'') {
            let mut name = String::new();
            let mut chars = quoted.char_indices();
            let mut end = None;
            while let Some((i, c)) = chars.next() {
                if c == '\'' {
                    match quoted[i + 1..].chars().next() {
                        Some('\'') => {
                            name.push('\'');
                            chars.next();
                        }
                        _ => {
                            end = Some(i + 1);
                            break;
                        }
                    }
                } else {
                    name.push(c);
                }
            }
            let end = end.ok_or(A1Error("unterminated sheet name quote"))?;
            let rest = quoted[end..]
                .strip_prefix('!')
                .ok_or(A1Error("expected '!' after the sheet name"))?;
            (Some(name), rest)
        } else if let Some(i) = s.rfind('!') {
            let name = &s[..i];
            if name.contains('\'') {
                return Err(A1Error("quote a sheet name that holds an apostrophe"));
            }
            if name.is_empty() {
                return Err(A1Error("empty sheet name"));
            }
            (Some(name.to_string()), &s[i + 1..])
        } else {
            (None, s)
        };
        let (a, b) = match rest.split_once(':') {
            Some((a, b)) => (a.parse::<CellRef>()?, b.parse::<CellRef>()?),
            None => {
                let c = rest.parse::<CellRef>()?;
                (c, c)
            }
        };
        Ok(A1Range {
            sheet,
            from: CellRef {
                col: a.col.min(b.col),
                row: a.row.min(b.row),
            },
            to: CellRef {
                col: a.col.max(b.col),
                row: a.row.max(b.row),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(99), "CV");
        assert_eq!(column_index("cv"), Ok(99));
        assert!(column_index("CW").is_err());
        assert!(column_index("").is_err());
        for c in 0..MAX_COLS {
            assert_eq!(column_index(&column_name(c)), Ok(c));
        }
    }

    #[test]
    fn ranges() {
        let r: A1Range = "Budget!A1:F40".parse().unwrap();
        assert_eq!(r.sheet.as_deref(), Some("Budget"));
        assert_eq!((r.rows(), r.cols(), r.cells()), (40, 6, 240));
        assert_eq!(r.to_string(), "Budget!A1:F40");
        let r: A1Range = "$F$40:$a$1".parse().unwrap();
        assert_eq!(r.to_string(), "A1:F40");
        let r: A1Range = "'My ''Q3'' sheet'!B2".parse().unwrap();
        assert_eq!(r.sheet.as_deref(), Some("My 'Q3' sheet"));
        assert_eq!(r.to_string(), "'My ''Q3'' sheet'!B2");
        for bad in ["", "A0", "A3001", "CW1", "A1:", ":A1", "!A1", "'x!A1", "'x'A1", "A01", "1A", "A1:B2:C3", "it's!A1"] {
            assert!(bad.parse::<A1Range>().is_err(), "{bad}");
        }
    }
}
