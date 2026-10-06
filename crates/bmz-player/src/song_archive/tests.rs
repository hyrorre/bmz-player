use super::*;
use std::sync::atomic::AtomicU64;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bmz-song-archive-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// A minimal stored ZIP fixture permits raw CP932 names and deliberately malformed
// duplicate/link headers which production ZipWriter correctly refuses to create.
fn zip(path: &Path, entries: &[(&[u8], &[u8], u32)]) {
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    for (name, data, mode) in entries {
        let offset = bytes.len() as u32;
        let crc = crc32(data);
        bytes.extend_from_slice(b"PK\x03\x04");
        for value in [20u16, 0, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, data.len() as u32, data.len() as u32] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(data);
        central.extend_from_slice(b"PK\x01\x02");
        for value in [0x0314u16, 20, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, data.len() as u32, data.len() as u32] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let offset = bytes.len() as u32;
    let size = central.len() as u32;
    bytes.extend_from_slice(&central);
    bytes.extend_from_slice(b"PK\x05\x06");
    for value in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    std::fs::write(path, bytes).unwrap();
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[test]
fn cp932_names_decode_before_slash_normalization_in_zip_and_rar() {
    let temp = Temp::new();
    let control = ArchiveControl::default();
    let zip_path = temp.path("legacy.zip");
    let rar_path = temp.path("legacy.rar");
    zip(&zip_path, &[(b"\x83\x5c/\x95\x88\x96\xca.bms", b"#TITLE CP932\n", 0o100644)]);
    std::fs::write(&rar_path, include_bytes!("fixtures/rar4-cp932.rar")).unwrap();
    for path in [zip_path, rar_path] {
        let index = inspect(&path, &control).unwrap();
        assert_eq!(index.entries[0].name, "ソ/譜面.bms");
        assert_eq!(read_chart(&path, "ソ/譜面.bms", &control).unwrap().bytes, b"#TITLE CP932\n");
        let materialized =
            materialize(&path, &index.generation, &temp.path("cache"), &control).unwrap();
        assert_eq!(
            std::fs::read(materialized.root.join("ソ/譜面.bms")).unwrap(),
            b"#TITLE CP932\n"
        );
    }
}

#[test]
fn rar4_and_rar5_solid_decode_prefix_once_and_publish_all_assets() {
    let temp = Temp::new();
    for (name, bytes) in [
        ("four.rar", include_bytes!("fixtures/rar4-solid.rar").as_slice()),
        ("five.rar", include_bytes!("fixtures/rar5-solid.rar").as_slice()),
    ] {
        let path = temp.path(name);
        std::fs::write(&path, bytes).unwrap();
        let mut charts = Vec::new();
        let index = scan_charts(&path, &ArchiveControl::default(), |entry, bytes| {
            charts.push((entry.name.clone(), bytes.to_vec()));
            Ok(())
        })
        .unwrap();
        assert_eq!(index.entries.len(), 6);
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].0, "曲/譜面.bms");
        let result =
            materialize(&path, &index.generation, &temp.path("cache"), &ArchiveControl::default())
                .unwrap();
        assert_eq!(
            std::fs::read(result.root.join("曲/音声01.wav")).unwrap(),
            std::fs::read(result.root.join("曲/音声02.wav")).unwrap()
        );
        assert_eq!(std::fs::read(result.root.join("曲/譜面.bms")).unwrap(), charts[0].1);
        assert_eq!(std::fs::metadata(result.root.join("empty.txt")).unwrap().len(), 0);
    }
}

#[test]
fn traversal_links_reserved_names_and_collisions_are_rejected() {
    let temp = Temp::new();
    let path = temp.path("unsafe.zip");
    for name in [
        "../outside.bms",
        "/root.bms",
        "C:\\root.bms",
        "x/../a.bms",
        "NUL.bms",
        "x .bms/.",
        "x./chart.bms",
        "stream:ads.bms",
    ] {
        zip(&path, &[(name.as_bytes(), b"chart", 0o100644)]);
        assert!(inspect(&path, &ArchiveControl::default()).is_err(), "{name}");
    }
    for (first, second) in [
        ("same.bms", "same.bms"),
        ("a.bms", "A.bms"),
        ("é.bms", "e\u{301}.bms"),
        ("x", "x/a.bms"),
        ("A/one.bms", "a/two.bms"),
    ] {
        zip(&path, &[(first.as_bytes(), b"x", 0o100644), (second.as_bytes(), b"y", 0o100644)]);
        assert!(inspect(&path, &ArchiveControl::default()).is_err(), "{first}, {second}");
    }
    zip(&path, &[(b"link.bms", b"../outside", 0o120777)]);
    assert!(inspect(&path, &ArchiveControl::default()).is_err());
    assert!(!temp.path("outside.bms").exists());
}

#[test]
fn limits_apply_before_extracting_and_cancel_during_scan() {
    let temp = Temp::new();
    let path = temp.path("limits.zip");
    zip(
        &path,
        &[(b"first.bms", b"first chart", 0o100644), (b"second.bms", b"second chart", 0o100644)],
    );
    for limits in [
        ArchiveLimits { compressed_bytes: 1, ..ArchiveLimits::default() },
        ArchiveLimits { extracted_bytes: 1, ..ArchiveLimits::default() },
        ArchiveLimits { chart_bytes: 1, ..ArchiveLimits::default() },
        ArchiveLimits { file_bytes: 1, ..ArchiveLimits::default() },
        ArchiveLimits { header_bytes: 1, ..ArchiveLimits::default() },
        ArchiveLimits { entries: 1, ..ArchiveLimits::default() },
    ] {
        assert!(inspect(&path, &ArchiveControl { limits, cancelled: None }).is_err());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let control =
        ArchiveControl { cancelled: Some(cancelled.clone()), ..ArchiveControl::default() };
    let result = scan_charts(&path, &control, |_, _| {
        cancelled.store(true, Ordering::Relaxed);
        Ok(())
    });
    assert!(result.unwrap_err().to_string().contains("cancel"));
    assert!(inspect(&path, &control).is_err());
}

#[test]
fn corruption_and_cancel_never_publish_partial_cache() {
    let temp = Temp::new();
    let path = temp.path("bad.zip");
    zip(&path, &[(b"chart.bms", b"valid content", 0o100644)]);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[30 + b"chart.bms".len()] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    let index = inspect(&path, &ArchiveControl::default()).unwrap();
    assert!(
        materialize(&path, &index.generation, &temp.path("cache"), &ArchiveControl::default())
            .is_err()
    );
    let source_root = std::fs::read_dir(temp.path("cache/song-archives"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read_dir(source_root).unwrap().count(), 0);
    let control = ArchiveControl {
        cancelled: Some(Arc::new(AtomicBool::new(true))),
        ..ArchiveControl::default()
    };
    assert!(materialize(&path, &index.generation, &temp.path("cancel"), &control).is_err());
    assert!(!temp.path("cancel").exists());
}

#[test]
fn concurrent_materialize_coalesces_and_generations_keep_old_assets() {
    let temp = Temp::new();
    let path = temp.path("song.zip");
    zip(&path, &[(b"chart.bms", b"#TITLE old", 0o100644)]);
    let generation = inspect(&path, &ArchiveControl::default()).unwrap().generation;
    let paths = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| {
                scope.spawn(|| {
                    materialize(&path, &generation, &temp.path("cache"), &ArchiveControl::default())
                        .unwrap()
                        .root
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
    });
    assert!(paths.iter().all(|path| path == &paths[0]));
    assert_eq!(std::fs::read_dir(paths[0].parent().unwrap().parent().unwrap()).unwrap().count(), 1);
    zip(&path, &[(b"chart.bms", b"#TITLE new generation", 0o100644)]);
    let next = inspect(&path, &ArchiveControl::default()).unwrap().generation;
    assert_ne!(next, generation);
    assert!(
        materialize(&path, &generation, &temp.path("cache"), &ArchiveControl::default()).is_err()
    );
    let newer = materialize(&path, &next, &temp.path("cache"), &ArchiveControl::default()).unwrap();
    assert_eq!(std::fs::read(paths[0].join("chart.bms")).unwrap(), b"#TITLE old");
    assert_eq!(std::fs::read(newer.root.join("chart.bms")).unwrap(), b"#TITLE new generation");
    let copy = temp.path("copy.zip");
    std::fs::copy(&path, &copy).unwrap();
    let copied =
        materialize(&copy, &next, &temp.path("cache"), &ArchiveControl::default()).unwrap();
    assert_ne!(copied.root, newer.root);
}

#[test]
fn damaged_private_cache_is_rebuilt() {
    let temp = Temp::new();
    let path = temp.path("song.zip");
    zip(&path, &[(b"chart.bms", b"#TITLE song", 0o100644)]);
    let control = ArchiveControl::default();
    let generation = inspect(&path, &control).unwrap().generation;
    let result = materialize(&path, &generation, &temp.path("cache"), &control).unwrap();
    std::fs::write(result.root.parent().unwrap().join("complete.json"), b"broken json").unwrap();
    let repaired = materialize(&path, &generation, &temp.path("cache"), &control).unwrap();
    assert_eq!(std::fs::read(repaired.root.join("chart.bms")).unwrap(), b"#TITLE song");
    std::fs::remove_dir_all(&repaired.root).unwrap();
    assert!(
        materialize(&path, &generation, &temp.path("cache"), &control)
            .unwrap()
            .root
            .join("chart.bms")
            .is_file()
    );
    std::fs::remove_file(result.root.join("chart.bms")).unwrap();
    assert!(
        materialize(&path, &generation, &temp.path("cache"), &control)
            .unwrap()
            .root
            .join("chart.bms")
            .is_file()
    );
}

#[test]
fn apple_metadata_is_not_a_chart_but_hidden_chart_remains_available() {
    for name in ["__MACOSX/chart.bms", "song/._chart.bms"] {
        assert!(!ArchiveEntry { name: name.to_owned(), size: 1, is_directory: false }.is_chart());
    }
    assert!(
        ArchiveEntry { name: ".hidden/chart.BMSON".to_owned(), size: 1, is_directory: false }
            .is_chart()
    );
}

fn sevenz(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    let metadata =
        entries.iter().map(|(name, _)| sevenz_rust2::ArchiveEntry::new_file(name)).collect();
    let sources = entries
        .iter()
        .map(|(_, data)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(*data)))
        .collect();
    writer.push_archive_entries(metadata, sources).unwrap();
    writer
        .push_archive_entry::<std::io::Empty>(
            sevenz_rust2::ArchiveEntry::new_directory("empty"),
            None,
        )
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn sevenz_solid_streams_assets_and_multiple_charts_with_dictionary_limit() {
    let temp = Temp::new();
    let path = temp.path("solid.7z");
    let payload = vec![0x55; 262144];
    sevenz(
        &path,
        &[
            ("曲/ソ.wav", &payload),
            ("曲/a.bms", b"#TITLE first"),
            ("曲/b.bms", b"#TITLE second"),
            ("empty.txt", b""),
        ],
    );
    let mut names = Vec::new();
    let control = ArchiveControl::default();
    let index = scan_charts(&path, &control, |entry, bytes| {
        assert!(bytes.starts_with(b"#TITLE"));
        names.push(entry.name.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(names, ["曲/a.bms", "曲/b.bms"]);
    assert_eq!(read_chart(&path, "曲/b.bms", &control).unwrap().bytes, b"#TITLE second");
    let output = materialize(&path, &index.generation, &temp.path("cache"), &control).unwrap();
    assert_eq!(std::fs::read(output.root.join("曲/ソ.wav")).unwrap(), payload);
    assert!(output.root.join("empty").is_dir());
    assert_eq!(std::fs::metadata(output.root.join("empty.txt")).unwrap().len(), 0);
    let tiny = ArchiveControl {
        limits: ArchiveLimits { dictionary_bytes: 1, ..ArchiveLimits::default() },
        ..ArchiveControl::default()
    };
    assert!(inspect(&path, &tiny).unwrap_err().to_string().contains("dictionary"));
}

#[test]
fn sevenz_rejects_traversal_links_and_encrypted_content() {
    let temp = Temp::new();
    let bad = temp.path("bad.7z");
    sevenz(&bad, &[("../chart.bms", b"chart")]);
    assert!(inspect(&bad, &ArchiveControl::default()).is_err());
    let mut writer = sevenz_rust2::ArchiveWriter::create(&bad).unwrap();
    let mut link = sevenz_rust2::ArchiveEntry::new_file("link.bms");
    link.has_windows_attributes = true;
    link.windows_attributes = 0o120777 << 16;
    writer.push_archive_entry(link, Some(std::io::Cursor::new(b"../outside"))).unwrap();
    writer.finish().unwrap();
    assert!(inspect(&bad, &ArchiveControl::default()).is_err());
    let mut writer = sevenz_rust2::ArchiveWriter::create(&bad).unwrap();
    writer.set_content_methods(vec![
        sevenz_rust2::encoder_options::AesEncoderOptions::new("secret".into()).into(),
        sevenz_rust2::EncoderMethod::LZMA2.into(),
    ]);
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("secret.bms"),
            Some(std::io::Cursor::new(b"secret")),
        )
        .unwrap();
    writer.finish().unwrap();
    assert!(inspect(&bad, &ArchiveControl::default()).is_err());
}

#[test]
fn metered_output_rejects_overrun_short_entry_and_midstream_cancel() {
    let entry = ArchiveEntry { name: "chart.bms".to_owned(), size: 3, is_directory: false };
    let cancelled = Arc::new(AtomicBool::new(false));
    let control =
        ArchiveControl { cancelled: Some(cancelled.clone()), ..ArchiveControl::default() };
    let meters = Arc::new(Mutex::new(safety::Meters::default()));
    let mut writer = MeteredWriter::new(Box::new(std::io::sink()), &entry, control, meters.clone());
    assert!(writer.write_all(b"four").is_err());
    writer.write_all(b"a").unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(writer.write_all(b"bc").is_err());
    drop(writer);
    assert!(!meters.lock().unwrap().complete);
}

#[test]
fn zip64_end_record_remains_supported_and_bounded() {
    let temp = Temp::new();
    let path = temp.path("zip64.zip");
    zip(&path, &[(b"chart.bms", b"#TITLE ZIP64", 0o100644)]);
    let mut bytes = std::fs::read(&path).unwrap();
    let end_offset = bytes.len() - 22;
    let mut end = bytes.split_off(end_offset);
    let size = u32::from_le_bytes(end[12..16].try_into().unwrap()) as u64;
    let offset = u32::from_le_bytes(end[16..20].try_into().unwrap()) as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44u64.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    for value in [1u64, 1, size, offset] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(end_offset as u64).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    end[8..20].fill(0xff);
    bytes.extend_from_slice(&end);
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(
        read_chart(&path, "chart.bms", &ArchiveControl::default()).unwrap().bytes,
        b"#TITLE ZIP64"
    );
    bytes[end_offset + 24..end_offset + 32].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[end_offset + 32..end_offset + 40].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(inspect(&path, &ArchiveControl::default()).unwrap_err().to_string().contains("count"));
}

#[test]
fn zip_verified_unicode_extra_name_takes_precedence_over_legacy_name() {
    let temp = Temp::new();
    let path = temp.path("unicode.zip");
    zip(&path, &[(b"fallback.bms", b"#TITLE Unicode", 0o100644)]);
    let mut bytes = std::fs::read(&path).unwrap();
    let end = bytes.len() - 22;
    let central = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    let name = "曲/譜面.bms";
    let mut extra = 0x7075u16.to_le_bytes().to_vec();
    extra.extend_from_slice(&(5u16 + name.len() as u16).to_le_bytes());
    extra.push(1);
    extra.extend_from_slice(&crc32(b"fallback.bms").to_le_bytes());
    extra.extend_from_slice(name.as_bytes());
    bytes[central + 30..central + 32].copy_from_slice(&(extra.len() as u16).to_le_bytes());
    let size =
        u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap()) + extra.len() as u32;
    bytes[end + 12..end + 16].copy_from_slice(&size.to_le_bytes());
    bytes.splice(end..end, extra);
    std::fs::write(&path, bytes).unwrap();
    let index = inspect(&path, &ArchiveControl::default()).unwrap();
    assert_eq!(index.entries[0].name, name);
    assert_eq!(
        read_chart(&path, name, &ArchiveControl::default()).unwrap().bytes,
        b"#TITLE Unicode"
    );
}

#[test]
fn cached_generation_rejects_links_instead_of_following_them() {
    let temp = Temp::new();
    let path = temp.path("song.zip");
    zip(&path, &[(b"chart.bms", b"#TITLE song", 0o100644)]);
    let control = ArchiveControl::default();
    let generation = inspect(&path, &control).unwrap().generation;
    let result = materialize(&path, &generation, &temp.path("cache"), &control).unwrap();
    let outside = temp.path("outside.bms");
    std::fs::write(&outside, b"#TITLE song").unwrap();
    let link = result.root.join("extra.bms");
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_file(&outside, &link);
    #[cfg(not(windows))]
    let created = std::os::unix::fs::symlink(&outside, &link);
    if let Err(error) = created {
        eprintln!("cache symlink fixture unavailable: {error}");
        return;
    }
    std::fs::remove_file(result.root.join("chart.bms")).unwrap();
    std::fs::rename(link, result.root.join("chart.bms")).unwrap();
    assert!(
        materialize(&path, &generation, &temp.path("cache"), &control)
            .unwrap_err()
            .to_string()
            .contains("link")
    );
    assert_eq!(std::fs::read(outside).unwrap(), b"#TITLE song");
}

#[test]
fn rar_volume_corruption_and_decoder_limits_are_rejected() {
    let temp = Temp::new();
    let path = temp.path("invalid.rar");
    let mut volume = include_bytes!("fixtures/rar4-cp932.rar").to_vec();
    let header_size = u16::from_le_bytes(volume[12..14].try_into().unwrap()) as usize;
    volume[10] |= 1; // RAR4 main header MHD_VOLUME, even without a split file.
    let checksum = crc32(&volume[9..7 + header_size]) as u16;
    volume[7..9].copy_from_slice(&checksum.to_le_bytes());
    std::fs::write(&path, volume).unwrap();
    assert!(inspect(&path, &ArchiveControl::default()).unwrap_err().to_string().contains("split"));
    let mut corrupt = include_bytes!("fixtures/rar4-cp932.rar").to_vec();
    let data = corrupt.windows(13).position(|bytes| bytes == b"#TITLE CP932\n").unwrap();
    corrupt[data] ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    assert!(read_chart(&path, "ソ/譜面.bms", &ArchiveControl::default()).is_err());
    std::fs::write(&path, include_bytes!("fixtures/rar5-solid.rar")).unwrap();
    let control = ArchiveControl {
        limits: ArchiveLimits { dictionary_bytes: 1, ..ArchiveLimits::default() },
        ..ArchiveControl::default()
    };
    assert!(scan_charts(&path, &control, |_, _| Ok(())).is_err());
    let control = ArchiveControl {
        limits: ArchiveLimits { reader_workspace_bytes: 1, ..ArchiveLimits::default() },
        ..ArchiveControl::default()
    };
    assert!(scan_charts(&path, &control, |_, _| Ok(())).is_err());
}
