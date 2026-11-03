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
        let mut entries = Vec::new();
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
            ensure!(entries.len() < control.limits.entries, "archive entry count exceeds limit");
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
            entries.push(ArchiveEntry {
                name: name(&header, directory)?,
                size: header.original_size,
                is_directory: directory,
            });
            members.push((header, offset));
            file.seek(SeekFrom::Start(end))?;
        }
        ensure!(!entries.is_empty(), "empty LZH archive");
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

fn name(header: &LhaHeader, directory: bool) -> Result<String> {
    let mut filename = header.filename.as_ref();
    let mut folders: &[u8] = &[];
    for extra in header.iter_extra() {
        match extra {
            [0x01, bytes @ ..] => filename = bytes,
            [0x02, bytes @ ..] => folders = bytes,
            _ => {}
        }
    }
    // 0xff separates LHA directory components. Decode CP932 before interpreting '\\'.
    let mut name = String::new();
    let folders = folders.strip_suffix(&[0xff]).unwrap_or(folders);
    if !folders.is_empty() {
        for component in folders.split(|byte| *byte == 0xff) {
            name.push_str(&safety::decode_name(component, false)?);
            name.push('/');
        }
    }
    name.push_str(&safety::decode_name(filename, false)?);
    safety::normalize_name(&name, directory)
}
