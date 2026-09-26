use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::storage::common::hash_to_hex;
use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};

fn course_definition(chart_id: i64, sha256: [u8; 32]) -> CourseDefinition {
    CourseDefinition {
        key: "copy-fallback".to_string(),
        title: "Copy fallback".to_string(),
        kind: CourseKind::Course,
        entries: vec![
            CourseEntry {
                title_hint: String::new(),
                md5: None,
                sha256: Some(hash_to_hex(&sha256)),
                chart_id: Some(chart_id),
            };
            2
        ],
        constraints: Default::default(),
        trophies: Vec::new(),
        release: true,
    }
}

#[test]
fn course_sources_resolve_changed_copies_before_metadata_and_preload() {
    let data = ProfileTestDir::new();
    let (mut boot, path, copy) = super::boot_chart::registered_charts(&data);
    let original_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let copy_id = boot.library_db.chart_id_by_chart_file_path(&copy).unwrap().unwrap();
    let sha256 = boot.library_db.chart_sha256_by_chart_id(original_id).unwrap().unwrap();
    let definition = course_definition(original_id, sha256);
    let course_id = boot.library_db.upsert_course("test", &definition, 0, 1).unwrap();
    std::fs::write(&path, "#TITLE Changed\n#BPM 180\n#00012:01\n").unwrap();

    // The lightweight repair still sees a readable file; starting a course must
    // resolve its contents before choosing any stage's metadata or asset folder.
    boot.library_db.repair_course_entry_chart_links_for_course(course_id).unwrap();
    let mut definition = boot.library_db.course_by_id(course_id).unwrap().unwrap().definition;
    assert_eq!(definition.entries[0].chart_id, Some(original_id));
    assert!(
        crate::screens::play_session::scored_chart_metrics_for_chart(
            &boot.library_db,
            original_id,
            &Default::default(),
        )
        .is_err()
    );

    resolve_course_chart_sources(&boot.library_db, &mut definition, None).unwrap();
    assert!(definition.entries.iter().all(|entry| entry.chart_id == Some(copy_id)));
    let snapshot = course_play_metrics_from_library_metadata(
        &boot.library_db,
        &definition,
        boot.profile_config.play.ln_mode_policy,
        &[PlayStartOptions::default(), PlayStartOptions::default()],
    )
    .unwrap();
    assert_eq!(snapshot.first_chart.chart_id, copy_id);
    for entry in &definition.entries {
        let id = entry.chart_id.unwrap();
        assert_eq!(boot.library_db.chart_sha256_by_chart_id(id).unwrap(), Some(sha256));
        assert!(
            crate::screens::play_session::scored_chart_metrics_for_chart(
                &boot.library_db,
                id,
                &Default::default(),
            )
            .is_ok()
        );
    }
}

#[test]
fn course_sources_require_unavailable_stages_only_when_they_will_be_played() {
    let data = ProfileTestDir::new();
    let (mut boot, path, copy) = super::boot_chart::registered_charts(&data);
    let copy_id = boot.library_db.chart_id_by_chart_file_path(&copy).unwrap().unwrap();
    let original_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let sha256 = boot.library_db.chart_sha256_by_chart_id(original_id).unwrap().unwrap();
    let missing_path = path.with_file_name("missing.bms");
    std::fs::write(&missing_path, "#TITLE Missing\n#BPM 120\n#00011:01\n").unwrap();
    let missing_id = crate::storage::import::import_chart_file(
        &mut boot.library_db,
        &missing_path,
        None,
        None,
        1,
    )
    .unwrap()
    .chart_id;
    let mut definition = course_definition(original_id, sha256);
    definition.entries[1].chart_id = Some(missing_id);
    definition.entries[1].sha256 = boot
        .library_db
        .chart_sha256_by_chart_id(missing_id)
        .unwrap()
        .map(|hash| hash_to_hex(&hash));
    std::fs::write(&path, "#TITLE Changed\n#BPM 180\n#00012:01\n").unwrap();
    let course_id = boot.library_db.upsert_course("test", &definition, 0, 1).unwrap();
    std::fs::write(&missing_path, "#TITLE Changed later stage\n#BPM 180\n#00012:01\n").unwrap();
    // Match the real replay launch: link repair retains readable copies, then
    // source verification detects their changed hashes only for played stages.
    boot.library_db.repair_course_entry_chart_links_for_course(course_id).unwrap();
    let mut definition = boot.library_db.course_by_id(course_id).unwrap().unwrap().definition;
    let before = definition.clone();
    assert!(resolve_course_chart_sources(&boot.library_db, &mut definition, None).is_err());
    assert_eq!(definition, before);

    // A replay that failed at stage 1 never loads stage 2, but still needs its
    // metadata for the course-wide score denominator and title list.
    resolve_course_chart_sources(&boot.library_db, &mut definition, Some(1)).unwrap();
    assert_eq!(definition.entries[0].chart_id, Some(copy_id));
    assert_eq!(definition.entries[1], before.entries[1]);
    let snapshot = course_play_metrics_from_library_metadata(
        &boot.library_db,
        &definition,
        boot.profile_config.play.ln_mode_policy,
        &[PlayStartOptions::default(), PlayStartOptions::default()],
    )
    .unwrap();
    assert_eq!(snapshot.first_chart.chart_id, copy_id);
    assert!(snapshot.titles.contains_key(&missing_id));
    assert!(
        crate::screens::play_session::scored_chart_metrics_for_chart(
            &boot.library_db,
            copy_id,
            &Default::default(),
        )
        .is_ok()
    );
}
