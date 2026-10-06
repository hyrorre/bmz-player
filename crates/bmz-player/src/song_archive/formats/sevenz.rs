use super::*;

pub(super) fn open(
    mut file: CheckedFile,
    control: &ArchiveControl,
) -> Result<(sevenz_rust2::ArchiveReader<CheckedFile>, Vec<ArchiveEntry>)> {
    let mut start = [0; 32];
    file.read_exact(&mut start)?;
    let header_size = u64::from_le_bytes(start[20..28].try_into().unwrap());
    ensure!(header_size <= control.limits.header_bytes, "7z header bytes exceed limit");
    file.seek(SeekFrom::Start(0))?;
    // The public decoder API decodes a compressed header before exposing coder properties.
    // Body dictionaries are bounded below; this is not a strict header-memory sandbox.
    let mut reader = sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty())?;
    reader.set_thread_count(1);
    let archive = reader.archive();
    ensure!(archive.files.len() <= control.limits.entries, "archive entry count exceeds limit");
    for block in &archive.blocks {
        ensure!(
            block.get_unpack_size() <= control.limits.extracted_bytes,
            "7z block bytes exceed limit"
        );
        let mut dictionaries = 0u64;
        for coder in &block.coders {
            ensure!(
                block.get_unpack_size_for_coder(coder) <= control.limits.extracted_bytes,
                "7z intermediate bytes exceed limit"
            );
            dictionaries = dictionaries
                .checked_add(dictionary_size(coder.encoder_method_id(), coder.properties())?)
                .ok_or_else(|| anyhow::anyhow!("7z dictionary size overflow"))?;
        }
        ensure!(
            dictionaries <= control.limits.dictionary_bytes,
            "7z dictionary bytes exceed limit"
        );
    }
    let entries = archive
        .files
        .iter()
        .map(|entry| {
            ensure!(!entry.is_anti_item, "7z anti-items are unsupported");
            if entry.has_windows_attributes {
                ensure!(
                    entry.windows_attributes & 0x400 == 0,
                    "archive reparse points are unsupported"
                );
                safety::regular_mode(entry.windows_attributes >> 16, entry.is_directory)?;
            }
            Ok(ArchiveEntry {
                name: safety::normalize_name(&entry.name, entry.is_directory)?,
                size: entry.size,
                is_directory: entry.is_directory,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((reader, entries))
}

pub(in crate::song_archive) fn dictionary_size(method: &[u8], properties: &[u8]) -> Result<u64> {
    match method {
        [3, 1, 1] | [3, 4, 1] => {
            ensure!(properties.len() == 5, "invalid LZMA/PPMd properties");
            let size = u32::from_le_bytes(properties[1..5].try_into().unwrap()) as u64;
            Ok(if method == [3, 1, 1] { size.max(4096) } else { size })
        }
        [0x21] => {
            ensure!(properties.len() == 1 && properties[0] <= 40, "invalid LZMA2 properties");
            let value = properties[0];
            Ok(if value == 40 {
                u32::MAX as u64
            } else {
                (2 | (value as u64 & 1)) << (value / 2 + 11)
            })
        }
        [6, 0xf1, 7, 1] => anyhow::bail!("encrypted song archives are unsupported"),
        _ => Ok(0),
    }
}
