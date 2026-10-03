use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn edit_v3(bytes: &mut [u8], edit: impl FnOnce(&mut [u8])) {
    let key = [0x55, 0xaa, 0x20, 0x55, 0x55, 0x06, 0x55, 0xaa, 0x55, 0xd5, 0x7c, 0x66];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[i % 12];
    }
    edit(bytes);
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[i % 12];
    }
}

#[test]
fn nested_directories_resolve_and_directory_cycles_are_rejected() {
    let root = root();
    let mut bytes = archive(3, &[("sub", b"", false), ("a.txt", b"nested", false)]);
    let old_len = bytes.len();
    bytes.extend([0; 16]);
    edit_v3(&mut bytes, |data| {
        let index = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let files = index + u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let dirs = index + u32::from_le_bytes(data[20..24].try_into().unwrap()) as usize;
        data[4..8].copy_from_slice(&((old_len + 16 - index) as u32).to_le_bytes());
        data[files + 4..files + 8].copy_from_slice(&16u32.to_le_bytes());
        data[files + 32..files + 36].copy_from_slice(&16u32.to_le_bytes());
        data[dirs + 8..dirs + 12].copy_from_slice(&1u32.to_le_bytes());
        data[old_len..].fill(0);
        data[old_len + 8..old_len + 12].copy_from_slice(&1u32.to_le_bytes());
        data[old_len + 12..old_len + 16].copy_from_slice(&44u32.to_le_bytes());
    });
    fs::write(root.join("font.dxa"), &bytes).unwrap();
    assert_eq!(read(&root.join("font/SUB/a.txt")).unwrap(), b"nested");
    edit_v3(&mut bytes, |data| {
        let index = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let files = index + u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        data[files + 32..files + 36].fill(0);
    });
    fs::write(root.join("font.dxa"), &bytes).unwrap();
    assert!(
        read(&root.join("font/sub/a.txt")).unwrap_err().root_cause().to_string().contains("cyclic")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_versions_offsets_and_size_limits_are_rejected() {
    let root = root();
    let valid = archive(3, &[("a.txt", b"hello", true)]);
    for field in [2, 4, 12, 16, 20] {
        let mut bytes = valid.clone();
        edit_v3(&mut bytes, |data| data[field..field + 4].fill(0xff));
        fs::write(root.join("font.dxa"), bytes).unwrap();
        assert!(read(&root.join("font/a.txt")).is_err(), "header offset={field}");
    }
    for field in [32, 36, 40] {
        let mut bytes = valid.clone();
        edit_v3(&mut bytes, |data| {
            let index = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
            let files = index + u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
            data[files + field..files + field + 4].copy_from_slice(&0xffff_fffeu32.to_le_bytes());
        });
        fs::write(root.join("font.dxa"), bytes).unwrap();
        assert!(read(&root.join("font/a.txt")).is_err(), "file offset={field}");
    }
    fs::remove_dir_all(root).unwrap();
}

fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "bmz-dxa-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

// Small independently generated archives; no third-party skin data in fixtures.
fn archive(version: u16, files: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let header_size = if version == 4 { 28 } else { 24 };
    let mut data = vec![0; header_size];
    let mut names = Vec::new();
    let mut entries = Vec::new();
    for (name, content, compressed) in files {
        let (encoded, _, errors) = encoding_rs::SHIFT_JIS.encode(name);
        assert!(!errors);
        let name_offset = names.len();
        let span = (encoded.len() + 1).next_multiple_of(4);
        names.extend((span as u16 / 4).to_le_bytes());
        names.extend([0; 2]);
        let mut uppercase = encoded.to_vec();
        uppercase.make_ascii_uppercase();
        names.extend(&uppercase);
        names.resize(name_offset + 4 + span, 0);
        names.extend(&encoded[..]);
        names.resize(name_offset + 4 + span * 2, 0);
        let stored = if *compressed { literal_compressed(content) } else { content.to_vec() };
        let offset = data.len() - header_size;
        data.extend(&stored);
        entries.extend((name_offset as u32).to_le_bytes());
        entries.extend(0u32.to_le_bytes()); // attributes
        entries.extend([0; 24]); // timestamps
        entries.extend((offset as u32).to_le_bytes());
        entries.extend((content.len() as u32).to_le_bytes());
        if version >= 2 {
            entries.extend(if *compressed { stored.len() as u32 } else { u32::MAX }.to_le_bytes());
        }
    }
    let index_start = data.len();
    let file_table = names.len();
    let dir_table = file_table + entries.len();
    data.extend(&names);
    data.extend(entries);
    data.extend(u32::MAX.to_le_bytes());
    data.extend(u32::MAX.to_le_bytes());
    data.extend((files.len() as u32).to_le_bytes());
    data.extend(0u32.to_le_bytes());
    let index_size = data.len() - index_start;
    data[..2].copy_from_slice(b"DX");
    data[2..4].copy_from_slice(&version.to_le_bytes());
    for (offset, value) in
        [(4, index_size), (8, header_size), (12, index_start), (16, file_table), (20, dir_table)]
    {
        data[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
    }
    if version == 4 {
        data[24..28].copy_from_slice(&932u32.to_le_bytes());
    }
    let key = if version <= 2 {
        [0xff; 12]
    } else {
        [0x55, 0xaa, 0x20, 0x55, 0x55, 0x06, 0x55, 0xaa, 0x55, 0xd5, 0x7c, 0x66]
    };
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= key[i % 12];
    }
    data
}

fn literal_compressed(content: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::from((content.len() as u32).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.push(0xff);
    for &byte in content {
        bytes.push(byte);
        if byte == 0xff {
            bytes.push(byte);
        }
    }
    let size = bytes.len() as u32;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn legacy_versions_decode_compressed_and_plain_files_without_extraction() {
    let root = root();
    for version in 1..=4 {
        fs::write(
            root.join("Font.dxa"),
            archive(
                version,
                &[
                    ("フォント.lr2font", b"#S,16\n\xff", version >= 2),
                    ("page.BMP", b"BMtest", false),
                ],
            ),
        )
        .unwrap();
        let asset = SkinAsset::resolve(&root.join("font/フォント.LR2FONT")).unwrap();
        assert!(asset.is_archived());
        assert_eq!(asset.read().unwrap(), b"#S,16\n\xff");
        assert_eq!(read(&root.join("FONT/PAGE.bmp")).unwrap(), b"BMtest");
        assert!(!root.join("Font").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn physical_override_and_archive_updates_are_observed() {
    let root = root();
    let archive_path = root.join("font.dxa");
    let logical = root.join("font/a.txt");
    fs::write(&archive_path, archive(3, &[("a.txt", b"first", false)])).unwrap();
    assert_eq!(read(&logical).unwrap(), b"first");
    fs::write(&archive_path, archive(3, &[("a.txt", b"replacement", true)])).unwrap();
    assert_eq!(read(&logical).unwrap(), b"replacement");
    fs::create_dir(root.join("font")).unwrap();
    fs::write(&logical, b"override").unwrap();
    let asset = SkinAsset::resolve(&logical).unwrap();
    assert!(!asset.is_archived());
    assert_eq!(asset.read().unwrap(), b"override");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn archives_are_checked_against_allowed_roots_before_parsing() {
    let root = root();
    fs::write(root.join("font.dxa"), b"not even an archive").unwrap();
    let error = SkinAsset::resolve_with(&root.join("font/a.txt"), |_| false).unwrap_err();
    assert!(error.to_string().contains("escapes allowed roots"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_archives_fail_without_panics_or_large_allocations() {
    let root = root();
    let valid = archive(3, &[("a.txt", b"test", true)]);
    for end in 0..valid.len() {
        fs::write(root.join("font.dxa"), &valid[..end]).unwrap();
        assert!(read(&root.join("font/a.txt")).is_err(), "truncation={end}");
    }
    for invalid in ["..", "../evil", "C:evil", "a\\b", "/absolute"] {
        fs::write(root.join("font.dxa"), archive(3, &[(invalid, b"test", false)])).unwrap();
        assert!(read(&root.join("font/a.txt")).is_err());
    }
    fs::write(root.join("font.dxa"), archive(3, &[("a.txt", b"test", false)])).unwrap();
    assert!(read(&root.join("font/nested/../a.txt")).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn kcool_font_archives_read_when_available() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/KCOOL SKIN (Ver 1.72)/Font");
    if !root.join("barfont.dxa").is_file() {
        return;
    }
    for (folder, font, page) in [
        ("barfont", "font.lr2font", "font_00.png"),
        ("SystemFont", "font.lr2font", "font_00.png"),
        ("title", "aq-kaic+.lr2font", "aq-kaic_00.png"),
    ] {
        assert!(!root.join(folder).exists(), "fixture should remain archived");
        let text = read(&root.join(folder).join(font)).unwrap();
        assert!(text.windows(3).any(|bytes| bytes == b"#R,"));
        assert!(read(&root.join(folder).join(page)).unwrap().starts_with(b"\x89PNG"));
    }
}
