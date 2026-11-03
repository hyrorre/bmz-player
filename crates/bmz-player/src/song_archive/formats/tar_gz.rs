use super::*;
use anyhow::Context;
use flate2::read::GzDecoder;

pub(in crate::song_archive) struct TarGz {
    file: CheckedFile,
}

impl TarGz {
    pub fn open(file: CheckedFile, control: &ArchiveControl) -> Result<(Self, Vec<ArchiveEntry>)> {
        let mut archive = Self { file };
        let mut entries = Vec::new();
        archive.visit(control, &mut |entry, _| {
            entries.push(entry);
            Ok(true)
        })?;
        Ok((archive, entries))
    }

    pub fn extract(
        &mut self,
        entries: &[ArchiveEntry],
        control: &ArchiveControl,
        selected: Selection<'_>,
        open: &mut dyn FnMut(&ArchiveEntry) -> Result<Box<dyn Write>>,
    ) -> Result<()> {
        let mut remaining = entries.iter().filter(|entry| selected.includes(entry)).count();
        let mut position = 0;
        self.visit(control, &mut |entry, reader| {
            ensure!(entries.get(position) == Some(&entry), "tar entry changed while reading");
            position += 1;
            if selected.includes(&entry) {
                let mut writer = open(&entry)?;
                std::io::copy(reader, &mut writer)?;
                writer.flush()?;
                remaining -= 1;
            }
            Ok(remaining != 0)
        })?;
        ensure!(remaining == 0, "tar omitted selected entries");
        Ok(())
    }

    fn visit(
        &mut self,
        control: &ArchiveControl,
        visit: &mut dyn FnMut(ArchiveEntry, &mut dyn Read) -> Result<bool>,
    ) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        // Bound all decompressed bytes, including metadata and tar padding.
        let limit = control
            .limits
            .extracted_bytes
            .saturating_add(control.limits.header_bytes)
            .saturating_add((control.limits.entries as u64).saturating_mul(1024))
            .saturating_add(1024 * 1024 + 1);
        let decoder = GzDecoder::new(&mut self.file).take(limit);
        let mut archive = tar::Archive::new(decoder);
        let mut long_name = None;
        let mut pax_name = None;
        let mut pax_size = None;
        let mut headers = 0u64;
        let mut total = 0u64;
        // Raw iteration lets us bound GNU/PAX metadata before allocating it.
        for (count, raw) in archive.entries()?.raw(true).enumerate() {
            control.check()?;
            ensure!(count < control.limits.entries, "archive entry count exceeds limit");
            let mut raw = raw?;
            let kind = raw.header().entry_type();
            headers = headers.saturating_add(512);
            ensure!(headers <= control.limits.header_bytes, "tar header bytes exceed limit");
            if kind.is_gnu_longname() || kind.is_pax_local_extensions() {
                headers = headers.saturating_add(raw.size());
                ensure!(headers <= control.limits.header_bytes, "tar header bytes exceed limit");
                let mut bytes = Vec::new();
                raw.read_to_end(&mut bytes)?;
                if kind.is_gnu_longname() {
                    ensure!(long_name.is_none(), "duplicate tar long name");
                    if bytes.last() == Some(&0) {
                        bytes.pop();
                    }
                    long_name = Some(safety::decode_name(&bytes, false)?);
                } else {
                    ensure!(pax_name.is_none() && pax_size.is_none(), "duplicate tar PAX header");
                    for (key, value) in pax_fields(&bytes)? {
                        match key {
                            "path" => pax_name = Some(std::str::from_utf8(value)?.to_owned()),
                            "size" => pax_size = Some(std::str::from_utf8(value)?.parse::<u64>()?),
                            key if key.starts_with("GNU.sparse") => {
                                anyhow::bail!("sparse tar is unsupported")
                            }
                            _ => {}
                        }
                    }
                }
                continue;
            }
            ensure!(
                kind.is_file() || kind.is_dir(),
                "tar links or special entries are unsupported"
            );
            let size = raw.size();
            if let Some(pax_size) = pax_size.take() {
                ensure!(pax_size == size, "tar PAX size overrides are unsupported");
            }
            ensure!(size <= control.limits.file_bytes, "archive entry exceeds the size limit");
            total = total.saturating_add(size);
            ensure!(
                total <= control.limits.extracted_bytes,
                "archive extracted bytes exceed limit"
            );
            let name = match pax_name.take().or_else(|| long_name.take()) {
                Some(name) => name,
                None => safety::decode_name(&raw.path_bytes(), false)?,
            };
            long_name = None;
            if kind.is_dir() && matches!(name.as_str(), "." | "./") && size == 0 {
                continue;
            }
            let entry = ArchiveEntry {
                name: safety::normalize_name(&name, kind.is_dir())?,
                size,
                is_directory: kind.is_dir(),
            };
            ensure!(
                !entry.is_chart() || size <= control.limits.chart_bytes,
                "archive chart bytes exceed limit"
            );
            if !visit(entry, &mut raw)? {
                return Ok(());
            }
        }
        ensure!(
            long_name.is_none() && pax_name.is_none() && pax_size.is_none(),
            "orphan tar extension header"
        );
        // Consume gzip's footer even after tar's end marker so CRC failures are visible.
        let trailing =
            std::io::copy(&mut archive.into_inner().take(1024 * 1024 + 1), &mut std::io::sink())
                .context("invalid gzip stream")?;
        ensure!(trailing <= 1024 * 1024, "excessive archive trailing data");
        Ok(())
    }
}

fn pax_fields(mut bytes: &[u8]) -> Result<Vec<(&str, &[u8])>> {
    let mut fields = Vec::new();
    while !bytes.is_empty() {
        let space = bytes.iter().position(|byte| *byte == b' ').context("invalid PAX field")?;
        let length: usize = std::str::from_utf8(&bytes[..space])?.parse()?;
        ensure!(length > space + 2 && length <= bytes.len(), "invalid PAX field length");
        let record = &bytes[space + 1..length];
        ensure!(record.last() == Some(&b'\n'), "invalid PAX field terminator");
        let equals = record.iter().position(|byte| *byte == b'=').context("invalid PAX key")?;
        fields
            .push((std::str::from_utf8(&record[..equals])?, &record[equals + 1..record.len() - 1]));
        bytes = &bytes[length..];
    }
    Ok(fields)
}
