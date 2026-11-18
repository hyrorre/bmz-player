use std::borrow::Cow;

use chardetng::EncodingDetector;
use encoding_rs::{BIG5, EUC_KR, GB18030, SHIFT_JIS, UTF_8, UTF_16BE, UTF_16LE};

use super::error::ImportWarning;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BmsTextEncoding {
    Utf8,
    Cp932,
    Ms949,
    Gb18030,
    Big5,
}

impl BmsTextEncoding {
    pub const ALL: [Self; 5] = [Self::Utf8, Self::Cp932, Self::Ms949, Self::Gb18030, Self::Big5];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Cp932 => "CP932",
            Self::Ms949 => "MS949",
            Self::Gb18030 => "GB18030",
            Self::Big5 => "Big5",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label.to_ascii_lowercase().as_str() {
            "utf-8" | "utf8" => Some(Self::Utf8),
            "cp932" | "ms932" | "shift_jis" | "shift-jis" | "windows-31j" => Some(Self::Cp932),
            "ms949" | "cp949" | "euc-kr" => Some(Self::Ms949),
            "gb18030" | "gbk" | "gb2312" | "cp936" => Some(Self::Gb18030),
            "big5" | "big5-hkscs" | "cp950" => Some(Self::Big5),
            _ => None,
        }
    }

    pub fn decode_strict(self, bytes: &[u8]) -> Option<Cow<'_, str>> {
        let encoding = match self {
            Self::Utf8 => UTF_8,
            Self::Cp932 => SHIFT_JIS,
            Self::Ms949 => EUC_KR,
            Self::Gb18030 => GB18030,
            Self::Big5 => BIG5,
        };
        encoding.decode_without_bom_handling_and_without_replacement(bytes)
    }
}

/// Additional strict decoders for text that already failed the UTF-8 and CP932 checks.
/// Used for BMS text and unmarked archive names. The guess never substitutes characters.
pub fn decode_cjk_fallback(bytes: &[u8]) -> Option<(BmsTextEncoding, Cow<'_, str>)> {
    use BmsTextEncoding::{Big5, Gb18030, Ms949};

    // Skip ASCII chart data before the first non-ASCII byte. Limit statistical
    // detection, then validate the selected decoder against the entire input.
    let start = bytes.iter().position(|byte| !byte.is_ascii())?;
    let sample = &bytes[start..];
    let mut detector = EncodingDetector::new();
    let max_sample = 64 * 1024;
    detector.feed(&sample[..sample.len().min(max_sample)], sample.len() <= max_sample);
    let (guess, assessed) = detector.guess_assess(None, false);
    let candidate = match guess.name() {
        "EUC-KR" => Some(Ms949),
        "GBK" => Some(Gb18030),
        "Big5" => Some(Big5),
        _ => None,
    };
    if assessed
        && let Some(encoding) = candidate
        && let Some(text) = encoding.decode_strict(bytes)
    {
        return Some((encoding, text));
    }
    // Retain the previous MS949 fallback when the statistical guess is unusable.
    // With two valid Chinese interpretations, leave the decision to an override.
    if let Some(text) = Ms949.decode_strict(bytes) {
        return Some((Ms949, text));
    }
    match (Gb18030.decode_strict(bytes), Big5.decode_strict(bytes)) {
        (Some(text), None) => Some((Gb18030, text)),
        (None, Some(text)) => Some((Big5, text)),
        _ => None,
    }
}

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

pub(super) fn decode_bms_source<'a>(
    path: &std::path::Path,
    (bytes, encoding): (&'a [u8], Option<BmsTextEncoding>),
    warnings: &mut Vec<ImportWarning>,
) -> Result<Cow<'a, str>, super::error::ImportError> {
    let Some(encoding) = encoding else { return Ok(decode_bms_text_cow(bytes, warnings)) };
    let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    encoding.decode_strict(body).ok_or_else(|| super::error::ImportError::Parse {
        path: path.to_path_buf(),
        message: format!("chart text is not valid {}", encoding.name()),
    })
}

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
    let bytes = if let Some(body) = bytes.strip_prefix(UTF8_BOM) {
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
    // Keep CP932 when it can decode the whole file. Short Japanese metadata can
    // score higher in a different encoding even when the CP932 decoding is correct.
    if had_errors && let Some((encoding, text)) = decode_cjk_fallback(bytes) {
        warnings.push(ImportWarning::CjkEncodingFallback(encoding));
        return text;
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
