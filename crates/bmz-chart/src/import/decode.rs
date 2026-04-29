use encoding_rs::SHIFT_JIS;

use super::error::ImportWarning;

pub fn decode_bms_text(bytes: &[u8], warnings: &mut Vec<ImportWarning>) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => {
            warnings.push(ImportWarning::EncodingFallback);
            let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
            if had_errors {
                warnings.push(ImportWarning::TextReplacementOccurred);
            }
            decoded.into_owned()
        }
    }
}
