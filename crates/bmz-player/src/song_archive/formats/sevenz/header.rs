//! Only the StreamsInfo needed to bound encoded-header decoding is parsed here.
//! Format: https://github.com/ip7z/7zip/blob/main/DOC/7zFormat.txt
use super::*;
use crate::song_archive::ArchiveLimits;
use anyhow::Context;

const MAX_DECODED_HEADER_BYTES: u64 = 64 << 20;
// Real header chains normally contain one compressor and at most a filter.
// Bound decoder nesting as well as dictionaries, including chains of Copy coders.
const MAX_HEADER_CODERS: usize = 32;

pub(super) fn preflight(reader: &mut (impl Read + Seek), limits: &ArchiveLimits) -> Result<()> {
    let mut start = [0; 32];
    reader.read_exact(&mut start)?;
    ensure!(&start[..6] == b"7z\xbc\xaf\x27\x1c" && start[6] == 0, "invalid 7z signature/version");
    // Reject unfinished/corrupt start headers: the library's recovery scan can
    // otherwise select a different encoded header which has not been checked.
    ensure!(
        checksum(&start[12..]) == u32::from_le_bytes(start[8..12].try_into()?),
        "invalid 7z start header CRC"
    );
    let offset = u64::from_le_bytes(start[12..20].try_into()?);
    let size = u64::from_le_bytes(start[20..28].try_into()?);
    ensure!(size <= limits.header_bytes, "7z header bytes exceed limit");
    let position = 32u64.checked_add(offset).context("7z header offset overflow")?;
    let end = position.checked_add(size).context("7z header size overflow")?;
    ensure!(end <= reader.seek(SeekFrom::End(0))?, "7z header is outside the archive");
    reader.seek(SeekFrom::Start(position))?;
    let size = usize::try_from(size)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size)?;
    bytes.resize(size, 0);
    reader.read_exact(&mut bytes)?;
    ensure!(
        checksum(&bytes) == u32::from_le_bytes(start[28..32].try_into()?),
        "invalid 7z next header CRC"
    );
    match bytes.first() {
        Some(0x17) => encoded(&bytes, position, limits),
        Some(0x01) | None => Ok(()), // Plain headers keep the existing decoder path.
        _ => anyhow::bail!("unsupported 7z header"),
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

struct Input<'a>(&'a [u8]);

impl<'a> Input<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        ensure!(length <= self.0.len(), "truncated 7z encoded header");
        let (value, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn expect(&mut self, id: u8) -> Result<()> {
        ensure!(self.byte()? == id, "invalid 7z encoded header structure");
        Ok(())
    }

    fn number(&mut self) -> Result<u64> {
        let first = self.byte()?;
        let mut value = 0;
        for index in 0..8 {
            let mask = 0x80 >> index;
            if first & mask == 0 {
                return Ok(value | (u64::from(first & (mask - 1)) << (index * 8)));
            }
            value |= u64::from(self.byte()?) << (index * 8);
        }
        Ok(value)
    }

    fn count(&mut self) -> Result<usize> {
        let count = usize::try_from(self.number()?)?;
        ensure!(count <= self.0.len(), "7z encoded header count exceeds available bytes");
        Ok(count)
    }

    fn digests(&mut self, count: usize) -> Result<Vec<bool>> {
        let flags = if self.byte()? == 0 { Some(self.take(count.div_ceil(8))?) } else { None };
        let defined: Vec<_> = (0..count)
            .map(|i| flags.is_none_or(|flags| flags[i / 8] & (0x80 >> (i % 8)) != 0))
            .collect();
        self.take(
            defined
                .iter()
                .filter(|defined| **defined)
                .count()
                .checked_mul(4)
                .context("7z CRC count overflow")?,
        )?;
        Ok(defined)
    }
}

struct Folder {
    coders: usize,
    final_output: usize,
    size: u64,
    has_crc: bool,
    streams: usize,
}

fn folder(input: &mut Input<'_>, dictionaries: &mut u64, limits: &ArchiveLimits) -> Result<Folder> {
    let coders = input.count()?;
    ensure!(
        (1..=MAX_HEADER_CODERS).contains(&coders),
        "7z encoded header coder count exceeds limit"
    );
    for _ in 0..coders {
        let flags = input.byte()?;
        ensure!(
            flags & 0xc0 == 0 && (1..=8).contains(&(flags & 0x0f)),
            "invalid 7z encoded header coder flags"
        );
        let method = input.take((flags & 0x0f) as usize)?;
        if flags & 0x10 != 0 {
            // The library's encoded-header path only supports simple chains.
            let inputs = input.number()?;
            let outputs = input.number()?;
            ensure!(inputs == 1 && outputs == 1, "multi-stream 7z header coders are unsupported");
        }
        let properties = if flags & 0x20 != 0 {
            let size = input.count()?;
            input.take(size)?
        } else {
            &[]
        };
        let dictionary = match method {
            [3, 1, 1] | [0x21] => super::dictionary_size(method, properties)?,
            [0] => {
                ensure!(properties.is_empty(), "invalid Copy header properties");
                0
            }
            [0x0a]
            | [0x0b]
            | [3, 3, 1, 3]
            | [3, 3, 2, 5]
            | [3, 3, 4, 1]
            | [3, 3, 5, 1]
            | [3, 3, 7, 1]
            | [3, 3, 8, 5] => {
                ensure!(
                    properties.is_empty() || properties.len() == 4,
                    "invalid BCJ header properties"
                );
                0
            }
            _ => anyhow::bail!("unsupported 7z encoded header coder: {method:x?}"),
        };
        *dictionaries =
            dictionaries.checked_add(dictionary).context("7z header dictionary overflow")?;
        ensure!(
            *dictionaries <= limits.dictionary_bytes,
            "7z encoded header dictionary bytes exceed limit"
        );
    }
    // Reject cyclic/disconnected bindings before the library can build a
    // chain which reuses or skips a decoder.
    let mut next = vec![None; coders];
    let mut bound_inputs = vec![false; coders];
    for _ in 1..coders {
        let input_index = input.count()?;
        let output_index = input.count()?;
        ensure!(input_index < coders && output_index < coders, "invalid 7z header binding index");
        ensure!(
            !bound_inputs[input_index] && next[output_index].is_none(),
            "duplicate 7z header binding"
        );
        bound_inputs[input_index] = true;
        next[output_index] = Some(input_index);
    }
    let mut current =
        bound_inputs.iter().position(|bound| !bound).context("missing 7z header packed stream")?;
    let mut visited = vec![false; coders];
    loop {
        ensure!(!visited[current], "cyclic 7z header coders");
        visited[current] = true;
        match next[current] {
            Some(index) => current = index,
            None => break,
        }
    }
    ensure!(visited.iter().all(|visited| *visited), "disconnected 7z header coders");
    Ok(Folder { coders, final_output: current, size: 0, has_crc: false, streams: 1 })
}

fn encoded(bytes: &[u8], position: u64, limits: &ArchiveLimits) -> Result<()> {
    let mut input = Input(bytes);
    input.expect(0x17)?;
    input.expect(0x06)?; // PackInfo
    let packed_position = input.number()?;
    let packed = input.count()?;
    ensure!(packed > 0, "missing 7z header packed streams");
    input.expect(0x09)?;
    let mut packed_end =
        32u64.checked_add(packed_position).context("7z header packed offset overflow")?;
    for _ in 0..packed {
        packed_end =
            packed_end.checked_add(input.number()?).context("7z header packed size overflow")?;
    }
    ensure!(packed_end <= position, "7z encoded header packed streams are outside the data area");
    let mut id = input.byte()?;
    if id == 0x0a {
        input.digests(packed)?;
        id = input.byte()?;
    }
    ensure!(id == 0, "invalid 7z header PackInfo");
    input.expect(0x07)?; // UnpackInfo
    input.expect(0x0b)?; // Folder
    let count = input.count()?;
    ensure!(
        count > 0 && count == packed && count <= limits.entries,
        "invalid 7z header folder count"
    );
    input.expect(0)?; // External folders are unsupported by the decoder too.
    let mut dictionaries = 0;
    let mut folders = Vec::new();
    for _ in 0..count {
        folders.push(folder(&mut input, &mut dictionaries, limits)?);
    }
    input.expect(0x0c)?; // CodersUnpackSize
    let decoded_limit = MAX_DECODED_HEADER_BYTES.min(limits.header_bytes);
    for folder in &mut folders {
        for index in 0..folder.coders {
            let size = input.number()?;
            ensure!(size <= decoded_limit, "7z decoded header bytes exceed limit");
            if index == folder.final_output {
                folder.size = size;
            }
        }
    }
    let mut id = input.byte()?;
    if id == 0x0a {
        for (folder, defined) in folders.iter_mut().zip(input.digests(count)?) {
            folder.has_crc = defined;
        }
        id = input.byte()?;
    }
    ensure!(id == 0, "invalid 7z header UnpackInfo");
    let mut id = input.byte()?;
    if id == 0x08 {
        // Optional SubStreamsInfo
        id = input.byte()?;
        if id == 0x0d {
            for folder in &mut folders {
                folder.streams = input.count()?;
            }
            id = input.byte()?;
        }
        let mut digests = 0usize;
        for folder in &folders {
            if id == 0x09 {
                let mut sum = 0u64;
                for _ in 1..folder.streams {
                    sum = sum
                        .checked_add(input.number()?)
                        .context("7z header sub-stream size overflow")?;
                }
                ensure!(sum <= folder.size, "invalid 7z header sub-stream sizes");
            } else {
                ensure!(folder.streams <= 1, "missing 7z header sub-stream sizes");
            }
            if folder.streams != 1 || !folder.has_crc {
                digests = digests
                    .checked_add(folder.streams)
                    .context("7z header digest count overflow")?;
            }
        }
        ensure!(digests <= bytes.len(), "7z header sub-stream count exceeds available bytes");
        if id == 0x09 {
            id = input.byte()?;
        }
        if id == 0x0a {
            input.digests(digests)?;
            id = input.byte()?;
        }
        ensure!(id == 0, "invalid 7z header SubStreamsInfo");
        id = input.byte()?;
    }
    ensure!(id == 0 && input.0.is_empty(), "invalid 7z encoded header ending");
    Ok(())
}

#[cfg(test)]
mod tests;
