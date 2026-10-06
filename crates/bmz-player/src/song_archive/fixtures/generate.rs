use rars::{ArchiveVersion, Builder};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).expect("fixture destination"));
    std::fs::create_dir_all(&root)?;
    let mut random = 0x12345678u32;
    let mut payload = vec![0; 32768];
    for byte in &mut payload {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        *byte = (random >> 24) as u8;
    }
    let mut repeated = payload.repeat(4);
    repeated.extend_from_slice(b"BMZ archive probe own fixture payload");
    let entries = vec![
        ("曲/音声01.wav", repeated.clone()),
        ("曲/音声02.wav", repeated.clone()),
        ("曲/譜面.bms", b"#TITLE archive probe\n#WAV01 ../common.wav\n#00111:01\n".to_vec()),
        ("common.wav", payload.clone()),
        ("empty.txt", Vec::new()),
        ("曲/readme.txt", "BMZの検証専用に生成したデータ。第三者素材なし。\n".as_bytes().to_vec()),
    ];
    let expected = root.join("expected");
    for (name, bytes) in &entries {
        let path = expected.join(name);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    for (label, format) in [("rar4-solid", ArchiveVersion::Rar29), ("rar5-solid", ArchiveVersion::Rar50)] {
        let mut builder = Builder::new(format).solid(true).compression_level(Some(3));
        for (name, bytes) in &entries {
            builder.add_bytes(name.as_bytes().to_vec(), bytes.clone(), None, None)?;
            if format == ArchiveVersion::Rar29 {
                let mut wire = b"fallback\0\0".to_vec();
                let units: Vec<_> = name.encode_utf16().collect();
                for group in units.chunks(4) {
                    wire.push(0xaa);
                    for unit in group { wire.extend_from_slice(&unit.to_le_bytes()); }
                }
                builder.set_legacy_unicode_name(name.as_bytes(), wire)?;
            }
        }
        let archive = root.join(format!("{label}.rar"));
        builder.write_to_path(&archive, None)?;
        println!("{} {} bytes", archive.display(), std::fs::metadata(&archive)?.len());
    }
    let mut legacy = Builder::new(ArchiveVersion::Rar29);
    // CP932: ソ/譜面.bms (ソ contains 0x5c as its second byte).
    legacy.add_bytes(b"\x83\x5c/\x95\x88\x96\xca.bms".to_vec(), b"#TITLE CP932\n".to_vec(), None, None)?;
    legacy.write_to_path(&root.join("rar4-cp932.rar"), None)?;
    Ok(())
}
