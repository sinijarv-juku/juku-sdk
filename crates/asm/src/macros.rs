//! Macros, FOR lists and the functions of an argument's text: what is
//! worked out from the source as written, before it is assembled.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asm::{Error, Frame, Line, Result, is_name, name_len};

/// The registers and the register pairs, as REG() and PAIR() know them.
const REGS: [&str; 8] = ["a", "b", "c", "d", "e", "h", "l", "m"];
const PAIRS: [&str; 5] = ["b", "d", "h", "sp", "psw"];

/// A parameter: its name, the text it stands for when the argument is
/// left out, and whether it takes the rest of the arguments as a list.
pub(crate) struct Param {
    name: String,
    default: Option<String>,
    rest: bool,
}

pub(crate) struct Macro {
    /// As defined, under the namespaces around it.
    pub name: String,
    params: Vec<Param>,
    pub body: Vec<Line>,
    /// The namespaces it is defined in, which its body's names are in.
    pub frames: Vec<Frame>,
}

impl Macro {
    /// The macro `name` with the parameters `params`, as written after
    /// MACRO: `dst, src`, `row=0`, `regs...`.
    pub fn new(name: String, params: &str, body: Vec<Line>, frames: Vec<Frame>) -> Result<Macro> {
        let mut out: Vec<Param> = Vec::new();
        for p in split_top(params) {
            if out.last().is_some_and(|p| p.rest) {
                return Err(Error::new("only the last parameter takes the rest"));
            }
            let (p, default) = match p.split_once('=') {
                Some((p, d)) => (p.trim(), Some(d.trim().to_string())),
                None => (p.as_str(), None),
            };
            let (p, rest) = match p.strip_suffix("...") {
                Some(p) => (p.trim(), true),
                None => (p, false),
            };
            if !is_name(p) || p.contains('.') || REGS.contains(&p.to_lowercase().as_str()) || PAIRS.contains(&p.to_lowercase().as_str()) {
                return Err(Error::new(format!("not a parameter name: {p}")));
            }
            if out.iter().any(|q| q.name == p) {
                return Err(Error::new(format!("parameter {p} twice")));
            }
            out.push(Param { name: p.to_string(), default, rest });
        }
        Ok(Macro { name, params: out, body, frames })
    }

    /// Each parameter's text for the arguments `args`: the rest of them, for
    /// a `name...`, as a list in brackets.
    pub fn bind(&self, args: &[String]) -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        for (i, p) in self.params.iter().enumerate() {
            if p.rest {
                let rest = args.get(i..).unwrap_or_default();
                out.push((p.name.clone(), format!("[{}]", rest.join(", "))));
                return Ok(out);
            }
            let arg = args.get(i).map(|a| a.trim()).filter(|a| !a.is_empty());
            let text = arg.map(str::to_string).or_else(|| p.default.clone()).unwrap_or_default();
            out.push((p.name.clone(), text));
        }
        if args.len() > self.params.len() {
            return Err(Error::new(format!("{} takes {} argument(s)", self.name, self.params.len())));
        }
        Ok(out)
    }
}

/// `text` with each name that `f` gives a replacement for replaced, outside
/// quotes. Numbers are left alone.
pub(crate) fn map_names(text: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    map_names_after(text, |name, _| f(name))
}

/// As `map_names`, with `f` given what comes before the name too.
pub(crate) fn map_names_after(text: &str, mut f: impl FnMut(&str, &str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let n = if c == '\'' {
            rest[1..].find('\'').map_or(rest.len(), |i| i + 2)
        } else if c.is_ascii_digit() {
            rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len())
        } else if let n @ 1.. = name_len(rest) {
            match f(&rest[..n], &out) {
                Some(new) => out.push_str(&new),
                None => out.push_str(&rest[..n]),
            }
            rest = &rest[n..];
            continue;
        } else {
            c.len_utf8()
        };
        out.push_str(&rest[..n]);
        rest = &rest[n..];
    }
    out
}

/// `text` without what is between quotes.
pub(crate) fn outside_quotes(text: &str) -> String {
    text.split('\'').step_by(2).collect()
}

/// `text` with each `{text}` outside quotes replaced by the text, joined to
/// what is around it: `sfx_{jump}` is `sfx_jump`. In it, `upper(text)` and
/// `lower(text)` change the text's case.
pub(crate) fn join(text: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '\'' {
            let n = rest[1..].find('\'').map_or(rest.len(), |i| i + 2);
            out.push_str(&rest[..n]);
            rest = &rest[n..];
        } else if c == '{' {
            let end = rest.find('}').ok_or_else(|| Error::new("missing }"))?;
            out.push_str(&case(&rest[1..end]));
            rest = &rest[end + 1..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(out)
}

/// `text`, in its case or, for `upper(text)` and `lower(text)`, in upper or
/// lower case.
fn case(text: &str) -> String {
    if let Some(inner) = call_of(text, "upper") {
        return case(inner).to_uppercase();
    }
    if let Some(inner) = call_of(text, "lower") {
        return case(inner).to_lowercase();
    }
    text.trim().to_string()
}

/// `text` with the parameters in `binding` replaced by their text.
pub(crate) fn substitute(text: &str, binding: &[(String, String)]) -> String {
    map_names(text, |name| binding.iter().find(|(p, _)| p == name).map(|(_, v)| v.clone()))
}

/// The length of the text in parentheses or brackets that `text` starts
/// with, up to and with the closing one: quotes and inner ones skipped.
pub(crate) fn group_len(text: &str) -> Option<usize> {
    let mut depth = 0;
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits `text` on the commas outside quotes, parentheses and brackets, each part
/// trimmed: none for blank text, and a blank part between two commas.
pub(crate) fn split_top(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let (mut out, mut start, mut depth, mut quoted) = (Vec::new(), 0, 0, false);
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(text[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].trim().to_string());
    out
}

/// What is between the parentheses of `text`, if it is all `name(...)`.
fn call_of<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let t = text.trim();
    let after = t.get(..name.len()).filter(|w| w.eq_ignore_ascii_case(name)).map(|_| t[name.len()..].trim_start())?;
    let n = group_len(after).filter(|&n| after.starts_with('(') && n == after.len())?;
    Some(&after[1..n - 1])
}

/// `text` is a list in brackets.
pub(crate) fn is_list(text: &str) -> bool {
    let t = text.trim();
    t.starts_with('[') && group_len(t) == Some(t.len())
}

/// `text` is a string in quotes, with any quote in it doubled.
pub(crate) fn is_string(text: &str) -> bool {
    let Some(mut rest) = text.trim().strip_prefix('\'') else {
        return false;
    };
    while let Some(i) = rest.find('\'') {
        rest = &rest[i + 1..];
        if rest.is_empty() {
            return true;
        }
        match rest.strip_prefix('\'') {
            Some(after) => rest = after,
            None => return false,
        }
    }
    false
}

/// The items of a list: `a, b, c`, `[a, b, c]` or `reverse(list)`.
pub(crate) fn list(text: &str) -> Vec<String> {
    if let Some(inner) = call_of(text, "reverse") {
        let mut items = list(inner);
        items.reverse();
        return items;
    }
    let t = text.trim();
    if is_list(t) {
        return split_top(&t[1..t.len() - 1]);
    }
    split_top(t)
}

/// `text` is a register (`reg`) or a register pair, in any case.
fn is_reg(text: &str, pairs: bool) -> bool {
    let t = text.trim().to_lowercase();
    if pairs { PAIRS.contains(&t.as_str()) } else { REGS.contains(&t.as_str()) }
}

/// SAME(x, y): the same text but for spaces; a register in any case.
fn same(x: &str, y: &str) -> bool {
    let squeeze = |t: &str| map_names(t, |_| None).split_whitespace().collect::<String>();
    let (x, y) = (squeeze(x), squeeze(y));
    let reg = |t: &str| is_reg(t, false) || is_reg(t, true);
    if reg(&x) && reg(&y) { x.eq_ignore_ascii_case(&y) } else { x == y }
}

/// What the functions of an argument's text need from the assembler: the
/// value of an expression, whether one has a value yet, and a string's
/// address in CONST.
pub(crate) trait Values {
    fn value(&mut self, text: &str) -> Result<i64>;
    fn known(&mut self, text: &str) -> bool;
    fn defined(&mut self, name: &str) -> bool;
    fn cstr(&mut self, bytes: Vec<u8>) -> Result<i64>;
}

/// `text` with each call of SAME, REG, PAIR, CONST, BLANK, COUNT, DEFINED
/// and CSTR replaced by its value.
pub(crate) fn functions(text: &str, values: &mut impl Values) -> Result<String> {
    const NAMES: [&str; 8] = ["SAME", "REG", "PAIR", "CONST", "BLANK", "COUNT", "DEFINED", "CSTR"];
    if !text.contains('(') {
        return Ok(text.to_string());
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let n = if c == '\'' {
            rest[1..].find('\'').map_or(rest.len(), |i| i + 2)
        } else if c.is_ascii_digit() {
            rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len())
        } else if let n @ 1.. = name_len(rest) {
            let name = &rest[..n];
            let after = rest[n..].trim_start();
            match NAMES.iter().position(|f| name.eq_ignore_ascii_case(f)) {
                Some(f) if after.starts_with('(') => {
                    let len = group_len(after).ok_or_else(|| Error::new("missing )"))?;
                    let inner = &after[1..len - 1];
                    out.push_str(&function(f, &name.to_lowercase(), inner, values)?.to_string());
                    rest = &after[len..];
                    continue;
                }
                _ => n,
            }
        } else {
            c.len_utf8()
        };
        out.push_str(&rest[..n]);
        rest = &rest[n..];
    }
    Ok(out)
}

/// The value of function `f` of NAMES, written `name`, of the text `inner`.
fn function(f: usize, name: &str, inner: &str, values: &mut impl Values) -> Result<i64> {
    let one = || {
        let args = split_top(inner);
        if args.len() == 1 { Ok(args[0].clone()) } else { Err(Error::new(format!("{name}() takes one argument"))) }
    };
    Ok(match f {
        0 => {
            let args = split_top(inner);
            let [x, y] = &args[..] else {
                return Err(Error::new("same() takes two arguments"));
            };
            same(x, y) as i64
        }
        1 => is_reg(&one()?, false) as i64,
        2 => is_reg(&one()?, true) as i64,
        3 => {
            let x = one()?;
            (!is_reg(&x, false) && !is_reg(&x, true) && values.known(&x)) as i64
        }
        4 => (inner.trim().is_empty() || inner.trim() == "[]") as i64,
        5 => list(inner).len() as i64,
        6 => {
            let name = one()?;
            if name.is_empty() || name_len(&name) != name.len() {
                return Err(Error::new("defined() takes a name"));
            }
            values.defined(&name) as i64
        }
        _ => {
            let mut bytes = Vec::new();
            for item in split_top(inner) {
                let n = item.chars().count();
                if n >= 2 && item.starts_with('\'') && item.ends_with('\'') && n != 3 {
                    bytes.extend(item[1..item.len() - 1].replace("''", "'").chars().map(|c| c as u8));
                } else {
                    let v = values.value(&item)?;
                    if !(-128..=255).contains(&v) {
                        return Err(Error::new(format!("{v} does not fit in a byte")));
                    }
                    bytes.push(v as u8);
                }
            }
            bytes.push(0);
            values.cstr(bytes)?
        }
    })
}

/// The strings CSTR() made, laid out: the bytes, and where each string is
/// in them. One that is the end of a longer one is in that one.
pub(crate) fn pool(strings: &[Vec<u8>]) -> (Vec<u8>, HashMap<Vec<u8>, i64>) {
    let mut order: Vec<&Vec<u8>> = strings.iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let (mut bytes, mut placed, mut offsets) = (Vec::new(), Vec::<(i64, &Vec<u8>)>::new(), HashMap::new());
    for s in order {
        let at = match placed.iter().find(|(_, t)| t.ends_with(s)) {
            Some((o, t)) => o + (t.len() - s.len()) as i64,
            None => {
                let o = bytes.len() as i64;
                bytes.extend(s.iter());
                placed.push((o, s));
                o
            }
        };
        offsets.insert(s.clone(), at);
    }
    (bytes, offsets)
}

/// `bytes` as DB writes them: text in quotes, the rest in hex.
pub(crate) fn shown(bytes: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut text = String::new();
    for &b in bytes {
        if (0x20..0x7F).contains(&b) {
            if b == b'\'' {
                text.push('\'');
            }
            text.push(b as char);
            continue;
        }
        if !text.is_empty() {
            parts.push(format!("'{}'", std::mem::take(&mut text)));
        }
        parts.push(b.to_string());
    }
    if !text.is_empty() {
        parts.push(format!("'{text}'"));
    }
    parts.join(", ")
}

/// The macros defined in a pass, by full name.
pub(crate) type Macros = HashMap<String, Rc<Macro>>;
