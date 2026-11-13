//! Qwilight's Lua lexer accepts a number immediately followed by a keyword
//! (for example `3then`). Lua 5.4 needs whitespace. Preserve quoted text and
//! comments verbatim; this is lexical normalization, never an extra evaluator.
pub(super) fn normalize_lua(source: &str) -> String {
    let source = source.trim_start_matches('\u{feff}');
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let comment = bytes[i..].starts_with(b"--");
        let bracket = if comment { i + 2 } else { i };
        if let Some((open_len, close)) = long_bracket(&bytes[bracket..]) {
            i = bracket + open_len;
            i = bytes[i..]
                .windows(close.len())
                .position(|s| s == close)
                .map_or(bytes.len(), |end| i + end + close.len());
        } else if comment {
            i += bytes[i..].iter().position(|&b| b == b'\n').unwrap_or(bytes.len() - i);
        } else if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else {
                    let end = bytes[i] == quote;
                    i += 1;
                    if end {
                        break;
                    }
                }
            }
        } else if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if start > 0
                && bytes[start - 1].is_ascii_digit()
                && [b"then".as_slice(), b"do", b"and", b"or", b"end", b"else", b"elseif", b"until"]
                    .contains(&&bytes[start..i])
            {
                out.push(b' ');
            }
        } else {
            i += 1;
        }
        out.extend_from_slice(&bytes[start..i]);
    }
    // Only ASCII spaces are inserted into UTF-8 input.
    String::from_utf8(out).expect("preserved UTF-8")
}

fn long_bracket(bytes: &[u8]) -> Option<(usize, Vec<u8>)> {
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let equals = bytes[1..].iter().take_while(|&&b| b == b'=').count();
    if bytes.get(equals + 1) != Some(&b'[') {
        return None;
    }
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', equals));
    close.push(b']');
    Some((equals + 2, close))
}
