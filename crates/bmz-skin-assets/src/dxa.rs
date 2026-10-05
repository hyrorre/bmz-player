//! Legacy DXArchive (versions 1..=4) index and LZ decoding.
//! Format references: DXArchive 1.02 by Takumi Yamada, and GARbro's
//! ArcFormats/DxLib/ArcDX.cs (morkt, MIT; see /THIRD-PARTY-NOTICES.txt).

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use encoding_rs::{SHIFT_JIS, UTF_8};

const DEFAULT_KEY: [u8; 12] =
    [0x55, 0xaa, 0x20, 0x55, 0x55, 0x06, 0x55, 0xaa, 0x55, 0xd5, 0x7c, 0x66];
const MAX_INDEX: usize = 16 * 1024 * 1024;
const MAX_FILE: usize = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub name: String,
    offset: u64,
    size: usize,
    stored_size: usize,
    compressed: bool,
    key: [u8; 12],
}

struct Index {
    bytes: Vec<u8>,
    files: usize,
    directories: usize,
    data_start: u64,
    data_end: u64,
    version: u16,
    key: [u8; 12],
    encoding: &'static encoding_rs::Encoding,
}

pub(super) fn find_entry(path: &Path, requested: &str) -> Result<Entry> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut header = [0; 24];
    file.read_exact(&mut header).context("truncated DXA header")?;
    let key = if header[..2] == *b"DX" {
        [0; 12]
    } else if header[0] ^ 0xff == b'D' && header[1] ^ 0xff == b'X' {
        [0xff; 12]
    } else {
        DEFAULT_KEY
    };
    crypt(&mut header, 0, &key);
    ensure!(header[..2] == *b"DX", "unsupported DXA key or invalid signature");
    let version = u16::from_le_bytes([header[2], header[3]]);
    ensure!((1..=4).contains(&version), "unsupported DXA version {version} (supported: 1..=4)");
    let index_size = u32_at(&header, 4)? as usize;
    ensure!(index_size <= MAX_INDEX, "DXA index exceeds size limit");
    let data_start = u64::from(u32_at(&header, 8)?);
    let index_start = u64::from(u32_at(&header, 12)?);
    let header_len = if version >= 4 { 28 } else { 24 };
    ensure!(
        data_start >= header_len
            && data_start <= index_start
            && index_start.checked_add(index_size as u64).is_some_and(|end| end <= file_len),
        "invalid DXA index range"
    );
    let encoding = if version >= 4 {
        let mut code_page = [0; 4];
        file.read_exact(&mut code_page)?;
        crypt(&mut code_page, 24, &key);
        match u32::from_le_bytes(code_page) {
            932 => SHIFT_JIS,
            65001 => UTF_8,
            code => bail!("unsupported DXA code page {code}"),
        }
    } else {
        SHIFT_JIS
    };
    let mut bytes = vec![0; index_size];
    file.seek(SeekFrom::Start(index_start))?;
    file.read_exact(&mut bytes)?;
    crypt(&mut bytes, index_start, &key);
    let index = Index {
        bytes,
        files: u32_at(&header, 16)? as usize,
        directories: u32_at(&header, 20)? as usize,
        data_start,
        data_end: index_start,
        version,
        key,
        encoding,
    };
    ensure!(
        index.files <= index.directories && index.directories < index_size,
        "invalid DXA table offsets"
    );
    let mut pending = vec![(0usize, String::new(), 0usize)];
    let mut visited = HashSet::new();
    let mut total_entries = 0usize;
    let mut total_name_bytes = 0usize;
    while let Some((directory, prefix, depth)) = pending.pop() {
        ensure!(
            depth <= MAX_DEPTH && visited.insert(directory),
            "cyclic or excessive DXA directories"
        );
        let dir = index.directories.checked_add(directory).context("DXA directory overflow")?;
        let count = u32_at(&index.bytes, dir + 8)? as usize;
        total_entries = total_entries.checked_add(count).context("DXA entry count overflow")?;
        ensure!(total_entries <= MAX_ENTRIES, "DXA entry count exceeds limit");
        let start = index
            .files
            .checked_add(u32_at(&index.bytes, dir + 12)? as usize)
            .context("DXA file table overflow")?;
        let stride = if index.version >= 2 { 44 } else { 40 };
        ensure!(
            start >= index.files
                && start.checked_add(count * stride).is_some_and(|end| end <= index.directories),
            "invalid DXA file table range"
        );
        for position in (start..start + count * stride).step_by(stride) {
            let name = index.name(u32_at(&index.bytes, position)? as usize)?;
            let full_name = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            total_name_bytes += full_name.len();
            ensure!(
                full_name.len() <= 4096 && total_name_bytes <= MAX_INDEX,
                "DXA paths exceed size limit"
            );
            let address = u32_at(&index.bytes, position + 32)?;
            if u32_at(&index.bytes, position + 4)? & 0x10 != 0 {
                pending.push((address as usize, full_name, depth + 1));
            } else if full_name.eq_ignore_ascii_case(requested) {
                let size = u32_at(&index.bytes, position + 36)? as usize;
                let packed = if index.version >= 2 {
                    u32_at(&index.bytes, position + 40)?
                } else {
                    u32::MAX
                };
                let compressed = packed != u32::MAX;
                let stored_size = if compressed { packed as usize } else { size };
                let offset = index.data_start + u64::from(address);
                ensure!(
                    size <= MAX_FILE && stored_size <= MAX_FILE,
                    "DXA entry exceeds size limit"
                );
                ensure!(
                    offset.checked_add(stored_size as u64).is_some_and(|end| end <= index.data_end),
                    "invalid DXA entry range"
                );
                return Ok(Entry {
                    name: full_name,
                    offset,
                    size,
                    stored_size,
                    compressed,
                    key: index.key,
                });
            }
        }
    }
    bail!("DXA entry not found: {requested}")
}

impl Index {
    fn name(&self, position: usize) -> Result<String> {
        let length = self
            .bytes
            .get(position..position.checked_add(2).context("DXA name overflow")?)
            .context("truncated DXA name")?;
        let length = u16::from_le_bytes(length.try_into().unwrap()) as usize * 4;
        let original = position.checked_add(4 + length).context("DXA name overflow")?;
        let tail = self.bytes.get(original..self.files).context("invalid DXA name range")?;
        let end = tail.iter().position(|&b| b == 0).context("unterminated DXA name")?;
        ensure!(end <= 1024, "DXA filename exceeds limit");
        let (name, errors) = self.encoding.decode_without_bom_handling(&tail[..end]);
        ensure!(!errors, "invalid DXA filename encoding");
        validate_name(&name)?;
        Ok(name.into_owned())
    }
}

pub(super) fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0']),
        "invalid DXA filename: {name:?}"
    );
    Ok(())
}

pub(super) fn read_entry(path: &Path, entry: &Entry) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut data = vec![0; entry.stored_size];
    file.read_exact(&mut data).context("truncated DXA entry")?;
    crypt(&mut data, entry.offset, &entry.key);
    if entry.compressed { decompress(&data, entry.size) } else { Ok(data) }
}

fn crypt(bytes: &mut [u8], offset: u64, key: &[u8; 12]) {
    let start = (offset % 12) as usize;
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[(start + i) % 12];
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset.checked_add(4).context("DXA offset overflow")?;
    Ok(u32::from_le_bytes(
        bytes.get(offset..end).context("truncated DXA field")?.try_into().unwrap(),
    ))
}

fn decompress(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    ensure!(
        expected <= MAX_FILE
            && u32_at(data, 0)? as usize == expected
            && u32_at(data, 4)? as usize == data.len(),
        "invalid DXA compression header"
    );
    let (&escape, mut input) = data
        .get(8..)
        .and_then(|bytes| bytes.split_first())
        .context("truncated DXA compression header")?;
    let mut output = Vec::with_capacity(expected);
    while !input.is_empty() {
        let byte = take_byte(&mut input)?;
        if byte != escape {
            output.push(byte);
        } else {
            let mut code = take_byte(&mut input)?;
            if code == escape {
                output.push(escape);
            } else {
                if code > escape {
                    code -= 1;
                }
                let mut count = usize::from(code >> 3);
                if code & 4 != 0 {
                    count |= usize::from(take_byte(&mut input)?) << 5;
                }
                count += 4;
                let width = usize::from(code & 3) + 1;
                ensure!(width <= 3, "invalid DXA back-reference width");
                let mut distance = 0usize;
                for shift in 0..width {
                    distance |= usize::from(take_byte(&mut input)?) << (shift * 8);
                }
                distance += 1;
                ensure!(
                    distance <= output.len() && count <= expected.saturating_sub(output.len()),
                    "invalid DXA back-reference"
                );
                for _ in 0..count {
                    output.push(output[output.len() - distance]);
                }
            }
        }
        ensure!(output.len() <= expected, "DXA output exceeds declared size");
    }
    ensure!(output.len() == expected, "truncated DXA output");
    Ok(output)
}

fn take_byte(input: &mut &[u8]) -> Result<u8> {
    let (&byte, rest) = input.split_first().context("truncated DXA compressed stream")?;
    *input = rest;
    Ok(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(size: u32, payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::from(size.to_le_bytes());
        data.extend((payload.len() as u32 + 9).to_le_bytes());
        data.push(0xff);
        data.extend(payload);
        data
    }

    #[test]
    fn overlapping_back_references_and_escape_literals() {
        assert_eq!(decompress(&stream(8, &[b'a', 0xff, 0x18, 0]), 8).unwrap(), b"aaaaaaaa");
        assert_eq!(decompress(&stream(1, &[0xff, 0xff]), 1).unwrap(), [0xff]);
        assert_eq!(decompress(&stream(5, &[b'a', 0xff, 1, 0, 0]), 5).unwrap(), b"aaaaa");
        assert_eq!(decompress(&stream(5, &[b'a', 0xff, 2, 0, 0, 0]), 5).unwrap(), b"aaaaa");
        assert_eq!(decompress(&stream(37, &[b'a', 0xff, 4, 1, 0]), 37).unwrap(), vec![b'a'; 37]);
    }

    #[test]
    fn invalid_compressed_streams_are_rejected() {
        for payload in [&[0xff][..], &[0xff, 0, 0], &[b'a', 0xff, 0, 8], &[b'a', 0xff, 3, 0], b"ab"]
        {
            assert!(decompress(&stream(1, payload), 1).is_err());
        }
        assert!(decompress(&stream(u32::MAX, &[]), u32::MAX as usize).is_err());
    }
}
