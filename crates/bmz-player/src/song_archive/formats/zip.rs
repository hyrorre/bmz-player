use super::*;
use ::zip::HasZipMetadata;

pub(super) fn open(
    mut file: CheckedFile,
    control: &ArchiveControl,
) -> Result<(::zip::ZipArchive<CheckedFile>, Vec<ArchiveEntry>)> {
    let count = preflight(&mut file, control)?;
    file.seek(SeekFrom::Start(0))?;
    let mut archive = ::zip::ZipArchive::new(file)?;
    ensure!(archive.offset() == 0, "self-extracting ZIP archives are unsupported");
    ensure!(archive.len() == count, "duplicate ZIP directory entries");
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = archive.by_index_raw(index)?;
        ensure!(!entry.encrypted(), "encrypted song archives are unsupported");
        ensure!(
            matches!(
                entry.compression(),
                ::zip::CompressionMethod::Stored | ::zip::CompressionMethod::Deflated
            ),
            "unsupported ZIP compression method"
        );
        let directory = entry.is_dir();
        if let Some(mode) = entry.unix_mode() {
            safety::regular_mode(mode, directory)?;
        }
        let name = safety::decode_name(entry.name_raw(), entry.get_metadata().is_utf8)?;
        entries.push(ArchiveEntry {
            name: safety::normalize_name(&name, directory)?,
            size: entry.size(),
            is_directory: directory,
        });
    }
    Ok((archive, entries))
}

// Bound central-directory allocation before ZipArchive parses it, including ZIP64.
fn preflight(file: &mut CheckedFile, control: &ArchiveControl) -> Result<usize> {
    let len = file.file.metadata()?.len();
    let tail_len = len.min(65_557) as usize;
    file.seek(SeekFrom::Start(len - tail_len as u64))?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail)?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16_at(&tail, i + 20) as usize == tail.len()
        })
        .ok_or_else(|| anyhow::anyhow!("ZIP end record not found"))?;
    ensure!(
        u16_at(&tail, end + 4) == 0 && u16_at(&tail, end + 6) == 0,
        "split ZIP archives are unsupported"
    );
    ensure!(
        u16_at(&tail, end + 8) == u16_at(&tail, end + 10),
        "split ZIP archives are unsupported"
    );
    let mut count = u16_at(&tail, end + 10) as u64;
    let mut size = u32_at(&tail, end + 12) as u64;
    let mut offset = u32_at(&tail, end + 16) as u64;
    let end_offset = len - tail_len as u64 + end as u64;
    if count == u16::MAX as u64 || size == u32::MAX as u64 || offset == u32::MAX as u64 {
        ensure!(end >= 20 && tail[end - 20..].starts_with(b"PK\x06\x07"), "ZIP64 locator missing");
        let locator = &tail[end - 20..end];
        ensure!(
            u32_at(locator, 4) == 0 && u32_at(locator, 16) == 1,
            "split ZIP64 archives are unsupported"
        );
        file.seek(SeekFrom::Start(u64_at(locator, 8)))?;
        let mut record = [0; 56];
        file.read_exact(&mut record)?;
        ensure!(
            record.starts_with(b"PK\x06\x06") && u64_at(&record, 4) <= control.limits.header_bytes,
            "invalid ZIP64 end record"
        );
        ensure!(
            u32_at(&record, 16) == 0
                && u32_at(&record, 20) == 0
                && u64_at(&record, 24) == u64_at(&record, 32),
            "split ZIP64 archives are unsupported"
        );
        count = u64_at(&record, 32);
        size = u64_at(&record, 40);
        offset = u64_at(&record, 48);
    }
    ensure!(count <= control.limits.entries as u64, "archive entry count exceeds limit");
    ensure!(
        size <= control.limits.header_bytes
            && offset.checked_add(size).is_some_and(|end| end <= end_offset),
        "ZIP header bytes exceed limit or range"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut consumed = 0u64;
    for _ in 0..count {
        let mut header = [0; 46];
        file.read_exact(&mut header)?;
        ensure!(header.starts_with(b"PK\x01\x02"), "invalid ZIP central directory");
        ensure!(u16_at(&header, 34) == 0, "split ZIP archives are unsupported");
        let extra =
            u16_at(&header, 28) as u64 + u16_at(&header, 30) as u64 + u16_at(&header, 32) as u64;
        consumed = consumed
            .checked_add(46 + extra)
            .ok_or_else(|| anyhow::anyhow!("ZIP directory size overflow"))?;
        ensure!(consumed <= size, "ZIP central directory exceeds declared size");
        file.seek(SeekFrom::Current(extra as i64))?;
    }
    Ok(count as usize)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
