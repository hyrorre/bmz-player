use std::fmt::Write;

use anyhow::Result;
use rusqlite::Connection;
use rusqlite::types::{FromSqlError, Type};

pub fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", "-65536")?;
    conn.pragma_update(None, "mmap_size", "268435456")?;
    Ok(())
}

/// Encode a byte slice as a lowercase hex string.
///
/// 全テーブルのハッシュ列（`charts.md5/sha256`, `chart_files.md5/sha256`,
/// `score.db.*.chart_sha256`）は小文字 hex TEXT で保存する。
/// この関数は SQLite への bind 直前で `[u8;16]` / `[u8;32]` を変換するためのもの。
pub fn hash_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Decode a lowercase hex string into a fixed-size byte array.
///
/// 行マッパーから呼ぶ前提で、長さ不一致・不正文字は `rusqlite::Error` として伝播する。
pub fn hex_to_hash<const N: usize>(s: &str) -> rusqlite::Result<[u8; N]> {
    if s.len() != N * 2 {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(FromSqlError::Other(
                format!("expected hex string of length {}, got {}", N * 2, s.len()).into(),
            )),
        ));
    }
    let mut out = [0_u8; N];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(c: u8) -> rusqlite::Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            Box::new(FromSqlError::Other(
                format!("invalid hex character: {:?}", c as char).into(),
            )),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_16() {
        let bytes: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let hex = hash_to_hex(&bytes);
        assert_eq!(hex, "00112233445566778899aabbccddeeff");
        assert_eq!(hex_to_hash::<16>(&hex).unwrap(), bytes);
    }

    #[test]
    fn round_trip_32() {
        let bytes: [u8; 32] = [7; 32];
        let hex = hash_to_hex(&bytes);
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c == '0' || c == '7'));
        assert_eq!(hex_to_hash::<32>(&hex).unwrap(), bytes);
    }

    #[test]
    fn decode_rejects_wrong_length() {
        assert!(hex_to_hash::<16>("00").is_err());
        assert!(hex_to_hash::<16>("00112233445566778899aabbccddeeff00").is_err());
    }

    #[test]
    fn decode_rejects_invalid_char() {
        assert!(hex_to_hash::<16>("zz112233445566778899aabbccddeeff").is_err());
    }

    #[test]
    fn decode_accepts_uppercase_but_encode_is_lowercase() {
        let upper = "00112233445566778899AABBCCDDEEFF";
        let bytes = hex_to_hash::<16>(upper).unwrap();
        assert_eq!(hash_to_hex(&bytes), "00112233445566778899aabbccddeeff");
    }
}
