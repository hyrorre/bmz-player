use std::borrow::Cow;

use encoding_rs::{EUC_KR, SHIFT_JIS, UTF_16BE, UTF_16LE};

use super::error::ImportWarning;

#[derive(Clone, Copy)]
struct WideEncoding {
    bom: &'static [u8],
    big_endian: bool,
}

// UTF-32LE's BOM starts with the same bytes as UTF-16LE's.
const WIDE_ENCODINGS: [WideEncoding; 4] = [
    WideEncoding { bom: b"\x00\x00\xFE\xFF", big_endian: true },
    WideEncoding { bom: b"\xFF\xFE\x00\x00", big_endian: false },
    WideEncoding { bom: b"\xFE\xFF", big_endian: true },
    WideEncoding { bom: b"\xFF\xFE", big_endian: false },
];

pub fn decode_bms_text(bytes: &[u8], warnings: &mut Vec<ImportWarning>) -> String {
    decode_bms_text_cow(bytes, warnings).into_owned()
}

pub(super) fn decode_bms_text_cow<'a>(
    bytes: &'a [u8],
    warnings: &mut Vec<ImportWarning>,
) -> Cow<'a, str> {
    let bytes = if let Some(body) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        // Some charts have a UTF-8 BOM before a CP932 body.
        body
    } else {
        for encoding in WIDE_ENCODINGS {
            if let Some(body) = bytes.strip_prefix(encoding.bom) {
                return encoding.decode(body, warnings);
            }
        }

        if bytes.contains(&0)
            && let Some(encoding) =
                WIDE_ENCODINGS.into_iter().find(|encoding| encoding.has_bms_command(bytes))
        {
            return encoding.decode(bytes, warnings);
        }
        bytes
    };

    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }

    let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
    // Keep CP932 when both encodings can decode the same bytes. The WHATWG EUC-KR
    // decoder includes the MS949 extensions.
    if had_errors
        && let Some(korean) = EUC_KR.decode_without_bom_handling_and_without_replacement(bytes)
    {
        warnings.push(ImportWarning::Ms949Fallback);
        return korean;
    }
    warnings.push(ImportWarning::EncodingFallback);
    if had_errors {
        warnings.push(ImportWarning::TextReplacementOccurred);
    }
    decoded
}

impl WideEncoding {
    fn has_bms_command(self, bytes: &[u8]) -> bool {
        let width = self.bom.len();
        let ascii = |unit: &[u8]| {
            let (&value, padding) = if self.big_endian {
                unit.split_last().unwrap()
            } else {
                unit.split_first().unwrap()
            };
            (value.is_ascii() && padding.iter().all(|&byte| byte == 0)).then_some(value)
        };

        // Look for a line-start BMS command aligned to this encoding's code units.
        let mut units = bytes.chunks_exact(width);
        let mut line_start = true;
        while let Some(unit) = units.next() {
            match ascii(unit) {
                Some(b'#') if line_start => {
                    if units
                        .clone()
                        .next()
                        .and_then(ascii)
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    {
                        return true;
                    }
                    line_start = false;
                }
                Some(b'\r' | b'\n') => line_start = true,
                Some(b' ' | b'\t') => {}
                _ => line_start = false,
            }
        }
        false
    }

    fn decode<'a>(self, bytes: &'a [u8], warnings: &mut Vec<ImportWarning>) -> Cow<'a, str> {
        let (decoded, had_errors) = if self.bom.len() == 2 {
            let encoding = if self.big_endian { UTF_16BE } else { UTF_16LE };
            encoding.decode_without_bom_handling(bytes)
        } else {
            let read_unit = if self.big_endian { u32::from_be_bytes } else { u32::from_le_bytes };
            let (units, tail) = bytes.as_chunks::<4>();
            let mut text = String::with_capacity(units.len());
            let mut had_errors = false;
            for &unit in units {
                text.push(char::from_u32(read_unit(unit)).unwrap_or_else(|| {
                    had_errors = true;
                    char::REPLACEMENT_CHARACTER
                }));
            }
            if !tail.is_empty() {
                had_errors = true;
                text.push(char::REPLACEMENT_CHARACTER);
            }
            (Cow::Owned(text), had_errors)
        };
        if had_errors {
            warnings.push(ImportWarning::TextReplacementOccurred);
        }
        decoded
    }
}
