//! Operand expressions, evaluated by recursive descent.

use crate::asm::{Assembler, CALLERS, Error, Result, SECTIONS, check_caller, name_len};

#[derive(Clone, PartialEq)]
enum Token<'a> {
    Num(i64),
    /// Qualified: a local label has its scope.
    Name(String),
    /// An anonymous label: -1 for `@-`, 2 for `@++`.
    Anon(i64),
    Op(&'a str),
}

/// The binary operators, loosest first.
const LEVELS: [&[&str]; 7] = [
    &["|"],
    &["^"],
    &["&"],
    &["==", "!=", "<", ">", "<=", ">="],
    &["<<", ">>"],
    &["+", "-"],
    &["*", "/", "%"],
];

/// The value of `text`. Before the last pass a symbol not defined yet is 0,
/// and notes that the pass has one.
pub(crate) fn eval(text: &str, asm: &mut Assembler) -> Result<i64> {
    let tokens = tokens(text, asm)?;
    let mut e = Expr { tokens, pos: 0, asm };
    let v = e.binary(0)?;
    if e.pos != e.tokens.len() {
        return Err(Error::new("unexpected text in expression"));
    }
    Ok(v)
}

fn tokens<'a>(text: &'a str, asm: &Assembler) -> Result<Vec<Token<'a>>> {
    let mut out = Vec::new();
    let mut s = text.trim_start();
    while let Some(c) = s.chars().next() {
        let (token, len) = if c.is_ascii_digit() {
            let b = s.as_bytes();
            let mut n = 1 + b[1..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if b.get(n).is_some_and(|c| b"HhBbDd".contains(c)) {
                n += 1;
            }
            (Token::Num(number(&s[..n])?), n)
        } else if let Some((v, n)) = char_literal(s) {
            (Token::Num(v), n)
        } else if let n @ 1.. = name_len(s) {
            check_caller(&s[..n], asm.depth())?;
            (Token::Name(asm.qualify(&s[..n])), n)
        } else if let Some(rest) = s.strip_prefix(CALLERS)
            && !matches!(out.last(), Some(Token::Num(_) | Token::Name(_) | Token::Anon(_) | Token::Op(")")))
            && let n @ 1.. = name_len(rest)
        {
            // A full name, as a macro's argument: `^name`.
            (Token::Name(format!("{CALLERS}{}", &rest[..n])), 1 + n)
        } else if let Some(rest) = s.strip_prefix('@') {
            let sign = rest.chars().next().filter(|c| *c == '+' || *c == '-').ok_or_else(|| Error::new("expected @+ or @-"))?;
            let n = rest.chars().take_while(|&c| c == sign).count();
            (Token::Anon(if sign == '-' { -(n as i64) } else { n as i64 }), 1 + n)
        } else if c == '$' {
            (Token::Num(asm.here), 1)
        } else {
            let two = ["<<", ">>", "==", "!=", "<=", ">="].iter().any(|op| s.starts_with(op));
            let n = if two { 2 } else { c.len_utf8() };
            (Token::Op(&s[..n]), n)
        };
        out.push(token);
        s = s[len..].trim_start();
    }
    Ok(out)
}

/// 123, 0FFh, 1010b or 123d.
fn number(text: &str) -> Result<i64> {
    let t = text.to_ascii_uppercase();
    let (digits, radix) = if let Some(h) = t.strip_suffix('H') {
        (h, 16)
    } else if let Some(b) = t.strip_suffix('B')
        && b.bytes().all(|c| c == b'0' || c == b'1')
    {
        (b, 2)
    } else if let Some(d) = t.strip_suffix('D') {
        (d, 10)
    } else {
        (t.as_str(), 10)
    };
    i64::from_str_radix(digits, radix).map_err(|_| Error::new(format!("bad number {text}")))
}

/// `'c'`, or `''''` for the quote: its value and length.
fn char_literal(s: &str) -> Option<(i64, usize)> {
    let mut chars = s.chars();
    if chars.next()? != '\'' {
        return None;
    }
    match (chars.next()?, chars.next()?) {
        ('\'', '\'') => (chars.next()? == '\'').then_some(('\'' as i64, 4)),
        (c, '\'') => Some((c as i64, 2 + c.len_utf8())),
        _ => None,
    }
}

struct Expr<'a, 'b> {
    tokens: Vec<Token<'a>>,
    pos: usize,
    asm: &'b mut Assembler,
}

impl<'a> Expr<'a, '_> {
    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.pos)
    }

    fn take(&mut self) -> Option<Token<'a>> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    fn binary(&mut self, level: usize) -> Result<i64> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut v = self.binary(level + 1)?;
        while let Some(&Token::Op(op)) = self.peek()
            && LEVELS[level].contains(&op)
        {
            self.pos += 1;
            let r = self.binary(level + 1)?;
            v = apply(op, v, r)?;
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<i64> {
        match self.take() {
            Some(Token::Op("-")) => Ok(self.unary()?.wrapping_neg()),
            Some(Token::Op("~")) => Ok(!self.unary()?),
            Some(Token::Op("!")) => Ok((self.unary()? == 0) as i64),
            Some(Token::Op("(")) => self.parenthesized(),
            Some(Token::Op("[")) => Err(Error::new("a list is not a number")),
            Some(Token::Num(v)) => Ok(v),
            Some(Token::Anon(_)) if self.asm.strict => Err(Error::new("not known yet")),
            Some(Token::Anon(n)) => self.asm.anon(n),
            Some(Token::Name(name)) => {
                let base = if name.starts_with('.') { name.rsplit('.').next().unwrap_or("") } else { &name };
                let (high, low) = (base.eq_ignore_ascii_case("HIGH"), base.eq_ignore_ascii_case("LOW"));
                if (high || low) && self.peek() == Some(&Token::Op("(")) {
                    self.pos += 1;
                    let v = self.parenthesized()?;
                    return Ok(if high { (v >> 8) & 0xFF } else { v & 0xFF });
                }
                let bound = ["START", "END", "SIZE"].iter().position(|f| name.eq_ignore_ascii_case(f));
                if let Some(what) = bound
                    && self.peek() == Some(&Token::Op("("))
                {
                    self.pos += 1;
                    let arg = self.take();
                    let section = match &arg {
                        Some(Token::Name(s)) => SECTIONS.iter().position(|n| s.eq_ignore_ascii_case(n)),
                        _ => None,
                    };
                    let closed = self.take() == Some(Token::Op(")"));
                    // SIZE() of a struct.
                    if let (2, None, Some(Token::Name(s)), true) = (what, section, &arg, closed) {
                        return self.asm.struct_size(s);
                    }
                    let (Some(section), true) = (section, closed) else {
                        let f = name.to_lowercase();
                        if what == 2 {
                            return Err(Error::new("size() takes a section (code, const, data, bss) or a struct"));
                        }
                        return Err(Error::new(format!("{f}() takes a section: code, const, data or bss")));
                    };
                    if self.asm.strict {
                        return Err(Error::new("not known yet"));
                    }
                    return Ok(self.asm.bound(what, section));
                }
                if self.asm.strict && !self.asm.is_defined(&name) {
                    return Err(Error::new("not known yet"));
                }
                if let Some(v) = self.asm.lookup(&name)? {
                    return Ok(v);
                }
                if self.asm.last_pass {
                    let name = name.trim_start_matches('^');
                    if self.asm.is_struct(name) {
                        return Err(Error::new(format!("{name} is a struct, not a number: size({name}) is its size")));
                    }
                    return Err(Error::new(format!("undefined symbol {name}")));
                }
                self.asm.unresolved = true;
                Ok(0)
            }
            _ => Err(Error::new("bad expression")),
        }
    }

    /// The rest of `( expr )`, after the opening parenthesis.
    fn parenthesized(&mut self) -> Result<i64> {
        let v = self.binary(0)?;
        if self.take() != Some(Token::Op(")")) {
            return Err(Error::new("missing )"));
        }
        Ok(v)
    }
}

/// Division rounds down, and by 0 is 0.
fn apply(op: &str, v: i64, r: i64) -> Result<i64> {
    if (op == "<<" || op == ">>") && r < 0 {
        return Err(Error::new("negative shift count"));
    }
    Ok(match op {
        "|" => v | r,
        "^" => v ^ r,
        "&" => v & r,
        "+" => v.wrapping_add(r),
        "-" => v.wrapping_sub(r),
        "*" => v.wrapping_mul(r),
        "/" if r == 0 => 0,
        "%" if r == 0 => 0,
        "/" => {
            let q = v.wrapping_div(r);
            if v.wrapping_rem(r) != 0 && (v < 0) != (r < 0) { q - 1 } else { q }
        }
        "%" => {
            let m = v.wrapping_rem(r);
            if m != 0 && (m < 0) != (r < 0) { m + r } else { m }
        }
        "<<" if r >= 64 => 0,
        "<<" => v << r,
        ">>" => v >> r.min(63),
        "==" => (v == r) as i64,
        "!=" => (v != r) as i64,
        "<" => (v < r) as i64,
        ">" => (v > r) as i64,
        "<=" => (v <= r) as i64,
        ">=" => (v >= r) as i64,
        _ => unreachable!("{op} is not an operator"),
    })
}
