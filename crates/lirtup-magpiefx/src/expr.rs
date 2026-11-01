//! Size expressions of `//!WIDTH` / `//!HEIGHT` (`expr.py`):
//! `INPUT_WIDTH * 2`, `OUTPUT_HEIGHT / 4`, with `+ - * / ( )`, comparisons
//! and `?:`.
//!
//! Magpie evaluates them with muparser; the effects in the wild only use
//! constants with `+ - * /` and parentheses, but the common muparser
//! operators and functions work too. Grammar, lowest to highest precedence:
//!
//! ```text
//! expr  := cmp ('?' expr ':' expr)?
//! cmp   := sum (('<' | '>' | '<=' | '>=' | '==' | '!=') sum)*
//! sum   := term (('+' | '-') term)*
//! term  := unary (('*' | '/' | '%') unary)*
//! unary := ('+' | '-') unary | power
//! power := atom ('^' unary)?
//! atom  := NUMBER | NAME | NAME '(' args ')' | '(' expr ')'
//! ```

use std::collections::BTreeMap;

use crate::{MagpieError, Result};

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Name(String),
    Op(&'static str),
    End,
}

const OPS: [&str; 17] = ["<=", ">=", "==", "!=", "-", "+", "*", "/", "%", "^", "(", ")", "?", ":", ",", "<", ">"];

fn error(text: &str, message: String) -> MagpieError {
    MagpieError::Parse { effect: String::new(), message: format!("{message} in expression '{text}'") }
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            // exponent, only if digits follow
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_digit() {
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            let value = text[start..i].parse::<f64>().map_err(|_| error(text, format!("Bad number at {start}")))?;
            tokens.push(Token::Num(value));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            tokens.push(Token::Name(text[start..i].to_owned()));
            continue;
        }
        match OPS.iter().find(|op| text[i..].starts_with(*op)) {
            Some(op) => {
                tokens.push(Token::Op(op));
                i += op.len();
            }
            None => return Err(error(text, format!("Bad character at {i}"))),
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    vars: &'a BTreeMap<String, f64>,
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn accept(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Token::Op(o) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<()> {
        if self.accept(op) { Ok(()) } else { Err(error(self.text, format!("Expected '{op}'"))) }
    }

    fn parse(&mut self) -> Result<f64> {
        let value = self.expr()?;
        if *self.peek() != Token::End {
            return Err(error(self.text, "Trailing tokens".to_owned()));
        }
        Ok(value)
    }

    fn expr(&mut self) -> Result<f64> {
        let cond = self.cmp()?;
        if self.accept("?") {
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.expr()?;
            return Ok(if cond != 0.0 { a } else { b });
        }
        Ok(cond)
    }

    fn cmp(&mut self) -> Result<f64> {
        let mut value = self.sum()?;
        loop {
            let op = match self.peek() {
                Token::Op(op @ ("<" | ">" | "<=" | ">=" | "==" | "!=")) => *op,
                _ => return Ok(value),
            };
            self.pos += 1;
            let rhs = self.sum()?;
            let result = match op {
                "<" => value < rhs,
                ">" => value > rhs,
                "<=" => value <= rhs,
                ">=" => value >= rhs,
                "==" => value == rhs,
                _ => value != rhs,
            };
            value = if result { 1.0 } else { 0.0 };
        }
    }

    fn sum(&mut self) -> Result<f64> {
        let mut value = self.term()?;
        loop {
            if self.accept("+") {
                value += self.term()?;
            } else if self.accept("-") {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64> {
        let mut value = self.unary()?;
        loop {
            if self.accept("*") {
                value *= self.unary()?;
            } else if self.accept("/") {
                let d = self.unary()?;
                if d == 0.0 {
                    return Err(error(self.text, "Division by zero".to_owned()));
                }
                value /= d;
            } else if self.accept("%") {
                value %= self.unary()?; // fmod
            } else {
                return Ok(value);
            }
        }
    }

    fn unary(&mut self) -> Result<f64> {
        if self.accept("-") {
            return Ok(-self.unary()?);
        }
        if self.accept("+") {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<f64> {
        let base = self.atom()?;
        if self.accept("^") {
            return Ok(base.powf(self.unary()?));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<f64> {
        let token = self.peek().clone();
        self.pos += 1;
        match token {
            Token::Num(v) => Ok(v),
            Token::Name(name) => {
                if self.accept("(") {
                    let mut args = vec![self.expr()?];
                    while self.accept(",") {
                        args.push(self.expr()?);
                    }
                    self.expect(")")?;
                    return call(self.text, &name, &args);
                }
                self.vars.get(&name).copied().ok_or_else(|| error(self.text, format!("Unknown constant '{name}'")))
            }
            Token::Op("(") => {
                let result = self.expr()?;
                self.expect(")")?;
                Ok(result)
            }
            Token::Op(op) => Err(error(self.text, format!("Unexpected token '{op}'"))),
            Token::End => Err(error(self.text, "Unexpected end".to_owned())),
        }
    }
}

fn call(text: &str, name: &str, args: &[f64]) -> Result<f64> {
    let one = || {
        if args.len() == 1 { Ok(args[0]) } else { Err(error(text, format!("{name}() takes one argument"))) }
    };
    Ok(match name {
        "min" => args.iter().copied().fold(f64::INFINITY, f64::min),
        "max" => args.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "abs" => one()?.abs(),
        "floor" => one()?.floor(),
        "ceil" => one()?.ceil(),
        "round" => (one()? + 0.5).floor(),
        "rint" => one()?.round_ties_even(),
        "sqrt" => one()?.sqrt(),
        "log2" => one()?.log2(),
        "ln" | "log" => one()?.ln(),
        "exp" => one()?.exp(),
        "sign" => {
            let x = one()?;
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        _ => return Err(error(text, format!("Unknown function '{name}'"))),
    })
}

/// Evaluate `text` with the given variables (`INPUT_WIDTH`, ...).
pub fn evaluate(text: &str, variables: &BTreeMap<String, f64>) -> Result<f64> {
    let text = text.trim();
    Parser { text, tokens: tokenize(text)?, pos: 0, vars: variables }.parse()
}

/// Evaluate a size expression and round it like Magpie (`lround`); the
/// result must be positive.
pub fn evaluate_size(text: &str, variables: &BTreeMap<String, f64>) -> Result<u32> {
    let value = evaluate(text, variables)?;
    let size = (value + 0.5).floor();
    if !(size > 0.0) || size > u32::MAX as f64 {
        return Err(error(text, format!("Size {value} is not positive")));
    }
    Ok(size as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> BTreeMap<String, f64> {
        [("INPUT_WIDTH", 1280.0), ("INPUT_HEIGHT", 720.0), ("OUTPUT_WIDTH", 2560.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect()
    }

    fn ev(text: &str) -> f64 {
        evaluate(text, &vars()).unwrap()
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("INPUT_WIDTH*2"), 2560.0);
        assert_eq!(ev("1 + 2 * 3"), 7.0);
        assert_eq!(ev("(1 + 2) * 3"), 9.0);
        assert_eq!(ev("-2^2"), -4.0);
        assert_eq!(ev("2^-1"), 0.5);
        assert_eq!(ev("7 % 4"), 3.0);
        assert_eq!(ev("OUTPUT_WIDTH / 4"), 640.0);
        assert_eq!(ev(".5e1 + 1."), 6.0);
    }

    #[test]
    fn comparisons_ternary_and_functions() {
        assert_eq!(ev("INPUT_WIDTH > INPUT_HEIGHT ? 1 : 2"), 1.0);
        assert_eq!(ev("1 == 2 ? 1 : 2 < 3"), 1.0);
        assert_eq!(ev("max(INPUT_WIDTH, OUTPUT_WIDTH, 3)"), 2560.0);
        assert_eq!(ev("min(4, 2)"), 2.0);
        assert_eq!(ev("round(2.5) + rint(2.5) + floor(1.9) + ceil(1.1)"), 3.0 + 2.0 + 1.0 + 2.0);
        assert_eq!(ev("sign(-3) + abs(-2) + sqrt(16) + log2(8)"), -1.0 + 2.0 + 4.0 + 3.0);
    }

    #[test]
    fn errors() {
        for bad in ["INPUT_WIDTH *", "1 / 0", "FOO", "foo(1)", "1 $ 2", "(1", "1 2", "abs(1, 2)"] {
            assert!(evaluate(bad, &vars()).is_err(), "{bad}");
        }
    }

    #[test]
    fn sizes_round_like_lround() {
        assert_eq!(evaluate_size("INPUT_WIDTH / 3", &vars()).unwrap(), 427);
        assert_eq!(evaluate_size("2.5", &vars()).unwrap(), 3);
        assert!(evaluate_size("0.4", &vars()).is_err());
        assert!(evaluate_size("-INPUT_WIDTH", &vars()).is_err());
    }
}
