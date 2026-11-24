//! JSON (RFC 8259) for the messages of the Debug Adapter Protocol and the
//! Language Server Protocol: a value tree, a parser and a writer; and in
//! [`frame`], the `Content-Length` framing both protocols put them in.
//!
//! juku-emu's debug server uses it, and so its app links it (juku-emu's
//! spec §12.1-12.2), as does the SDK's language server. The app is to stay
//! small, so this crate has no dependencies and uses no `format!`: numbers
//! are written as text by hand (`text.rs`). Objects keep their keys in
//! order, in a list: the protocols' objects are small, and a lookup is a
//! short scan. Their numbers are whole numbers; any other is kept as its
//! text, so that no floating-point parsing or printing comes in.

use core::ops::Index;

pub mod frame;
mod text;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(i64),
    /// A number with a fraction or an exponent, or too large for an i64, as
    /// written.
    OtherNumber(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

static NULL: Value = Value::Null;

/// Deepest nesting the parser accepts, so input cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// An object from its members.
pub fn obj<const N: usize>(members: [(&str, Value); N]) -> Value {
    // Only the move into a Vec is made for each N; the rest is shared.
    object(Vec::from(members))
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(members.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

impl Value {
    /// Parses a whole JSON text.
    pub fn parse(text: &str) -> Result<Value, String> {
        let mut p = Parser { s: text.as_bytes(), at: 0 };
        let v = p.value(0)?;
        p.space();
        if p.at != p.s.len() {
            return Err(p.error("text after the value"));
        }
        Ok(v)
    }

    /// The member `key` of an object, or null.
    pub fn get(&self, key: &str) -> &Value {
        match self {
            Value::Object(members) => members.iter().find(|(k, _)| k == key).map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }

    /// Adds or replaces the member `key` of an object.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Object(members) = self {
            match members.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => *v = value,
                None => members.push((key.to_string(), value)),
            }
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    /// The elements of an array; none for anything else.
    pub fn as_array(&self) -> &[Value] {
        match self {
            Value::Array(a) => a,
            _ => &[],
        }
    }

    /// The JSON text of the value.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => out.push_str(&text::dec(*n)),
            Value::OtherNumber(text) => out.push_str(text),
            Value::String(s) => write_string(out, s),
            Value::Array(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Value::Object(members) => {
                out.push('{');
                for (i, (k, v)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, k);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                out.push_str(&text::hex(c as u32, 4));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Index<&str> for Value {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(s)
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Value {
        Value::Array(a)
    }
}

macro_rules! from_number {
    ($($t:ty),*) => {
        $(impl From<$t> for Value {
            fn from(n: $t) -> Value {
                Value::Number(n as i64)
            }
        })*
    };
}

from_number!(u8, u16, u32, usize, i32, i64);

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> String {
        "JSON: ".to_string() + what + " at byte " + &text::dec(self.at as i64)
    }

    fn space(&mut self) {
        while self.s.get(self.at).is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        let found = self.peek() == Some(c);
        self.at += found as usize;
        found
    }

    fn word(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.s[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(v)
        } else {
            Err(self.error("unknown word"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        self.space();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("no value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut members = Vec::new();
        self.space();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return Err(self.error("no member name"));
            }
            let key = self.string()?;
            self.space();
            if !self.eat(b':') {
                return Err(self.error("no ':'"));
            }
            members.push((key, self.value(depth + 1)?));
            self.space();
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            if !self.eat(b',') {
                return Err(self.error("no ',' or '}'"));
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut elements = Vec::new();
        self.space();
        if self.eat(b']') {
            return Ok(Value::Array(elements));
        }
        loop {
            elements.push(self.value(depth + 1)?);
            self.space();
            if self.eat(b']') {
                return Ok(Value::Array(elements));
            }
            if !self.eat(b',') {
                return Err(self.error("no ',' or ']'"));
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.s.get(self.at..self.at + 4).ok_or_else(|| self.error("short \\u escape"))?;
        let text = core::str::from_utf8(digits).map_err(|_| self.error("bad \\u escape"))?;
        let v = u32::from_str_radix(text, 16).map_err(|_| self.error("bad \\u escape"))?;
        self.at += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            // A run of plain characters at once. The input is a str, so a run
            // ending at an ASCII byte is whole UTF-8.
            let start = self.at;
            while self.peek().is_some_and(|c| c != b'"' && c != b'\\' && c >= 0x20) {
                self.at += 1;
            }
            out.push_str(core::str::from_utf8(&self.s[start..self.at]).expect("a str"));
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let c = self.peek().ok_or_else(|| self.error("short escape"))?;
                    self.at += 1;
                    match c {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut u = self.hex4()?;
                            // A surrogate pair is one character; a lone
                            // surrogate becomes U+FFFD.
                            if (0xD800..0xDC00).contains(&u) && self.s[self.at..].starts_with(b"\\u") {
                                let back = self.at;
                                self.at += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    u = 0x10000 + ((u - 0xD800) << 10) + (low - 0xDC00);
                                } else {
                                    self.at = back;
                                }
                            }
                            out.push(char::from_u32(u).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
                Some(_) => return Err(self.error("control character in a string")),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        let mut whole = true;
        let digits = |p: &mut Parser| {
            let from = p.at;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.at += 1;
            }
            p.at > from
        };
        self.eat(b'-');
        if !self.eat(b'0') && !digits(self) {
            return Err(self.error("bad number"));
        }
        if self.eat(b'.') {
            whole = false;
            if !digits(self) {
                return Err(self.error("bad fraction"));
            }
        }
        if self.peek().is_some_and(|c| c == b'e' || c == b'E') {
            whole = false;
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if !digits(self) {
                return Err(self.error("bad exponent"));
            }
        }
        let text = core::str::from_utf8(&self.s[start..self.at]).expect("ASCII");
        Ok(match text.parse() {
            Ok(n) if whole => Value::Number(n),
            _ => Value::OtherNumber(text.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        let v = Value::parse(r#" {"a": [1, -2.5, 3e2, true, false, null], "b": {"c": "d"}, "": 0} "#).unwrap();
        assert_eq!(v["a"].as_array().len(), 6);
        assert_eq!(v["a"].as_array()[1], Value::OtherNumber("-2.5".into()));
        assert_eq!(v["a"].as_array()[2].as_i64(), None);
        assert_eq!(v["b"]["c"].as_str(), Some("d"));
        assert_eq!(v[""].as_i64(), Some(0));
        assert!(v["missing"]["deeper"].is_null());
        assert_eq!(v.to_json(), r#"{"a":[1,-2.5,3e2,true,false,null],"b":{"c":"d"},"":0}"#);
    }

    #[test]
    fn strings() {
        let v = Value::parse(r#""q\" s\\ \/ \b\f\n\r\t \u00e9 \ud83d\ude00 \ud800 x""#).unwrap();
        assert_eq!(v.as_str(), Some("q\" s\\ / \u{8}\u{c}\n\r\t é 😀 \u{FFFD} x"));
        assert_eq!(v.to_json(), "\"q\\\" s\\\\ / \\b\\f\\n\\r\\t é 😀 \u{FFFD} x\"");
        assert_eq!(Value::from("\u{1f}").to_json(), r#""\u001F""#);
        assert_eq!(Value::parse("\"ünï\"").unwrap().as_str(), Some("ünï"));
    }

    #[test]
    fn numbers() {
        for (text, n) in [("0", 0), ("-0", 0), ("12", 12), ("-7", -7), ("9223372036854775807", i64::MAX)] {
            assert_eq!(Value::parse(text).unwrap().as_i64(), Some(n), "{text}");
        }
        for text in ["1.5", "1E3", "2e-1", "-1e+2", "9223372036854775808"] {
            let v = Value::parse(text).unwrap();
            assert_eq!((v.as_i64(), v.to_json()), (None, text.to_string()));
        }
        assert_eq!(Value::from(-42).to_json(), "-42");
    }

    #[test]
    fn refused() {
        for text in [
            "", "{", "[1,]", "{\"a\" 1}", "{\"a\":1,}", "01", "1.", "-", "1e", ".5", "tru", "nul", "\"a", "\"\\x\"",
            "\"\u{1}\"", "\"\\u12\"", "[1] 2", "{1:2}",
        ] {
            assert!(Value::parse(text).is_err(), "{text:?}");
        }
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert!(Value::parse(&deep).is_err());
        let ok = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(Value::parse(&ok).is_ok());
    }

    #[test]
    fn building() {
        let mut v = obj([("seq", 1.into()), ("type", "event".into())]);
        v.set("seq", 2.into());
        v.set("body", obj([("list", vec![Value::Null].into())]));
        assert_eq!(v.to_json(), r#"{"seq":2,"type":"event","body":{"list":[null]}}"#);
    }
}
