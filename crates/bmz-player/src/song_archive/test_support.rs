use super::*;
use bmz_song_resource::ArchiveFormat;

pub const CHART: &[u8] = b"#TITLE Archive Fixture\n#BPM 120\n#WAV01 kick.wav\n#00011:01\n";
pub const FORMATS: [ArchiveFormat; 5] = [
    ArchiveFormat::Zip,
    ArchiveFormat::Rar,
    ArchiveFormat::SevenZ,
    ArchiveFormat::TarGz,
    ArchiveFormat::Lzh,
];

pub fn archive(format: ArchiveFormat) -> Vec<u8> {
    let files: &[(&str, &[u8])] = &[("song/chart.bms", CHART), ("song/kick.wav", b"WAVE fixture")];
    match format {
        ArchiveFormat::Zip => {
            let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name, bytes) in files {
                archive.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                archive.write_all(bytes).unwrap();
            }
            archive.finish().unwrap().into_inner()
        }
        ArchiveFormat::Rar => include_bytes!("fixtures/rar4-solid.rar").to_vec(),
        ArchiveFormat::SevenZ => {
            let root = tempfile::tempdir().unwrap();
            let folder = root.path().join("source");
            for (name, bytes) in files {
                let path = folder.join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, bytes).unwrap();
            }
            let archive = root.path().join("test.7z");
            sevenz_rust2::compress_to_path(&folder, &archive).unwrap();
            std::fs::read(archive).unwrap()
        }
        ArchiveFormat::TarGz => {
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            for (name, bytes) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive.append_data(&mut header, name, *bytes).unwrap();
            }
            archive.into_inner().unwrap().finish().unwrap()
        }
        ArchiveFormat::Lzh => lzh(
            &files.iter().map(|(name, bytes)| (name.as_bytes(), *bytes)).collect::<Vec<_>>(),
            false,
        ),
    }
}

/// BMZ-owned level-0 LHA data; LH5 uses single-symbol Huffman blocks.
pub fn lzh(files: &[(&[u8], &[u8])], lh5: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in files {
        let compressed = if lh5 { lh5_data(data) } else { data.to_vec() };
        let mut header = vec![0, 0];
        header.extend_from_slice(if lh5 { b"-lh5-" } else { b"-lh0-" });
        header.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&[0x20, 0, name.len() as u8]);
        header.extend_from_slice(name);
        let mut crc = 0u16;
        for byte in *data {
            crc ^= *byte as u16;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xa001u16.wrapping_mul(crc & 1));
            }
        }
        header.extend_from_slice(&crc.to_le_bytes());
        header[0] = (header.len() - 2) as u8;
        header[1] = header[2..].iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        out.extend_from_slice(&header);
        out.extend_from_slice(&compressed);
    }
    out.push(0);
    out
}

fn lh5_data(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::new();
    for byte in data {
        for (value, width) in
            [(1u16, 16), (0, 5), (0, 5), (0, 9), (*byte as u16, 9), (0, 4), (0, 4)]
        {
            for bit in (0..width).rev() {
                bits.push((value >> bit) as u8 & 1);
            }
        }
    }
    bits.chunks(8)
        .map(|chunk| chunk.iter().enumerate().fold(0, |byte, (i, bit)| byte | (bit << (7 - i))))
        .collect()
}
