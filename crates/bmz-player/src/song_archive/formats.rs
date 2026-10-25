mod sevenz;
mod zip;

use super::{ArchiveControl, ArchiveEntry, CheckedFile, Selection, safety};
use anyhow::{Result, ensure};
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) enum Backend {
    Zip(::zip::ZipArchive<CheckedFile>),
    Rar(rars::Archive),
    SevenZ(Box<sevenz_rust2::ArchiveReader<CheckedFile>>),
}

impl Backend {
    pub(super) fn open(
        mut file: CheckedFile,
        control: &ArchiveControl,
    ) -> Result<(Self, Vec<ArchiveEntry>)> {
        let mut magic = [0; 8];
        let len = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;
        if magic[..len].starts_with(b"PK\x03\x04") || magic[..len].starts_with(b"PK\x05\x06") {
            let (archive, entries) = zip::open(file, control)?;
            Ok((Self::Zip(archive), entries))
        } else if magic[..len].starts_with(b"Rar!\x1a\x07") {
            let archive =
                rars::ArchiveReader::read_reader_with_options(file, rar_options(control))?;
            ensure!(archive.sfx_offset() == 0, "self-extracting archives are unsupported");
            let volume = match &archive {
                rars::Archive::Rar13(archive) => archive.main.is_volume(),
                rars::Archive::Rar15To40(archive) => archive.main.is_volume(),
                rars::Archive::Rar50Plus(archive) => archive.main.is_volume(),
                _ => anyhow::bail!("unsupported RAR archive version"),
            };
            ensure!(!volume, "split song archives are unsupported");
            let mut entries = Vec::new();
            for member in archive.members() {
                let meta = &member.meta;
                ensure!(!meta.is_encrypted, "encrypted song archives are unsupported");
                ensure!(
                    !meta.is_split_before && !meta.is_split_after,
                    "split song archives are unsupported"
                );
                ensure!(!meta.is_redirection, "archive links are unsupported");
                if matches!(meta.attr_source(), rars::AttrSource::Unix) {
                    safety::regular_mode(meta.file_attr as u32, meta.is_directory)?;
                } else {
                    ensure!(meta.file_attr & 0x400 == 0, "archive reparse points are unsupported");
                }
                entries.push(ArchiveEntry {
                    name: safety::normalize_name(
                        &safety::decode_name(&meta.name, member.name_is_unicode())?,
                        meta.is_directory,
                    )?,
                    size: meta.unpacked_size,
                    is_directory: meta.is_directory,
                });
            }
            Ok((Self::Rar(archive), entries))
        } else if magic[..len].starts_with(b"7z\xbc\xaf\x27\x1c") {
            let (archive, entries) = sevenz::open(file, control)?;
            Ok((Self::SevenZ(Box::new(archive)), entries))
        } else {
            anyhow::bail!("unsupported song archive signature (SFX is unsupported)")
        }
    }

    /// Whether a member's data depends on decoding the members before it.
    pub(super) fn solid(&self) -> bool {
        match self {
            Self::Zip(_) => false,
            // The same conservative test rars applies before refusing to skip a member.
            Self::Rar(rars::Archive::Rar13(archive)) => archive.main.is_solid(),
            Self::Rar(rars::Archive::Rar15To40(archive)) => {
                archive.main.is_solid() || archive.files().any(|file| file.is_solid())
            }
            Self::Rar(rars::Archive::Rar50Plus(archive)) => {
                archive.main.is_solid()
                    || archive.files().any(|file| file.compression_info & 0x40 != 0)
            }
            Self::Rar(_) => true,
            Self::SevenZ(archive) => {
                let mut seen = std::collections::HashSet::new();
                archive
                    .archive()
                    .stream_map
                    .file_block_index
                    .iter()
                    .flatten()
                    .any(|block| !seen.insert(*block))
            }
        }
    }

    /// Visits the selected entries. Unselected members are skipped where the format allows,
    /// and reading stops once every selected entry has been written.
    pub(super) fn extract(
        &mut self,
        entries: &[ArchiveEntry],
        control: &ArchiveControl,
        selected: Selection<'_>,
        open: &mut dyn FnMut(&ArchiveEntry) -> Result<Box<dyn Write>>,
    ) -> Result<()> {
        let mut remaining = entries.iter().filter(|entry| selected.includes(entry)).count();
        let solid = self.solid();
        match self {
            Self::Zip(archive) => {
                for (index, entry) in entries.iter().enumerate() {
                    if remaining == 0 {
                        break;
                    }
                    control.check()?;
                    if selected.includes(entry) {
                        remaining -= 1;
                        let mut source = archive.by_index(index)?;
                        let mut writer = open(entry)?;
                        std::io::copy(&mut source, &mut writer)?;
                        writer.flush()?;
                    }
                }
            }
            Self::Rar(archive) => {
                let mut index = 0;
                archive.extract_with_control(rar_options(control), |_| {
                    let result = (|| -> Result<_> {
                        control.check()?;
                        if remaining == 0 {
                            return Ok(rars::ExtractionDecision::Stop);
                        }
                        let entry = entries
                            .get(index)
                            .ok_or_else(|| anyhow::anyhow!("RAR entry index changed"))?;
                        index += 1;
                        if entry.is_directory {
                            return Ok(rars::ExtractionDecision::Skip);
                        }
                        if selected.includes(entry) {
                            remaining -= 1;
                        } else if !solid {
                            return Ok(rars::ExtractionDecision::Skip);
                        }
                        // Unselected solid members are still decoded to keep the history.
                        Ok(rars::ExtractionDecision::Extract(open(entry)?))
                    })();
                    result.map_err(|error| rars::Error::from(std::io::Error::other(error)))
                })?;
            }
            Self::SevenZ(archive) => {
                // The library visits solid streams before streamless entries, not index order.
                let names: std::collections::HashMap<_, _> =
                    entries.iter().map(|entry| (entry.name.as_str(), entry)).collect();
                // A block can be abandoned once none of its remaining entries is selected.
                let mut block_of = std::collections::HashMap::new();
                let mut pending = std::collections::HashMap::<usize, usize>::new();
                let info = archive.archive();
                for (file_index, raw) in info.files.iter().enumerate() {
                    let name = safety::normalize_name(&raw.name, raw.is_directory)?;
                    let block = info.stream_map.file_block_index[file_index];
                    if let (Some(block), Some(entry)) = (block, names.get(name.as_str()))
                        && selected.includes(entry)
                    {
                        *pending.entry(block).or_default() += 1;
                    }
                    block_of.insert(name, block);
                }
                let mut visited = std::collections::HashSet::new();
                archive.for_each_entries(|raw, source| {
                    let result = (|| -> Result<bool> {
                        control.check()?;
                        if remaining == 0 {
                            return Ok(false);
                        }
                        let name = safety::normalize_name(&raw.name, raw.is_directory)?;
                        let entry = names
                            .get(name.as_str())
                            .ok_or_else(|| anyhow::anyhow!("7z entry index changed"))?;
                        let block = block_of.get(&name).copied().flatten();
                        ensure!(visited.insert(name), "7z entry visited twice");
                        if entry.is_directory {
                            return Ok(true);
                        }
                        if selected.includes(entry) {
                            remaining -= 1;
                            if let Some(count) = block.and_then(|block| pending.get_mut(&block)) {
                                *count -= 1;
                            }
                        } else {
                            match block {
                                // Streamless entries have nothing to decode.
                                None => return Ok(true),
                                // Returning false leaves the rest of this block undecoded;
                                // the next block or the streamless entries follow.
                                Some(block) if pending.get(&block).copied().unwrap_or(0) == 0 => {
                                    return Ok(false);
                                }
                                // A later entry of this solid block is selected.
                                Some(_) => {}
                            }
                        }
                        let mut writer = open(entry)?;
                        std::io::copy(source, &mut writer)?;
                        writer.flush()?;
                        Ok(true)
                    })();
                    result.map_err(|error| sevenz_rust2::Error::from(std::io::Error::other(error)))
                })?;
            }
        }
        ensure!(remaining == 0, "archive extraction omitted selected entries");
        Ok(())
    }
}

fn rar_options(control: &ArchiveControl) -> rars::ArchiveReadOptions<'_> {
    let limits = &control.limits;
    rars::ArchiveReadOptions::new()
        .with_max_header_count(limits.entries as u64 + 1024)
        .with_max_header_bytes(limits.header_bytes)
        .with_max_reader_workspace_bytes(limits.reader_workspace_bytes)
        .with_max_member_output_bytes(limits.file_bytes)
        .with_max_total_output_bytes(limits.extracted_bytes)
        .with_rar50_dictionary_size_limit(limits.dictionary_bytes)
        .with_rar50_buffered_decode_limit(limits.reader_workspace_bytes)
}
