//! bms-rs がまだ解釈しない Kaleid の HLN ヘッダだけを補完する。
//! RANDOM 解決後のテキストを使い、元ファイルのハッシュは変更しない。
pub(super) fn preprocess(text: &str) -> (String, bool, String) {
    let mut parsed = String::with_capacity(text.len());
    let mut markers = String::new();
    let mut hln_mode = false;
    for line in text.lines() {
        let header = line.trim_start().strip_prefix('#').map(str::trim_start);
        let (name, value) = header
            .map(|body| body.split_once(char::is_whitespace).unwrap_or((body, "")))
            .unwrap_or(("", ""));
        if name.eq_ignore_ascii_case("HLNOBJ") {
            markers.push_str("#LNOBJ ");
            markers.push_str(value);
            parsed.push('\n');
        } else if name.eq_ignore_ascii_case("LNMODE") && value.trim() == "4" {
            hln_mode = true;
            parsed.push_str("#LNMODE 1\n");
        } else {
            if name.eq_ignore_ascii_case("LNMODE") && matches!(value.trim(), "1" | "2" | "3") {
                hln_mode = false;
            }
            parsed.push_str(line);
            parsed.push('\n');
        }
        // Keep diagnostic line numbers aligned with the original source.
        markers.push('\n');
    }
    (parsed, hln_mode, markers)
}
