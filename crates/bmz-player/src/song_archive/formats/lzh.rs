use super::*;
use delharc::{
    LhaHeader,
    decode::{DecoderAny, LhaDecodeReader},
};

pub(in crate::song_archive) struct Lzh {
    file: CheckedFile,
    members: Vec<(LhaHeader, u64)>,
}

impl Lzh {
    pub fn open(
        mut file: CheckedFile,
        control: &ArchiveControl,
    ) -> Result<(Self, Vec<ArchiveEntry>)> {
        let file_size = file.file.metadata()?.len();
        let mut members = Vec::new();
        let mut header_bytes = 0u64;
        loop {
            control.check()?;
            let before = file.stream_position()?;
            if before == file_size {
                break;
            }
            ensure!(header_bytes < control.limits.header_bytes, "LZH header bytes exceed limit");
            let header = LhaHeader::read(
                &mut (&mut file).take(control.limits.header_bytes.saturating_sub(header_bytes)),
            )?;
            header_bytes = header_bytes.saturating_add(file.stream_position()? - before);
            ensure!(header_bytes <= control.limits.header_bytes, "LZH header bytes exceed limit");
            let Some(header) = header else { break };
            ensure!(members.len() < control.limits.entries, "archive entry count exceeds limit");
            let directory = header.is_directory();
            ensure!(!directory || header.compressed_size == 0, "LZH directory carries data");
            ensure!(
                !header.iter_extra().any(|extra| extra.first() == Some(&0x39)),
                "split LZH archives are unsupported"
            );
            if let Some(mode) = header.parse_unix_permissions() {
                safety::regular_mode(mode.bits() as u32, directory)?;
            }
            let dictionary = match &header.compression {
                b"-lh1-" | b"-lh4-" | b"-lz5-" => 4096,
                b"-lh5-" => 8192,
                b"-lh6-" => 32768,
                b"-lh7-" => 65536,
                b"-lhx-" => 512 * 1024,
                b"-lzs-" => 2048,
                _ => 0,
            };
            ensure!(
                dictionary <= control.limits.dictionary_bytes,
                "LZH dictionary bytes exceed limit"
            );
            let mut decoder = LhaDecodeReader::<&mut CheckedFile>::default();
            decoder.begin_with_header_and_decoder(
                header.clone(),
                DecoderAny::new_from_header(&header, &mut file),
            );
            ensure!(
                directory || decoder.is_decoder_supported(),
                "unsupported LZH compression method"
            );
            drop(decoder);
            let offset = file.stream_position()?;
            let end = offset
                .checked_add(header.compressed_size)
                .filter(|end| *end <= file_size)
                .ok_or_else(|| anyhow::anyhow!("truncated LZH member"))?;
            members.push((header, offset));
            file.seek(SeekFrom::Start(end))?;
        }
        ensure!(!members.is_empty(), "empty LZH archive");
        let names = safety::LegacyNames::detect(members.iter().flat_map(|(header, _)| {
            let (folders, filename) = raw_name(header);
            folders.chain([filename]).map(|bytes| (bytes, false))
        }));
        let entries = members
            .iter()
            .map(|(header, _)| {
                let directory = header.is_directory();
                Ok(ArchiveEntry {
                    name: name(header, directory, &names)?,
                    size: header.original_size,
                    is_directory: directory,
                })
            })
            .collect::<Result<_>>()?;
        Ok((Self { file, members }, entries))
    }

    pub fn extract(
        &mut self,
        entries: &[ArchiveEntry],
        control: &ArchiveControl,
        selected: Selection<'_>,
        open: &mut dyn FnMut(&ArchiveEntry) -> Result<Box<dyn Write>>,
    ) -> Result<()> {
        for (entry, (header, offset)) in entries.iter().zip(&self.members) {
            if !selected.includes(entry) {
                continue;
            }
            control.check()?;
            self.file.seek(SeekFrom::Start(*offset))?;
            let mut decoder = LhaDecodeReader::<&mut CheckedFile>::default();
            decoder.begin_with_header_and_decoder(
                header.clone(),
                DecoderAny::new_from_header(header, &mut self.file),
            );
            let mut writer = open(entry)?;
            std::io::copy(&mut decoder, &mut writer)?;
            decoder.crc_check()?;
            writer.flush()?;
        }
        Ok(())
    }
}

/// Raw directory components and filename. 0xff separates LHA directory
/// components, so names are decoded before interpreting '\\'.
fn raw_name(header: &LhaHeader) -> (impl Iterator<Item = &[u8]>, &[u8]) {
    let mut filename = header.filename.as_ref();
    let mut folders: &[u8] = &[];
    for extra in header.iter_extra() {
        match extra {
            [0x01, bytes @ ..] => filename = bytes,
            [0x02, bytes @ ..] => folders = bytes,
            _ => {}
        }
    }
    let folders = folders.strip_suffix(&[0xff]).unwrap_or(folders);
    let components =
        (!folders.is_empty()).then(|| folders.split(|byte| *byte == 0xff)).into_iter().flatten();
    (components, filename)
}

fn name(header: &LhaHeader, directory: bool, names: &safety::LegacyNames) -> Result<String> {
    let (folders, filename) = raw_name(header);
    let mut name = String::new();
    for component in folders {
        name.push_str(&names.decode(component, false)?);
        name.push('/');
    }
    name.push_str(&names.decode(filename, false)?);
    safety::normalize_name(&name, directory)
}
