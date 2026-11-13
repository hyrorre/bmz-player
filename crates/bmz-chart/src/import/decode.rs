use std::borrow::Cow;

use encoding_rs::SHIFT_JIS;

use super::error::ImportWarning;

pub fn decode_bms_text(bytes: &[u8], warnings: &mut Vec<ImportWarning>) -> String {
    decode_bms_text_cow(bytes, warnings).into_owned()
}

pub(super) fn decode_bms_text_cow<'a>(
    bytes: &'a [u8],
    warnings: &mut Vec<ImportWarning>,
) -> Cow<'a, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => {
            warnings.push(ImportWarning::EncodingFallback);
            let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
            if had_errors {
                warnings.push(ImportWarning::TextReplacementOccurred);
            }
            decoded
        }
    }
}
