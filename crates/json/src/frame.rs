//! Messages as DAP and LSP frame them: header lines, each `Name: value`,
//! up to an empty one, then a body of as many bytes as `Content-Length`
//! says.

use crate::text;

/// Longest header taken.
const MAX_HEADER: usize = 1024;

/// A message that cannot be read: a header that is not one, too long, or
/// with no `Content-Length`, or a body too long or not UTF-8. The stream
/// cannot be followed past it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadMessage;

/// Takes one message's body from `input` if it is all there, and drops the
/// message from it. A body longer than `max_body` bytes is refused.
pub fn take_message(input: &mut Vec<u8>, max_body: usize) -> Result<Option<String>, BadMessage> {
    let mut length = None;
    let mut at = 0;
    loop {
        let Some(end) = input[at..].iter().position(|&b| b == b'\n') else {
            return if input.len() > MAX_HEADER { Err(BadMessage) } else { Ok(None) };
        };
        let line = input[at..at + end].strip_suffix(b"\r").unwrap_or(&input[at..at + end]);
        at += end + 1;
        if line.is_empty() {
            break;
        }
        let line = core::str::from_utf8(line).map_err(|_| BadMessage)?;
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("Content-Length")
        {
            length = Some(value.trim().parse::<usize>().map_err(|_| BadMessage)?);
        }
        if at > MAX_HEADER {
            return Err(BadMessage);
        }
    }
    let length = length.filter(|&n| n <= max_body).ok_or(BadMessage)?;
    if input.len() < at + length {
        return Ok(None);
    }
    let body = String::from_utf8(input[at..at + length].to_vec()).map_err(|_| BadMessage)?;
    input.drain(..at + length);
    Ok(Some(body))
}

/// Appends a message with body `body` to `out`.
pub fn write_message(out: &mut Vec<u8>, body: &str) {
    out.extend_from_slice(b"Content-Length: ");
    out.extend_from_slice(text::dec(body.len() as i64).as_bytes());
    out.extend_from_slice(b"\r\n\r\n");
    out.extend_from_slice(body.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_BODY: usize = 1 << 20;

    #[test]
    fn messages() {
        let mut input = b"Content-Length: 2\r\n\r\n{}content-length:3\nX-Other: 1\n\n[1]Content-Length: 5\r\n\r\n[1".to_vec();
        assert_eq!(take_message(&mut input, MAX_BODY), Ok(Some("{}".into())));
        assert_eq!(take_message(&mut input, MAX_BODY), Ok(Some("[1]".into())));
        // Not all there yet.
        assert_eq!(take_message(&mut input, MAX_BODY), Ok(None));
        input.extend_from_slice(b",2]");
        assert_eq!(take_message(&mut input, MAX_BODY), Ok(Some("[1,2]".into())));
        assert_eq!(take_message(&mut input, MAX_BODY), Ok(None));
        assert!(input.is_empty());
    }

    #[test]
    fn bad_messages() {
        for input in [&b"\r\n{}"[..], b"Content-Length: x\r\n\r\n", b"Content-Length: 99999999\r\n\r\n"] {
            assert_eq!(take_message(&mut input.to_vec(), MAX_BODY), Err(BadMessage));
        }
        assert_eq!(take_message(&mut vec![b'x'; MAX_HEADER + 1], MAX_BODY), Err(BadMessage));
        assert_eq!(take_message(&mut b"Content-Length: 3\r\n\r\nabc".to_vec(), 2), Err(BadMessage));
    }

    #[test]
    fn written() {
        let mut out = Vec::new();
        write_message(&mut out, "{\"ü\":1}");
        assert_eq!(out, "Content-Length: 8\r\n\r\n{\"ü\":1}".as_bytes());
        assert_eq!(take_message(&mut out, MAX_BODY), Ok(Some("{\"ü\":1}".into())));
    }
}
