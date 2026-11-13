use bmz_chart::import::{BmsRandomSource, import_chart_bytes_with_random_source};
use bmz_chart::model::{NoteKind, PlayableChart};
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::time::TimeUs;
use std::path::Path;

fn bms(name: &str, body: &str) -> PlayableChart {
    let text = format!("#BPM 120\n#WAV01 a.wav\n{body}");
    import_chart_bytes_with_random_source(
        Path::new(name),
        text.as_bytes(),
        BmsRandomSource::Seed(Some(1)),
        false,
    )
    .unwrap()
    .chart
}

#[test]
fn partial_headers_preserve_original_identity_and_exclude_inactive_random_branches() {
    let partial = bms("x.bmc", "#HLNOBJ ?\n#LNMODE 9\n#BMD music.mid\n#00111:01");
    assert!(partial.metadata.kaleid.partial());
    assert_eq!(partial.total_notes, 1);
    let unused = bms("x.bms", "#HLNOBJ ZZ\n#00111:01");
    assert!(unused.metadata.kaleid.declared.hln);
    assert!(!unused.metadata.kaleid.source.hln);
    let inactive = bms(
        "x.bms",
        "#SETRANDOM 1\n#IF 2\n#BMD music.mid\n#001F1:01\n#ENDIF\n#ENDRANDOM\n#00111:01",
    );
    assert!(!inactive.metadata.kaleid.source.potion);
    assert!(!inactive.metadata.kaleid.partial());
}

#[test]
fn branch_hln_voices_keep_resource_identity_when_slices_change() {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version":"1.0.0", "info":{"title":"HLN", "artist":"", "genre":"", "level":1,
            "init_bpm":120, "resolution":240, "mode_hint":"beat-7k"},
        "sound_channels":[], "conditional_blocks":[
            {"y":0, "branches":[{"when":"AUTO==1", "sound_channels":[{"name":"same.wav", "notes":[
                {"x":1,"y":960,"l":120,"t":4,"c":false}, {"x":2,"y":1440,"l":120,"t":4,"c":true}]}]}, {"default":true}]},
            {"y":960, "branches":[{"when":"AUTO==1","bpm_events":[{"y":960,"bpm":240}]},{"default":true}]}
        ]
    })).unwrap();
    let chart = import_chart_bytes_with_random_source(
        Path::new("x.bmson"),
        &bytes,
        BmsRandomSource::Seed(None),
        false,
    )
    .unwrap()
    .chart;
    let program = chart.metadata.conditional.as_ref().unwrap();
    assert!(program.source_ln_modes & 16 != 0);
    assert!(chart.metadata.kaleid.source.hln);
    let selected = program.materialize(&[Some(0), Some(0)]).unwrap();
    assert_eq!(selected.long_notes.len(), 2);
    for pair in &selected.long_notes {
        assert!(chart.sounds.iter().any(|asset| Some(asset.id) == pair.sound));
    }
    let second =
        selected.sounds.iter().find(|a| Some(a.id) == selected.long_notes[1].sound).unwrap();
    assert_eq!(second.slice.unwrap().start_us, 500_000);
}

#[test]
fn silent_objects_do_not_inflate_the_score_or_load_marker_sounds() {
    let chart = bms("x.bms", "#001F1:0A\n#001H2:01\n#001J3:0101\n#001L4:01\n#001N5:01\n");
    assert_eq!(chart.total_notes, 0);
    assert_eq!(chart.long_notes.len(), 1);
    assert!(chart.is_dummy_long(&chart.long_notes[0]));
    assert!(chart.lane_notes.iter().flatten().all(|n| n.sound.is_none()));
    assert_eq!(chart.lane_notes[Lane::Key1.index()][0].damage, Some(10.0));
    assert!(chart.metadata.kaleid.source.potion && chart.metadata.kaleid.source.dummy);
}

#[test]
fn real_notes_take_priority_over_potions_and_dummy_shapes() {
    let chart = bms("x.bme", "#00111:01\n#001F1:0A\n#001H1:01\n#001L1:01\n");
    assert_eq!(chart.lane_notes[Lane::Key1.index()].len(), 1);
    assert_eq!(chart.lane_notes[Lane::Key1.index()][0].kind, NoteKind::Tap);
    assert_eq!(chart.total_notes, 1);
}

#[test]
fn base62_potion_recovery_stays_base36() {
    let chart = bms("x.bml", "#BASE 62\n#001F1:0a\n");
    assert_eq!(chart.lane_notes[Lane::Key1.index()][0].damage, Some(10.0));
}

#[test]
fn adjacent_free_zones_merge_and_inner_notes_are_replaced_in_the_count() {
    let chart = bms("x.bmc", "#001SF:01010000\n#00116:01010000\n");
    assert_eq!(chart.metadata.free_zones.len(), 1);
    let zone = &chart.metadata.free_zones[0];
    assert_eq!(zone.start_time, TimeUs(2_000_000));
    assert_eq!(zone.end_time, TimeUs(3_000_000));
    assert_eq!(zone.inner_notes.len(), 2);
    assert_eq!(chart.total_notes, 1);
    assert!(chart.note_by_id(zone.note_id).is_some_and(|n| n.kind == NoteKind::FreeZoneEnd));
}

#[test]
fn free_zone_pair_spans_measures_and_legacy_needs_no_file_metadata() {
    let pair = bms("does-not-exist.bms", "#001SG:01\n#003SG:01\n");
    assert_eq!(pair.metadata.free_zones[0].end_time, TimeUs(6_000_000));
    let legacy = bms("also-not-a-file.bms", "#00117:01\n#00127:01\n");
    assert_eq!(legacy.metadata.free_zones.len(), 2);
    assert_eq!(legacy.metadata.key_mode, KeyMode::K10);
    let pms = bms("ordinary.pms", "#00117:01\n");
    assert!(pms.metadata.free_zones.is_empty());
    assert_eq!(pms.total_notes, 1);
}

#[test]
fn dummy_keys_participate_in_layout_detection_and_st_adds_a_bar() {
    let chart = bms("x.bms", "#001I9:01\n#001ST:0001\n");
    assert_eq!(chart.metadata.key_mode, KeyMode::K14);
    assert!(chart.bar_lines.iter().any(|line| line.time == TimeUs(3_000_000)));
}

#[test]
fn unclosed_visual_ln_and_zone_are_partial_without_fake_notes() {
    let chart = bms("x.bms", "#001J1:01\n#001SG:01\n");
    assert_eq!(chart.total_notes, 0);
    assert!(chart.long_notes.is_empty() && chart.metadata.free_zones.is_empty());
    assert!(chart.metadata.kaleid.partial());
}

#[test]
fn static_unselected_extensions_are_absent() {
    let chart = bms("x.bms", "#SETRANDOM 1\n#IF 2\n#001F1:01\n#ENDIF\n#ENDRANDOM\n");
    assert!(!chart.metadata.kaleid.source.potion);
}

#[test]
fn bmson_extensions_keep_mode_hint_and_overlapping_free_zone_spans() {
    let text = serde_json::json!({
        "version": "1.0.0", "info": { "title": "K", "artist": "", "genre": "", "level": 1, "mode_hint": "beat-7k", "init_bpm": 120, "resolution": 240, "judge_rank": 100, "total": 100 },
        "sound_channels": [],
        "potion_notes": [{"x": 1, "y": 240, "recovery": 12.5}],
        "dummy_notes": [{"x": 2, "y": 240, "l": 240, "t": 4}],
        "free_zone_channels": [{"name": "f.wav", "notes": [{"x": 8, "y": 960, "l": 960}, {"x": 8, "y": 1200, "l": 240}]}],
        "speed_events": [{"y": 960, "rate": 2.0}],
    }).to_string();
    let chart = import_chart_bytes_with_random_source(
        Path::new("x.bmson"),
        text.as_bytes(),
        BmsRandomSource::Seed(None),
        false,
    )
    .unwrap()
    .chart;
    assert_eq!(chart.total_notes, 1);
    assert_eq!(chart.metadata.free_zones.len(), 1);
    assert_eq!(chart.metadata.free_zones[0].end_time, TimeUs(4_000_000));
    assert_eq!(chart.speed_events[0].factor, 2.0);
    assert_eq!(chart.bgm_events.len(), 2);
    assert!(chart.bgm_events.iter().any(|event| event.time == TimeUs(2_000_000)));
    assert_eq!(chart.metadata.key_mode, KeyMode::K7);
}

#[test]
fn bmson_branches_retime_slices_without_changing_source_identity_or_mode_hint() {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": "1.0.0", "info": {"title": "branch", "artist": "", "genre": "", "level": 1,
            "mode_hint": "beat-7k", "init_bpm": 120, "resolution": 240, "judge_rank": 100, "total": 100},
        "sound_channels": [{"name": "common.wav", "notes": [{"x": 1, "y": 0, "l": 0, "c": false}]}],
        "conditional_blocks": [
            {"y": 960, "branches": [
                {"when": "AUTO==1", "bpm_events": [{"y": 960, "bpm": 240}],
                 "lines": [{"y": 1080}],
                 "sound_channels": [{"name": "branch.wav", "notes": [
                     {"x": 2, "y": 960, "l": 0, "c": false}, {"x": 3, "y": 1440, "l": 0, "c": true}]}]},
                {"default": true}]},
            {"y": 1200, "branches": [
                {"when": "AUTO==1", "bpm_events": [{"y": 1200, "bpm": 120}]},
                {"default": true}]}
        ]
    })).unwrap();
    let chart = import_chart_bytes_with_random_source(
        Path::new("x.bmson"),
        &bytes,
        BmsRandomSource::Seed(None),
        false,
    )
    .unwrap()
    .chart;
    assert_eq!(chart.total_notes, 1);
    let program = chart.metadata.conditional.as_ref().unwrap();
    let selected = program.materialize(&[Some(0), Some(0)]).unwrap();
    assert_eq!(selected.identity, chart.identity);
    assert_eq!(selected.metadata.key_mode, KeyMode::K7);
    assert_eq!(selected.total_notes, 3);
    assert_eq!(program.evaluation_time(1, &selected), TimeUs(2_250_000));
    let first = selected.lane_notes[Lane::Key2.index()][0].sound.unwrap();
    let second = selected.lane_notes[Lane::Key3.index()][0].sound.unwrap();
    let first_slice =
        selected.sounds.iter().find(|asset| asset.id == first).unwrap().slice.unwrap();
    let second_slice =
        selected.sounds.iter().find(|asset| asset.id == second).unwrap().slice.unwrap();
    assert_eq!(first_slice.start_us, 0);
    assert_eq!(first_slice.duration_us, Some(750_000));
    assert_eq!(second_slice.start_us, 750_000);
    assert!(selected.bar_lines.iter().any(|line| line.time == TimeUs(2_125_000)));
    assert!(
        selected.sounds.iter().all(|asset| chart.sounds.iter().any(|loaded| loaded.id == asset.id))
    );
}
