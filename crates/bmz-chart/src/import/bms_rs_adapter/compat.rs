/// beatoraja / jbms-parser が受け付ける区切り文字を bms-rs 向けに正規化する。
///
/// 一部の譜面では `#BPMxx:value` / `#STOPxx:value` が使われる。jbms-parser
/// は object id の直後から値を読むが、bms-rs はヘッダ名と値の間に空白が
/// 必要なため、パーサーへ渡すテキストだけを正規化する。ハッシュ計算や
/// 生ヘッダ保存に使う原文は変更しない。空白区切りの小節行は、通常の
/// `#xxxyy:data` へ変換して小節長・BGA・疎行を含むすべての経路へ渡す。
pub(super) fn normalize_beatoraja_separators(text: &str) -> String {
    let mut rewritten = String::with_capacity(text.len());
    for line in text.lines() {
        if let Some(colon) = legacy_header_colon(line) {
            rewritten.push_str(&line[..colon]);
            rewritten.push(' ');
            rewritten.push_str(&line[colon + 1..]);
        } else if let Some((head, payload)) =
            line.trim_start().strip_prefix('#').and_then(split_bms_channel_command)
        {
            let leading = line.len() - line.trim_start().len();
            rewritten.push_str(&line[..leading + 1 + head.len()]);
            rewritten.push(':');
            rewritten.push_str(
                payload.trim_start_matches([':', ' ', '\t']).trim_end_matches([' ', '\t']),
            );
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }
    rewritten
}

/// `#` を除いた小節行を分割する。PMS 判定と生ヘッダ抽出にも使い、
/// 正規化前の空白区切り行をヘッダとして扱わないようにする。
pub(super) fn split_bms_channel_command(body: &str) -> Option<(&str, &str)> {
    let delimiter = body.find([':', ' ', '\t'])?;
    let head = &body[..delimiter];
    let bytes = head.as_bytes();
    if bytes.len() < 5
        || !bytes[..bytes.len() - 2].iter().all(u8::is_ascii_digit)
        || !bytes[bytes.len() - 2..].iter().all(u8::is_ascii_alphanumeric)
    {
        return None;
    }
    Some((head, &body[delimiter + 1..]))
}

fn legacy_header_colon(line: &str) -> Option<usize> {
    let leading = line.len() - line.trim_start().len();
    let body = &line[leading..];
    let bytes = body.as_bytes();

    if bytes.len() >= 7
        && bytes[0] == b'#'
        && ascii_prefix_ignore_case(&bytes[1..], b"BPM")
        && is_ascii_base36(bytes[4])
        && is_ascii_base36(bytes[5])
        && bytes[6] == b':'
    {
        return Some(leading + 6);
    }

    if bytes.len() >= 8
        && bytes[0] == b'#'
        && ascii_prefix_ignore_case(&bytes[1..], b"STOP")
        && is_ascii_base36(bytes[5])
        && is_ascii_base36(bytes[6])
        && bytes[7] == b':'
    {
        return Some(leading + 7);
    }

    None
}

fn ascii_prefix_ignore_case(value: &[u8], prefix: &[u8]) -> bool {
    value.get(..prefix.len()).is_some_and(|candidate| {
        candidate.iter().zip(prefix).all(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
    })
}

fn is_ascii_base36(byte: u8) -> bool {
    byte.is_ascii_digit() || byte.is_ascii_uppercase() || byte.is_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::normalize_beatoraja_separators;

    #[test]
    fn normalizes_colon_separated_bpm_and_stop_definitions() {
        let source = "#BPM0A:160000160\n#STOP01:8000008.0\n#BPM 160\n#00108:0A\n";

        assert_eq!(
            normalize_beatoraja_separators(source),
            "#BPM0A 160000160\n#STOP01 8000008.0\n#BPM 160\n#00108:0A\n"
        );
    }

    #[test]
    fn preserves_leading_whitespace_and_unrelated_colons() {
        let source = "  #bpm01:240\n#TITLE Artist: Song\n#BPM:invalid\n";

        assert_eq!(
            normalize_beatoraja_separators(source),
            "  #bpm01 240\n#TITLE Artist: Song\n#BPM:invalid\n"
        );
    }

    #[test]
    fn normalizes_space_and_tab_separated_channel_data() {
        let source = "  #00111  : 0101  \r\n#00002   0.5\n#10000A\t\t01\t\n#00212:0202\n";

        assert_eq!(
            normalize_beatoraja_separators(source),
            "  #00111:0101\n#00002:0.5\n#10000A:01\n#00212:0202\n"
        );
    }

    #[test]
    fn preserves_headers_and_malformed_channel_heads() {
        let source = "#TITLE 00111 0101\n#0011 0101\n#00A11 0101\n#001あ 01\n#TITLE Artist: Song\n";

        assert_eq!(normalize_beatoraja_separators(source), source);
    }
}
