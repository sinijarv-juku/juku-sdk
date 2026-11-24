//! Numbers as text, written out by hand: `format!` would bring std's
//! formatting machinery into juku-emu's app for this alone (its spec §12.1).

/// `v` in hex with at least `digits` digits, upper case.
pub(crate) fn hex(v: u32, digits: usize) -> String {
    let mut out = Vec::new();
    let mut v = v;
    while v != 0 || out.len() < digits {
        out.push(b"0123456789ABCDEF"[(v & 15) as usize]);
        v >>= 4;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

/// `v` in decimal.
pub(crate) fn dec(v: i64) -> String {
    let mut out = Vec::new();
    let mut n = v.unsigned_abs();
    loop {
        out.push(b'0' + (n % 10) as u8);
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if v < 0 {
        out.push(b'-');
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(hex(0, 2), "00");
        assert_eq!(hex(0xFF98, 4), "FF98");
        assert_eq!(hex(0x1F, 4), "001F");
        assert_eq!(hex(0x12345, 4), "12345");
        assert_eq!(dec(0), "0");
        assert_eq!(dec(-1205), "-1205");
        assert_eq!(dec(i64::MIN), i64::MIN.to_string());
        assert_eq!(dec(i64::MAX), i64::MAX.to_string());
    }
}
