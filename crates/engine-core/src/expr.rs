//! A hand-rolled recursive-descent parser for the ODK/XLSForm expression subset
//! (docs/21 item 1).
//!
//! Items carry `relevance`, `constraint`, and `calculation` expressions written in
//! the ODK dialect: `${var}` references, comparison/arithmetic/boolean operators, and
//! function calls such as `selected(${q1}, 'yes')`. The stage-16 integrity pass only
//! needed the *set* of referenced variables and scanned for `${var}` with a byte loop
//! ([`crate::integrity::extract_var_refs`]). The stage-21 checks (type-mismatch and
//! choice-reference) need the *shape* of the expression — which operator is applied to
//! which operand, and the literal arguments of a `selected()` call — so we parse to an
//! AST.
//!
//! We deliberately **do not evaluate**. The PoC needs only reference extraction and a
//! conservative structural inspection, so the parser is permissive: any identifier is
//! accepted as a function name and arity is never checked. It is hand-rolled (no `nom`
//! / `winnow`) to keep the dependency surface small, matching the existing hand-rolled
//! `extract_var_refs`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A literal value appearing in an expression. Kept separate from `serde_json::Value`
/// so the AST can derive `PartialEq` without JSON's number/equality quirks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
}

/// Binary operators in the supported subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
}

impl BinOp {
    /// Whether this operator is arithmetic (`+ - * / mod div`).
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
        )
    }

    /// Whether this operator is an *ordering* comparison (`< <= > >=`). Equality
    /// (`= !=`) is excluded — it is valid on operands of any type.
    pub fn is_ordering(self) -> bool {
        matches!(self, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
}

/// Unary operators in the supported subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
}

/// An expression AST node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// `${var_name}` — a reference to another node's `var_name`.
    VarRef(String),
    /// A string / numeric / boolean literal.
    Literal(Value),
    /// A binary operation `lhs op rhs`.
    BinOp(BinOp, Box<Expr>, Box<Expr>),
    /// A unary operation `op operand`.
    UnaryOp(UnaryOp, Box<Expr>),
    /// A function call `name(arg, …)`. Any identifier is accepted as a name.
    Call(String, Vec<Expr>),
    /// The `.` token — ODK's "current field value" used in constraints.
    Dot,
}

/// A typed parse error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("unexpected character {ch:?} at byte {pos}")]
    UnexpectedChar { ch: char, pos: usize },
    #[error("expected {expected} at byte {pos}")]
    Expected { expected: String, pos: usize },
    #[error("unterminated string literal starting at byte {pos}")]
    UnterminatedString { pos: usize },
    #[error("unterminated variable reference starting at byte {pos}")]
    UnterminatedVarRef { pos: usize },
    #[error("trailing input at byte {pos}")]
    TrailingInput { pos: usize },
}

/// Parse an ODK expression string into an [`Expr`] AST.
///
/// Operator precedence (lowest → highest): `or`, `and`, comparison (`= != < <= > >=`),
/// additive (`+ -`), multiplicative (`* / mod div`), unary (`not -`), primary
/// (literals, `${var}`, `.`, calls, parens).
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let mut p = Parser::new(input);
    p.skip_ws();
    let expr = p.parse_or()?;
    p.skip_ws();
    if p.pos < p.len() {
        return Err(ParseError::TrailingInput { pos: p.pos });
    }
    Ok(expr)
}

/// Walk an AST and collect every referenced `var_name` from `VarRef` nodes.
pub fn referenced_vars(expr: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_vars(expr, &mut out);
    out
}

fn collect_vars(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::VarRef(name) => {
            out.insert(name.clone());
        }
        Expr::Literal(_) | Expr::Dot => {}
        Expr::BinOp(_, a, b) => {
            collect_vars(a, out);
            collect_vars(b, out);
        }
        Expr::UnaryOp(_, a) => collect_vars(a, out),
        Expr::Call(_, args) => {
            for arg in args {
                collect_vars(arg, out);
            }
        }
    }
}

/// Extract `(var_name, choice_literal)` pairs from every `selected(...)` call in the
/// AST. ODK allows either argument order — `selected(${q}, 'lit')` and
/// `selected('lit', ${q})` — so both are recognised. Only calls with exactly one
/// `${var}` argument and one string-literal argument contribute a pair.
pub fn selected_choice_refs(expr: &Expr) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_selected(expr, &mut out);
    out
}

fn collect_selected(expr: &Expr, out: &mut Vec<(String, String)>) {
    match expr {
        Expr::VarRef(_) | Expr::Literal(_) | Expr::Dot => {}
        Expr::BinOp(_, a, b) => {
            collect_selected(a, out);
            collect_selected(b, out);
        }
        Expr::UnaryOp(_, a) => collect_selected(a, out),
        Expr::Call(name, args) => {
            if name == "selected" && args.len() == 2 {
                let var = args.iter().find_map(|a| match a {
                    Expr::VarRef(v) => Some(v.clone()),
                    _ => None,
                });
                let lit = args.iter().find_map(|a| match a {
                    Expr::Literal(Value::Str(s)) => Some(s.clone()),
                    _ => None,
                });
                if let (Some(var), Some(lit)) = (var, lit) {
                    out.push((var, lit));
                }
            }
            for arg in args {
                collect_selected(arg, out);
            }
        }
    }
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// If the remaining input starts with `word` as a whole keyword (not followed by
    /// an identifier character), consume it and return true.
    fn eat_keyword(&mut self, word: &str) -> bool {
        let end = self.pos + word.len();
        if end <= self.len()
            && &self.input[self.pos..end] == word
            && !self.bytes.get(end).copied().is_some_and(is_ident_continue)
        {
            self.pos = end;
            true
        } else {
            false
        }
    }

    /// If the remaining input starts with `sym`, consume it and return true.
    fn eat_symbol(&mut self, sym: &str) -> bool {
        let end = self.pos + sym.len();
        if end <= self.len() && &self.input[self.pos..end] == sym {
            self.pos = end;
            true
        } else {
            false
        }
    }

    // or := and ( 'or' and )*
    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.eat_keyword("or") {
                self.skip_ws();
                let rhs = self.parse_and()?;
                lhs = Expr::BinOp(BinOp::Or, Box::new(lhs), Box::new(rhs));
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    // and := comparison ( 'and' comparison )*
    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_comparison()?;
        loop {
            self.skip_ws();
            if self.eat_keyword("and") {
                self.skip_ws();
                let rhs = self.parse_comparison()?;
                lhs = Expr::BinOp(BinOp::And, Box::new(lhs), Box::new(rhs));
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    // comparison := additive ( ( '=' | '!=' | '<=' | '>=' | '<' | '>' ) additive )*
    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_additive()?;
        loop {
            self.skip_ws();
            // Order matters: try two-char operators before single-char ones.
            let op = if self.eat_symbol("!=") {
                Some(BinOp::Ne)
            } else if self.eat_symbol("<=") {
                Some(BinOp::Le)
            } else if self.eat_symbol(">=") {
                Some(BinOp::Ge)
            } else if self.eat_symbol("=") {
                Some(BinOp::Eq)
            } else if self.eat_symbol("<") {
                Some(BinOp::Lt)
            } else if self.eat_symbol(">") {
                Some(BinOp::Gt)
            } else {
                None
            };
            match op {
                Some(op) => {
                    self.skip_ws();
                    let rhs = self.parse_additive()?;
                    lhs = Expr::BinOp(op, Box::new(lhs), Box::new(rhs));
                }
                None => break,
            }
        }
        Ok(lhs)
    }

    // additive := multiplicative ( ( '+' | '-' ) multiplicative )*
    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            self.skip_ws();
            let op = if self.eat_symbol("+") {
                Some(BinOp::Add)
            } else if self.eat_symbol("-") {
                Some(BinOp::Sub)
            } else {
                None
            };
            match op {
                Some(op) => {
                    self.skip_ws();
                    let rhs = self.parse_multiplicative()?;
                    lhs = Expr::BinOp(op, Box::new(lhs), Box::new(rhs));
                }
                None => break,
            }
        }
        Ok(lhs)
    }

    // multiplicative := unary ( ( '*' | 'div' | 'mod' ) unary )*
    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            self.skip_ws();
            // ODK uses `div` and `mod` keywords; `/` is also accepted as division.
            let op = if self.eat_symbol("*") {
                Some(BinOp::Mul)
            } else if self.eat_keyword("div") {
                Some(BinOp::Div)
            } else if self.eat_keyword("mod") {
                Some(BinOp::Mod)
            } else if self.eat_symbol("/") {
                Some(BinOp::Div)
            } else {
                None
            };
            match op {
                Some(op) => {
                    self.skip_ws();
                    let rhs = self.parse_unary()?;
                    lhs = Expr::BinOp(op, Box::new(lhs), Box::new(rhs));
                }
                None => break,
            }
        }
        Ok(lhs)
    }

    // unary := ( 'not' | '-' ) unary | primary
    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        if self.eat_keyword("not") {
            self.skip_ws();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(operand)));
        }
        if self.peek() == Some(b'-') {
            self.pos += 1;
            self.skip_ws();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(operand)));
        }
        self.parse_primary()
    }

    // primary := number | string | '${' var '}' | '.' | '(' or ')' | call
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_ws();
        let b = self.peek().ok_or(ParseError::UnexpectedEof)?;

        // Parenthesised sub-expression.
        if b == b'(' {
            self.pos += 1;
            self.skip_ws();
            let inner = self.parse_or()?;
            self.skip_ws();
            if !self.eat_symbol(")") {
                return Err(ParseError::Expected {
                    expected: ")".to_string(),
                    pos: self.pos,
                });
            }
            return Ok(inner);
        }

        // Variable reference `${name}`.
        if b == b'$' && self.bytes.get(self.pos + 1) == Some(&b'{') {
            let start = self.pos;
            self.pos += 2;
            let name_start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'}' {
                    break;
                }
                self.pos += 1;
            }
            if self.peek() != Some(b'}') {
                return Err(ParseError::UnterminatedVarRef { pos: start });
            }
            let name = self.input[name_start..self.pos].trim().to_string();
            self.pos += 1; // consume '}'
            return Ok(Expr::VarRef(name));
        }

        // String literal.
        if b == b'\'' || b == b'"' {
            return self.parse_string(b);
        }

        // Numeric literal.
        if b.is_ascii_digit() || (b == b'.' && self.next_is_digit()) {
            return self.parse_number();
        }

        // Bare `.` token (current field value).
        if b == b'.' {
            self.pos += 1;
            return Ok(Expr::Dot);
        }

        // Identifier: a function call, or `true`/`false` literal.
        if is_ident_start(b) {
            return self.parse_ident_or_call();
        }

        Err(ParseError::UnexpectedChar {
            ch: b as char,
            pos: self.pos,
        })
    }

    fn next_is_digit(&self) -> bool {
        self.bytes
            .get(self.pos + 1)
            .copied()
            .is_some_and(|c| c.is_ascii_digit())
    }

    fn parse_string(&mut self, quote: u8) -> Result<Expr, ParseError> {
        let start = self.pos;
        self.pos += 1; // opening quote
        let content_start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let s = self.input[content_start..self.pos].to_string();
                self.pos += 1; // closing quote
                return Ok(Expr::Literal(Value::Str(s)));
            }
            self.pos += 1;
        }
        Err(ParseError::UnterminatedString { pos: start })
    }

    fn parse_number(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.input[start..self.pos];
        let value: f64 = text.parse().map_err(|_| ParseError::Expected {
            expected: "number".to_string(),
            pos: start,
        })?;
        Ok(Expr::Literal(Value::Number(value)))
    }

    fn parse_ident_or_call(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name = self.input[start..self.pos].to_string();

        // Boolean literals.
        match name.as_str() {
            "true" => return Ok(Expr::Literal(Value::Bool(true))),
            "false" => return Ok(Expr::Literal(Value::Bool(false))),
            _ => {}
        }

        // Function call if an opening paren follows (allowing whitespace).
        let save = self.pos;
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.pos += 1;
            let args = self.parse_args()?;
            return Ok(Expr::Call(name, args));
        }
        self.pos = save;

        // A bare identifier with no call/`${}` is not part of the supported subset;
        // treat it as an error rather than silently accepting it.
        Err(ParseError::UnexpectedChar {
            ch: self.input[start..].chars().next().unwrap_or(' '),
            pos: start,
        })
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_ws();
        if self.eat_symbol(")") {
            return Ok(args);
        }
        loop {
            self.skip_ws();
            let arg = self.parse_or()?;
            args.push(arg);
            self.skip_ws();
            if self.eat_symbol(",") {
                continue;
            }
            if self.eat_symbol(")") {
                break;
            }
            return Err(ParseError::Expected {
                expected: ", or )".to_string(),
                pos: self.pos,
            });
        }
        Ok(args)
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(input: &str) -> Vec<String> {
        referenced_vars(&parse(input).unwrap())
            .into_iter()
            .collect()
    }

    #[test]
    fn parses_var_ref() {
        assert_eq!(parse("${age}").unwrap(), Expr::VarRef("age".into()));
        assert_eq!(vars("${age}"), vec!["age".to_string()]);
    }

    #[test]
    fn parses_numeric_comparison() {
        let expr = parse("${age} >= 18").unwrap();
        assert_eq!(
            expr,
            Expr::BinOp(
                BinOp::Ge,
                Box::new(Expr::VarRef("age".into())),
                Box::new(Expr::Literal(Value::Number(18.0)))
            )
        );
    }

    #[test]
    fn precedence_and_binds_tighter_than_or() {
        // a or b and c  ==  a or (b and c)
        let expr = parse("${a} = 1 or ${b} = 1 and ${c} = 1").unwrap();
        match expr {
            Expr::BinOp(BinOp::Or, _, rhs) => {
                assert!(matches!(*rhs, Expr::BinOp(BinOp::And, _, _)));
            }
            other => panic!("expected top-level Or, got {other:?}"),
        }
    }

    #[test]
    fn arithmetic_precedence() {
        // 1 + 2 * 3  ==  1 + (2 * 3)
        let expr = parse("1 + 2 * 3").unwrap();
        match expr {
            Expr::BinOp(BinOp::Add, _, rhs) => {
                assert!(matches!(*rhs, Expr::BinOp(BinOp::Mul, _, _)));
            }
            other => panic!("expected top-level Add, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_calls_and_collects_refs() {
        let expr = parse("${a} > 5 and selected(${b}, 'y')").unwrap();
        let refs: Vec<String> = referenced_vars(&expr).into_iter().collect();
        assert_eq!(refs, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            selected_choice_refs(&expr),
            vec![("b".to_string(), "y".to_string())]
        );
    }

    #[test]
    fn selected_handles_reversed_arg_order() {
        let expr = parse("selected('yes', ${q})").unwrap();
        assert_eq!(
            selected_choice_refs(&expr),
            vec![("q".to_string(), "yes".to_string())]
        );
    }

    #[test]
    fn parses_count_and_function_with_many_args() {
        let expr = parse("count-selected(${q}) > 2").unwrap();
        let refs: Vec<String> = referenced_vars(&expr).into_iter().collect();
        assert_eq!(refs, vec!["q".to_string()]);
    }

    #[test]
    fn parses_dot_token() {
        assert_eq!(
            parse(". > 5").unwrap(),
            Expr::BinOp(
                BinOp::Gt,
                Box::new(Expr::Dot),
                Box::new(Expr::Literal(Value::Number(5.0)))
            )
        );
    }

    #[test]
    fn parses_not_and_parens() {
        let expr = parse("not(${a} = 1)").unwrap();
        assert!(matches!(expr, Expr::UnaryOp(UnaryOp::Not, _)));
    }

    #[test]
    fn parses_string_literals_both_quotes() {
        assert_eq!(parse("'a'").unwrap(), Expr::Literal(Value::Str("a".into())));
        assert_eq!(
            parse("\"b\"").unwrap(),
            Expr::Literal(Value::Str("b".into()))
        );
    }

    #[test]
    fn parses_mod_and_div_keywords() {
        assert!(matches!(
            parse("${a} mod 2").unwrap(),
            Expr::BinOp(BinOp::Mod, _, _)
        ));
        assert!(matches!(
            parse("${a} div 2").unwrap(),
            Expr::BinOp(BinOp::Div, _, _)
        ));
    }

    #[test]
    fn malformed_input_errors_not_panics() {
        assert!(parse("${a} >").is_err());
        assert!(parse("${a").is_err());
        assert!(parse("'unterminated").is_err());
        assert!(parse("selected(${a}, )").is_err());
        assert!(parse("@").is_err());
        assert!(parse("${a} = 1 garbage").is_err());
        assert!(parse("").is_err());
    }
}
