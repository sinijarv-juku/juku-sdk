//! Source text: lines, the words in a line, and positions in the units the
//! client counts in.

/// The lines of `text`, ended by LF, CR LF or CR, as juku-asm and LSP split
/// them.
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.find(['\n', '\r']).unwrap_or(rest.len());
        lines.push(&rest[..end]);
        rest = &rest[end..];
        rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix(['\n', '\r'])).unwrap_or(rest);
    }
    lines
}

/// Line `n` of `text`, from 0; empty past the last.
pub fn line(text: &str, n: usize) -> &str {
    split_lines(text).get(n).copied().unwrap_or("")
}

/// `line` up to its comment: a `;` outside quotes.
pub fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            return &line[..i];
        }
    }
    line
}

/// What a word is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    /// A name, a mnemonic, a directive or a register: a letter, `_`, `?`,
    /// `@` or `.`, then those or digits.
    Name,
    /// A digit, then letters or digits: `12`, `0ffh`.
    Number,
}

/// The words of `line`'s code, outside quotes, as byte ranges.
pub fn words(line: &str) -> Vec<(Word, usize, usize)> {
    let code = strip_comment(line);
    let mut out = Vec::new();
    let mut chars = code.char_indices().peekable();
    let mut quoted = false;
    while let Some((i, c)) = chars.next() {
        if c == '\'' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        let kind = if c.is_ascii_alphabetic() || "_?@.".contains(c) {
            Word::Name
        } else if c.is_ascii_digit() {
            Word::Number
        } else {
            continue;
        };
        let mut end = i + c.len_utf8();
        while let Some(&(j, d)) = chars.peek() {
            let more = match kind {
                Word::Name => d.is_alphanumeric() || "_?@.".contains(d),
                Word::Number => d.is_ascii_alphanumeric(),
            };
            if !more {
                break;
            }
            end = j + d.len_utf8();
            chars.next();
        }
        out.push((kind, i, end));
    }
    out
}

/// The word of `line` at byte `at`, or just before it.
pub fn word_at(line: &str, at: usize) -> Option<(Word, usize, usize)> {
    words(line).into_iter().find(|&(_, start, end)| start <= at && at <= end)
}

/// The byte in `line` at `units` from its start, counted in UTF-8 bytes or
/// UTF-16 code units; the end if it is past it.
pub fn byte_at(line: &str, units: usize, utf8: bool) -> usize {
    if utf8 {
        let mut at = units.min(line.len());
        while !line.is_char_boundary(at) {
            at -= 1;
        }
        return at;
    }
    let mut count = 0;
    for (i, c) in line.char_indices() {
        if count >= units {
            return i;
        }
        count += c.len_utf16();
    }
    line.len()
}

/// How long `text` is, in UTF-8 bytes or UTF-16 code units.
pub fn units(text: &str, utf8: bool) -> usize {
    if utf8 { text.len() } else { text.encode_utf16().count() }
}

/// Sorts `v`, equal elements in their order. One merge sort of indices,
/// with the order given as a function, serves every type: std's sort is
/// built anew, inlined, for each type and order, and its copies were a
/// quarter of the server's code. The lists here are short.
pub fn sort_by<T>(v: &mut Vec<T>, order: impl Fn(&T, &T) -> std::cmp::Ordering) {
    let perm = permutation(v.len(), &|i, j| order(&v[i], &v[j]));
    let mut slots: Vec<Option<T>> = v.drain(..).map(Some).collect();
    v.extend(perm.into_iter().filter_map(|i| slots[i].take()));
}

/// Sorts `v` in its order; see [`sort_by`].
pub fn sort<T: Ord>(v: &mut Vec<T>) {
    sort_by(v, T::cmp);
}

/// Sorts `v` by `key`; see [`sort_by`].
pub fn sort_by_key<T, K: Ord>(v: &mut Vec<T>, key: impl Fn(&T) -> K) {
    sort_by(v, |a, b| key(a).cmp(&key(b)));
}

/// The indices 0..n in the order `order` gives them, equal ones as they
/// were: a merge sort from runs of one up.
fn permutation(n: usize, order: &dyn Fn(usize, usize) -> std::cmp::Ordering) -> Vec<usize> {
    let mut a: Vec<usize> = (0..n).collect();
    let mut b = vec![0; n];
    let mut width = 1;
    while width < n {
        for start in (0..n).step_by(2 * width) {
            let (mid, end) = ((start + width).min(n), (start + 2 * width).min(n));
            let (mut i, mut j) = (start, mid);
            for slot in &mut b[start..end] {
                if j >= end || (i < mid && order(a[i], a[j]) != std::cmp::Ordering::Greater) {
                    *slot = a[i];
                    i += 1;
                } else {
                    *slot = a[j];
                    j += 1;
                }
            }
        }
        std::mem::swap(&mut a, &mut b);
        width *= 2;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_words() {
        let line = "loop:   mvi a, 'x;y' + 0ffh ; count rb.tto";
        let got: Vec<(Word, &str)> = words(line).into_iter().map(|(k, s, e)| (k, &line[s..e])).collect();
        assert_eq!(got, [(Word::Name, "loop"), (Word::Name, "mvi"), (Word::Name, "a"), (Word::Number, "0ffh")]);
        let line = "        call rb.tto  ; print";
        assert_eq!(word_at(line, 14).map(|(_, s, e)| &line[s..e]), Some("rb.tto"));
        assert_eq!(word_at(line, 19).map(|(_, s, e)| &line[s..e]), Some("rb.tto"), "just after it");
        assert_eq!(word_at(line, 22), None, "in the comment");
    }

    #[test]
    fn sorting() {
        let mut v: Vec<(u8, char)> = vec![(3, 'a'), (1, 'b'), (2, 'c'), (1, 'd'), (3, 'e'), (0, 'f'), (2, 'g')];
        sort_by_key(&mut v, |p| p.0);
        assert_eq!(v, [(0, 'f'), (1, 'b'), (1, 'd'), (2, 'c'), (2, 'g'), (3, 'a'), (3, 'e')], "equal ones in their order");
        for n in 0..40 {
            let mut w: Vec<u32> = (0..n).map(|i| (i * 7919 + 13) % 23).collect();
            let mut want = w.clone();
            want.sort();
            sort(&mut w);
            assert_eq!(w, want);
        }
    }

    #[test]
    fn positions() {
        let line = "db 'ä😀', x";
        assert_eq!(byte_at(line, 10, true), 10);
        assert_eq!(byte_at(line, 5, true), 4, "inside ä: its start");
        assert_eq!(byte_at(line, 7, false), 10, "😀 is two UTF-16 units");
        assert_eq!(units(&line[..10], false), 7);
        assert_eq!(byte_at(line, 99, false), line.len());
        assert_eq!(split_lines("a\r\nb\rc\n\nd"), ["a", "b", "c", "", "d"]);
    }
}
