use super::*;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::song_archive::test_support::{self, FORMATS};
use crate::storage::library_db::LibraryDatabase;

#[test]
fn every_format_reads_scans_and_extracts_through_the_same_backend() {
    for format in FORMATS {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("package.{}", format.extension()));
        std::fs::write(&path, test_support::archive(format)).unwrap();
        let control = ArchiveControl::default();
        let index = inspect(&path, &control).unwrap();
        let expected = read_all(&path, &control).unwrap();
        let mut charts = Vec::new();
        scan_charts(&path, &control, |entry, bytes| {
            charts.push(entry.name.clone());
            assert_eq!(bytes, expected[&entry.name]);
            let locator = crate::chart_source::ChartLocator::Archive {
                container: path.clone(),
                entry: entry.name.clone(),
            };
            assert_eq!(
                crate::chart_source::ChartLocator::parse(&locator.to_path_buf()).unwrap(),
                locator
            );
            let resolved = locator.resolve().unwrap();
            assert_eq!(bmz_song_resource::read(&resolved.path).unwrap().unwrap().as_ref(), bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(charts.len(), index.entries.iter().filter(|entry| entry.is_chart()).count());
        assert!(!charts.is_empty());
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "direct reads must not extract files"
        );
        let destination = root.path().join("extracted");
        extract_to(&path, &destination, &control).unwrap();
        for (name, bytes) in expected {
            assert_eq!(std::fs::read(destination.join(name)).unwrap(), bytes);
        }
    }
}

#[test]
fn explicit_archive_scans_register_only_the_target_and_survive_full_sync() {
    for format in FORMATS {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("target.{}", format.extension()));
        let unrelated = root.path().join("unrelated.bms");
        std::fs::write(&path, test_support::archive(format)).unwrap();
        std::fs::write(&unrelated, b"#TITLE Unrelated\n#BPM 180\n#00011:01\n").unwrap();
        let db_path = root.path().join("library.db");
        crate::storage::migration::migrate_library_db(&db_path).unwrap();
        let mut db = LibraryDatabase::open(&db_path).unwrap();
        let targets = [PathEntry {
            path: path.to_string_lossy().into_owned(),
            enabled: true,
            recursive: false,
        }];
        db.set_configured_song_roots(&targets).unwrap();
        let config = AppConfig::default(); // File targets must bypass Everything even when enabled.
        let report =
            crate::storage::scan::scan_song_roots(&mut db, &targets, &config.scan, 1, false)
                .unwrap();
        assert!(report.summary.imported > 0, "{format:?}: {report:?}");
        assert!(report.discovery_issues.is_empty());
        assert!(db.chart_id_by_chart_file_path(&unrelated).unwrap().is_none());
        assert!(!crate::song_download::downloaded_chart_identities(&db, &path).unwrap().is_empty());
        assert_eq!(db.reconcile_configured_song_roots(&targets).unwrap(), 0);
        let again =
            crate::storage::scan::scan_song_roots(&mut db, &targets, &config.scan, 2, false)
                .unwrap();
        assert_eq!(again.summary.imported, 0);
        assert_eq!(again.summary.removed_files, 0);
    }
}

#[test]
fn lzh_lh0_and_lh5_preserve_cp932_paths_and_verify_crc() {
    for lh5 in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("legacy.lzh");
        let name = b"\x83\x5c\\\x95\x88\x96\xca.bms";
        let bytes = test_support::lzh(&[(name, test_support::CHART)], lh5);
        std::fs::write(&path, &bytes).unwrap();
        let entries = read_all(&path, &Default::default()).unwrap();
        assert_eq!(entries["ソ/譜面.bms"], test_support::CHART);
        let mut corrupt = bytes;
        let crc_offset = 22 + name.len();
        corrupt[crc_offset] ^= 1;
        corrupt[1] = corrupt[2..corrupt[0] as usize + 2]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        std::fs::write(&path, corrupt).unwrap();
        assert!(read_all(&path, &Default::default()).is_err());
    }
}

#[test]
fn lzh_rejects_paths_links_truncation_and_limits_before_reading() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("legacy.lzh");
    for name in [b"../outside.bms".as_slice(), b"/outside.bms", b"C:\\outside.bms"] {
        std::fs::write(&path, test_support::lzh(&[(name, b"#TITLE Test")], false)).unwrap();
        assert!(inspect(&path, &Default::default()).is_err());
    }
    std::fs::write(&path, test_support::archive(bmz_song_resource::ArchiveFormat::Lzh)).unwrap();
    for limits in [
        ArchiveLimits { entries: 1, ..Default::default() },
        ArchiveLimits { file_bytes: 1, ..Default::default() },
        ArchiveLimits { header_bytes: 1, ..Default::default() },
    ] {
        assert!(inspect(&path, &ArchiveControl { limits, cancelled: None }).is_err());
    }
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.truncate(bytes.len() / 2);
    std::fs::write(&path, bytes).unwrap();
    assert!(inspect(&path, &Default::default()).is_err());
    std::fs::write(&path, test_support::lzh(&[(b"chart.bms", test_support::CHART)], true)).unwrap();
    assert!(
        inspect(
            &path,
            &ArchiveControl {
                limits: ArchiveLimits { dictionary_bytes: 1, ..Default::default() },
                cancelled: None,
            }
        )
        .is_err()
    );
}

#[test]
fn tar_gz_supports_root_directory_gnu_names_and_pax_paths_with_bounded_headers() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("names.tar.gz");
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut directory = tar::Header::new_gnu();
    directory.set_size(0);
    directory.set_mode(0o755);
    directory.set_entry_type(tar::EntryType::Directory);
    directory.set_cksum();
    builder.append_data(&mut directory, "./", std::io::empty()).unwrap();
    let long = format!("{}/chart.bms", "long-name-".repeat(15));
    for name in [&long, "日本語/chart.bms"] {
        let header_name = if name.starts_with("日本語") {
            builder.append_pax_extensions([("path", name.as_bytes())]).unwrap();
            "pax-placeholder.bms"
        } else {
            name
        };
        let mut header = tar::Header::new_gnu();
        header.set_size(test_support::CHART.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, header_name, test_support::CHART).unwrap();
    }
    std::fs::write(&path, builder.into_inner().unwrap().finish().unwrap()).unwrap();
    let all = read_all(&path, &Default::default()).unwrap();
    assert_eq!(all[&long], test_support::CHART);
    assert_eq!(all["日本語/chart.bms"], test_support::CHART);
    assert!(
        inspect(
            &path,
            &ArchiveControl {
                limits: ArchiveLimits { header_bytes: 512, ..Default::default() },
                cancelled: None,
            }
        )
        .is_err()
    );
}

#[test]
fn an_archive_partial_rescan_prunes_only_that_container_under_a_configured_parent() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("target.zip");
    let other = root.path().join("other.zip");
    zip(&target, &[(b"old.bms", b"#TITLE Old\n#BPM 120\n#00011:01\n", 0o100644)]);
    zip(&other, &[(b"keep.bms", b"#TITLE Keep\n#BPM 120\n#00011:01\n", 0o100644)]);
    let db_path = root.path().join("library.db");
    crate::storage::migration::migrate_library_db(&db_path).unwrap();
    let mut db = LibraryDatabase::open(&db_path).unwrap();
    let parent = [PathEntry {
        path: root.path().to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    db.set_configured_song_roots(&parent).unwrap();
    let mut config = AppConfig::default();
    config.scan.use_everything = false;
    crate::storage::scan::scan_song_roots(&mut db, &parent, &config.scan, 1, true).unwrap();
    let locator = |container: &Path, entry: &str| {
        crate::chart_source::ChartLocator::Archive {
            container: container.into(),
            entry: entry.into(),
        }
        .to_path_buf()
    };
    let old = locator(&target, "old.bms");
    let keep = locator(&other, "keep.bms");
    let keep_id = db.chart_file_id_by_path(&keep).unwrap().unwrap();
    zip(&target, &[(b"new.bms", b"#TITLE New\n#BPM 180\n#00011:01\n", 0o100644)]);
    let file_target = [PathEntry {
        path: target.to_string_lossy().into_owned(),
        enabled: true,
        recursive: false,
    }];
    let report =
        crate::storage::scan::scan_song_roots(&mut db, &file_target, &config.scan, 2, true)
            .unwrap();
    assert_eq!(report.summary.removed_files, 1);
    assert!(db.chart_file_id_by_path(&old).unwrap().is_none());
    assert!(db.chart_file_id_by_path(&locator(&target, "new.bms")).unwrap().is_some());
    assert_eq!(db.chart_file_id_by_path(&keep).unwrap(), Some(keep_id));
    // An unreadable replacement cannot establish that a previously imported chart vanished.
    std::fs::write(&target, b"broken archive").unwrap();
    let failed =
        crate::storage::scan::scan_song_roots(&mut db, &file_target, &config.scan, 3, true)
            .unwrap();
    assert!(!failed.discovery_issues.is_empty());
    assert!(db.chart_file_id_by_path(&locator(&target, "new.bms")).unwrap().is_some());
}

#[test]
fn lzh_extended_headers_preserve_cp932_directory_components() {
    for level in [1, 2, 3] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("legacy.lzh");
        std::fs::write(&path, lzh_extended(level, false)).unwrap();
        let all = read_all(&path, &Default::default()).unwrap();
        assert_eq!(all["ソ/譜面.bms"], test_support::CHART);
        std::fs::write(&path, lzh_extended(level, true)).unwrap();
        assert!(inspect(&path, &Default::default()).is_err(), "symlink in header level {level}");
    }
}

fn lzh_extended(level: u8, link: bool) -> Vec<u8> {
    let data = test_support::CHART;
    let base = test_support::lzh(&[(b"x", data)], false);
    let crc = &base[23..25];
    let mut extensions = vec![vec![0x01], vec![0x02]];
    extensions[0].extend_from_slice(b"\x95\x88\x96\xca.bms");
    extensions[1].extend_from_slice(b"\x83\x5c\xff");
    if link {
        extensions.push(vec![0x50, 0xff, 0xa1]);
    }
    let width = if level == 3 { 4 } else { 2 };
    let lengths: Vec<_> = extensions.iter().map(|bytes| bytes.len() + width).collect();
    let mut extra = Vec::new();
    for (i, mut bytes) in extensions.into_iter().enumerate() {
        let next = lengths.get(i + 1).copied().unwrap_or(0) as u32;
        bytes.extend_from_slice(&next.to_le_bytes()[..width]);
        extra.extend(bytes);
    }
    let mut header = vec![if level == 3 { 4 } else { 0 }, 0];
    header.extend_from_slice(b"-lh0-");
    header.extend_from_slice(
        &((data.len() + if level == 1 { extra.len() } else { 0 }) as u32).to_le_bytes(),
    );
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&[0x20, level]);
    if level == 1 {
        header.push(0);
    } // Empty base filename; use the filename extension.
    header.extend_from_slice(crc);
    header.push(b'U');
    if level == 3 {
        header.extend_from_slice(&((32 + extra.len()) as u32).to_le_bytes());
    }
    header.extend_from_slice(&(lengths[0] as u32).to_le_bytes()[..width]);
    if level == 1 {
        header[0] = (header.len() - 2) as u8;
        header[1] = header[2..].iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    } else if level == 2 {
        let length = (header.len() + extra.len()) as u16;
        header[..2].copy_from_slice(&length.to_le_bytes());
    }
    header.extend(extra);
    header.extend_from_slice(data);
    header.push(0);
    header
}
