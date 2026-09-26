use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;

pub(super) fn registered_charts(
    data: &ProfileTestDir,
) -> (bootstrap::BootstrappedApp, PathBuf, PathBuf) {
    let mut boot = data.boot();
    let root = data.paths.data_dir.join("songs");
    let path = root.join("original/song.bms");
    let copy = root.join("copy/song.bms");
    for file in [&path, &copy] {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "#TITLE Original\n#BPM 120\n#00011:01\n").unwrap();
    }
    boot.app_config.songs.roots = vec![PathEntry {
        path: root.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    boot.app_config.scan.use_everything = false;
    crate::storage::scan::scan_song_roots(
        &mut boot.library_db,
        &boot.app_config.songs.roots,
        &boot.app_config.scan,
        1,
        false,
    )
    .unwrap();
    boot.library_db.set_configured_song_roots(&boot.app_config.songs.roots).unwrap();
    (boot, path, copy)
}

#[test]
fn explicit_boot_path_refreshes_changed_file_instead_of_playing_its_old_copy() {
    let data = ProfileTestDir::new();
    let (mut boot, path, copy) = registered_charts(&data);
    std::fs::write(&path, "#TITLE Changed\n#BPM 180\n#00012:01\n").unwrap();
    let mut options = AppOptions {
        boot_play_path: Some(path.to_string_lossy().into_owned()),
        ..Default::default()
    };
    prepare_boot_chart_options(&mut boot, &mut options).unwrap();
    let id = resolve_boot_chart_id(&boot.library_db, &options).unwrap();
    let source = boot.library_db.verified_chart_source(id).unwrap();
    assert_eq!(source.path.canonicalize().unwrap(), path.canonicalize().unwrap());
    let chart =
        crate::screens::play_session::load_source_chart_for_chart(&boot.library_db, id, None)
            .unwrap();
    assert_eq!(chart.metadata.title, "Changed");
    let copy_id = boot.library_db.chart_id_by_chart_file_path(&copy).unwrap().unwrap();
    assert_ne!(
        boot.library_db.chart_sha256_by_chart_id(copy_id).unwrap(),
        Some(chart.identity.file_sha256)
    );
    std::fs::remove_file(copy).unwrap();
    prepare_boot_chart_options(&mut boot, &mut options).unwrap();
    assert!(boot.library_db.verified_chart_source(id).is_ok());
}

#[test]
fn explicit_boot_path_can_play_a_registered_file_in_a_disabled_root() {
    let data = ProfileTestDir::new();
    let (mut boot, path, _) = registered_charts(&data);
    boot.app_config.songs.roots[0].enabled = false;
    boot.library_db.set_configured_song_roots(&boot.app_config.songs.roots).unwrap();
    let mut options = AppOptions {
        boot_play_path: Some(path.to_string_lossy().into_owned()),
        ..Default::default()
    };
    prepare_boot_chart_options(&mut boot, &mut options).unwrap();
    let id = resolve_boot_chart_id(&boot.library_db, &options).unwrap();
    let source = boot.library_db.verified_chart_source(id).unwrap();
    assert_eq!(source.path.canonicalize().unwrap(), path.canonicalize().unwrap());
    assert!(
        crate::screens::play_session::load_source_chart_for_chart(&boot.library_db, id, None)
            .is_ok()
    );
    assert!(!boot.app_config.songs.roots[0].enabled);
    assert!(boot.library_db.available_chart_source(id).unwrap().is_none());
}
