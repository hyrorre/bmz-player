use super::*;

#[test]
fn imports_scroll_and_speed_events() {
    // SCROLL チャネル (SC) と SPEED チャネル (SP) を含む BMS。
    // bms-rs は `#SCROLLxx` / `#SPEEDxx` 定義と `#xxxSC` / `#xxxSP` 行を
    // 解釈して factor を引き出す。
    let text = "\
#TITLE Scroll Song
#BPM 120
#TOTAL 200
#SCROLL01 2.0
#SCROLL02 0.5
#SPEED01 1.5
#00111:01
#001SC:0102
#001SP:0001
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(
        result.chart.scroll_events.len(),
        2,
        "scroll events: {:?}",
        result.chart.scroll_events
    );
    assert_eq!(result.chart.scroll_events[0].factor, 2.0);
    assert_eq!(result.chart.scroll_events[1].factor, 0.5);
    assert_eq!(result.chart.speed_events.len(), 1, "speed events: {:?}", result.chart.speed_events);
    assert_eq!(result.chart.speed_events[0].factor, 1.5);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_exrank_judge_events() {
    let text = "\
#TITLE Exrank Song
#BPM 120
#TOTAL 200
#RANK 3
#EXRANK01 1
#EXRANK02 0
#00111:01
#001A0:01000000
#002A0:02000000
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.judge_rank_events.len(), 2);
    assert_eq!(result.chart.judge_rank_events[0].rank_percent, 50);
    assert_eq!(result.chart.judge_rank_events[1].rank_percent, 25);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_rank_as_bms_rank_source() {
    let text = "\
#TITLE Rank Song
#BPM 120
#RANK 4
#00111:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.judge_rank, Some(4));
    let spec = result.chart.metadata.judge_rank_spec.unwrap();
    assert_eq!(spec.value, 4);
    assert_eq!(spec.kind, JudgeRankKind::BmsRank);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_defexrank_as_judge_rank_source() {
    let text = "\
#TITLE Defexrank Song
#BPM 120
#RANK 3
#DEFEXRANK 125
#00111:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.judge_rank, Some(125));
    let spec = result.chart.metadata.judge_rank_spec.unwrap();
    assert_eq!(spec.value, 125);
    assert_eq!(spec.kind, JudgeRankKind::DefExRank);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_volwav_and_volume_channels() {
    let text = "\
#TITLE Volume Song
#BPM 120
#TOTAL 200
#VOLWAV 50
#00111:01
#00197:80
#00198:40
#00297:FF
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.volwav_percent, 50);
    assert_eq!(result.chart.bgm_volume_events.len(), 2);
    assert_eq!(result.chart.bgm_volume_events[0].value, 0x80);
    assert_eq!(result.chart.bgm_volume_events[1].value, 0xFF);
    assert_eq!(result.chart.key_volume_events.len(), 1);
    assert_eq!(result.chart.key_volume_events[0].value, 0x40);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_text_events() {
    let text = "\
#TITLE Text Song
#BPM 120
#TOTAL 200
#TEXT01 Hello World
#TEXT02 Test Message
#00111:01
#00199:01000200
#00299:02000100
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.text_events.len(), 4);
    assert!(result.chart.text_events.iter().any(|event| &*event.text == "Hello World"));
    assert!(result.chart.text_events.iter().any(|event| &*event.text == "Test Message"));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bga_opacity_and_argb_events() {
    let text = "\
#TITLE BGA FX Song
#BPM 120
#TOTAL 200
#ARGB01 255,255,0,0
#00111:01
#0010B:80
#001A1:01000000
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.bga_opacity_events.len(), 1);
    assert_eq!(result.chart.bga_opacity_events[0].opacity, 0x80);
    assert_eq!(result.chart.bga_argb_events.len(), 1);
    assert_eq!(result.chart.bga_argb_events[0].red, 255);
    assert_eq!(result.chart.bga_argb_events[0].green, 0);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bga_layer2_separate_from_layer() {
    let text = "\
#TITLE Layer2 Song
#BPM 120
#TOTAL 200
#BMP01 layer.png
#BMP02 layer2.png
#00007:0001
#0010A:0002
#00011:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.bga_events.len(), 2);
    assert_eq!(result.chart.bga_events[0].kind, BgaEventKind::Layer);
    assert_eq!(result.chart.bga_events[1].kind, BgaEventKind::Layer2);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_swbga_and_keybound_events() {
    let text = "\
#TITLE Keybound Song
#BPM 120
#TOTAL 200
#BMP01 f1.png
#BMP02 f2.png
#SWBGA01 100:0:11:0:255,0,0,0 0102
#000A5:01
#00011:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.swbga_definitions.len(), 1);
    assert_eq!(result.chart.swbga_definitions[0].pattern_bmp_keys, vec![1, 2]);
    assert_eq!(result.chart.swbga_definitions[0].line, 11);
    assert_eq!(result.chart.bga_keybound_events.len(), 1);
    assert_eq!(result.chart.bga_keybound_events[0].swbga_id, 1);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn sets_base62_obj_ids_metadata() {
    let text = "\
#TITLE Base62 Flag
#BPM 120
#BASE 62
";
    let path = write_temp_bms(text);
    let mut warnings = Vec::new();
    let intermediate =
        super::bms_rs_adapter::import_bms_to_intermediate(&path, None, &mut warnings).unwrap();
    assert!(intermediate.metadata.base62_obj_ids, "warnings: {warnings:?}");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_base62_swbga_pattern_with_distinct_case_ids() {
    let text = "\
#TITLE Base62 SWBGA
#BPM 120
#TOTAL 200
#BASE 62
#BMPaa aa.png
#BMPAA AA.png
#SWBGA01 100:0:11:0:255,0,0,0 aaAA
#000A5:01
#00011:01
";
    let path = write_temp_bms(text);
    let result = import_bms_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.swbga_definitions.len(), 1);
    assert_eq!(
        result.chart.swbga_definitions[0].pattern_bmp_keys,
        vec![36 * 62 + 36, 10 * 62 + 10]
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_sparse_long_bms_bpm_message_without_dense_expansion() {
    let mut payload = vec!["00"; 10_000];
    payload[9_999] = "01";
    let text = format!(
        "\
#TITLE Sparse BPM
#BPM 120
#TOTAL 200
#BPM01 180
#00108:{}
#00211:01
",
        payload.join("")
    );
    let path = write_temp_bms(&text);
    let result = import_bms_chart(&path, None, false).unwrap();

    assert!(result.warnings.iter().any(|warning| {
        matches!(
            warning,
            ImportWarning::ParserDiagnostic { code, .. } if code == "SparseBmsMessage"
        )
    }));
    assert!(
        result.chart.timing_events.iter().any(|event| {
            matches!(event.kind, TimingEventKind::BpmChange { bpm } if bpm == 180.0)
        })
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_into_playable_chart() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Bmson Song",
            "artist": "Test Artist",
            "genre": "Test",
            "level": 5,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 200.0,
            "resolution": 240
        },
        "sound_channels": []
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.source_format, crate::model::ChartSourceFormat::Bmson);
    assert_eq!(result.chart.metadata.title, "Bmson Song");
    assert_eq!(result.chart.metadata.difficulty_name, "BEGINNER");
    assert_eq!(result.chart.metadata.total, Some(520.0));
    assert_eq!(result.chart.metadata.judge_rank, Some(100));
    assert_eq!(
        result.chart.metadata.judge_rank_spec,
        Some(crate::model::JudgeRankSpec { value: 100, kind: JudgeRankKind::BmsonJudgeRank })
    );
    assert_eq!(result.chart.metadata.long_note_mode, crate::model::LongNoteMode::Ln);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_appends_chart_name_to_subtitle_and_preserves_full_level_value() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Metadata",
            "artist": "Test",
            "genre": "Test",
            "subtitle": "Original",
            "chart_name": "FOUR DIMENSIONS",
            "level": 300,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": []
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");

    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.subtitle, "Original [FOUR DIMENSIONS]");
    assert_eq!(result.chart.metadata.difficulty_name, "BEGINNER");
    assert_eq!(result.chart.metadata.play_level, "300");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_preserves_simultaneous_base_layer_and_poor_bga_events() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Simultaneous BGA",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": [],
        "bga": {
            "bga_header": [
                {"id": 10, "name": "base.png"},
                {"id": 20, "name": "layer.png"},
                {"id": 30, "name": "poor.png"}
            ],
            "bga_events": [{"y": 0, "id": 10}],
            "layer_events": [{"y": 0, "id": 20}],
            "poor_events": [{"y": 0, "id": 30}]
        }
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");

    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.bga_events.len(), 3);
    assert!(result.chart.bga_events.iter().any(|event| event.kind == BgaEventKind::Base));
    assert!(result.chart.bga_events.iter().any(|event| event.kind == BgaEventKind::Layer));
    assert!(result.chart.bga_events.iter().any(|event| event.kind == BgaEventKind::Poor));
    assert!(result.chart.bga_events.iter().all(|event| event.tick.0 == 0 && event.time.0 == 0));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_title_image_fallback_to_backbmp() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Title Image Song",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "back_image": "",
            "title_image": "_Back.png"
        },
        "sound_channels": []
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.backbmp_file, "_Back.png");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_subartists_into_subartist() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Subartist Song",
            "artist": "Main",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "subartists": ["music:Alice", "chart:Bob"]
        },
        "sound_channels": []
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.subartist, "music:Alice / chart:Bob");
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_ln_type_into_long_note_mode() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Hcn Song",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "ln_type": 3
        },
        "sound_channels": [{
            "name": "long.wav",
            "notes": [{"x": 1, "y": 0, "l": 240, "c": false}]
        }]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.long_note_mode, crate::model::LongNoteMode::Hcn);
    assert!(result.chart.metadata.long_note_mode_defined);
    assert_eq!(result.chart.long_notes[0].mode, Some(crate::model::LongNoteMode::Hcn));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_note_type_overrides_global_ln_type_per_note() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Mixed LN",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k",
            "ln_type": 1
        },
        "sound_channels": [
            {"name": "cn.wav", "notes": [{"x": 1, "y": 0, "l": 240, "c": false, "t": 2}]},
            {"name": "hcn.wav", "notes": [{"x": 2, "y": 480, "l": 240, "c": false, "t": 3}]}
        ]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let modes = result.chart.long_notes.iter().map(|pair| pair.mode).collect::<Vec<_>>();
    assert_eq!(
        modes,
        vec![Some(crate::model::LongNoteMode::Cn), Some(crate::model::LongNoteMode::Hcn)]
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_up_note_becomes_long_note_end_sound() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "LN End Sound",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": [
            {"name": "end.wav", "notes": [{"x": 1, "y": 240, "l": 0, "c": false, "up": true}]},
            {"name": "start.wav", "notes": [{"x": 1, "y": 0, "l": 240, "c": false}]}
        ]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let notes = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()];
    assert_eq!(notes.len(), 2, "notes: {notes:?}");
    let end = notes.iter().find(|note| note.kind == crate::model::NoteKind::LongEnd).unwrap();
    let end_sound = end.sound.expect("up note should define the LN end sound");
    assert_eq!(
        result.chart.sounds[end_sound.0 as usize].path.file_name().and_then(|name| name.to_str()),
        Some("end.wav")
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_stop_duration_uses_pulses_at_current_resolution() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Pulse Stop",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "stop_events": [{"y": 240, "duration": 240}],
        "sound_channels": [{
            "name": "key.wav",
            "notes": [
                {"x": 1, "y": 0, "l": 0, "c": false},
                {"x": 1, "y": 480, "l": 0, "c": false}
            ]
        }]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let notes = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()];
    assert_eq!(notes[1].time, bmz_core::time::TimeUs(1_500_000));
    assert!(matches!(
        result.chart.timing_events.as_slice(),
        [crate::model::TimingEvent {
            time: bmz_core::time::TimeUs(500_000),
            kind: crate::model::TimingEventKind::Stop { duration_us: 500_000 },
            ..
        }]
    ));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_stop_at_bpm_change_uses_the_new_bpm() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Changed BPM Stop",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "bpm_events": [{"y": 240, "bpm": 240.0}],
        "stop_events": [{"y": 240, "duration": 240}],
        "sound_channels": [{
            "name": "key.wav",
            "notes": [{"x": 1, "y": 480, "l": 0, "c": false}]
        }]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let note = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()][0];
    assert_eq!(note.time, bmz_core::time::TimeUs(1_000_000));
    assert!(result.chart.timing_events.iter().any(|event| {
        matches!(event.kind, crate::model::TimingEventKind::Stop { duration_us: 250_000 })
    }));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_mine_keeps_fractional_damage_and_channel_sound() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Mine",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "mine_channels": [{
            "name": "mine.wav",
            "notes": [{"x": 1, "y": 240, "damage": 12.5}]
        }],
        "sound_channels": []
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let mine = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()][0];
    assert_eq!(mine.kind, crate::model::NoteKind::Mine);
    assert_eq!(mine.damage, Some(12.5));
    let sound = mine.sound.expect("mine channel name should become a chart sound");
    assert_eq!(
        result.chart.sounds[sound.0 as usize].path.file_name().and_then(|name| name.to_str()),
        Some("mine.wav")
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_sound_channel_c_builds_continuous_file_slices() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "One WAV",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": [{
            "name": "full.wav",
            "notes": [
                {"x": 1, "y": 0, "l": 0, "c": false},
                {"x": 1, "y": 240, "l": 0, "c": true},
                {"x": 1, "y": 480, "l": 0, "c": true},
                {"x": 1, "y": 720, "l": 0, "c": false}
            ]
        }]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let notes = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()];
    let slices = notes
        .iter()
        .map(|note| {
            let sound = note.sound.expect("BMSON sound note should have a slice sound");
            result.chart.sounds[sound.0 as usize].slice
        })
        .collect::<Vec<_>>();
    assert_eq!(
        slices,
        vec![
            Some(crate::model::SoundSlice { start_us: 0, duration_us: Some(500_000) }),
            Some(crate::model::SoundSlice { start_us: 500_000, duration_us: Some(500_000) }),
            Some(crate::model::SoundSlice { start_us: 1_000_000, duration_us: None }),
            Some(crate::model::SoundSlice { start_us: 0, duration_us: None }),
        ]
    );
    let sound_ids =
        notes.iter().filter_map(|note| note.sound).collect::<std::collections::HashSet<_>>();
    assert_eq!(sound_ids.len(), 4);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_bgm_notes_use_the_same_sound_slice_rules() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "One WAV BGM",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": [{
            "name": "full.wav",
            "notes": [
                {"x": 0, "y": 0, "l": 0, "c": false},
                {"x": 0, "y": 240, "l": 0, "c": true}
            ]
        }]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.bgm_events.len(), 2);
    let first = result.chart.bgm_events[0].sound;
    let second = result.chart.bgm_events[1].sound;
    assert_eq!(
        result.chart.sounds[first.0 as usize].slice,
        Some(crate::model::SoundSlice { start_us: 0, duration_us: Some(500_000) })
    );
    assert_eq!(
        result.chart.sounds[second.0 as usize].slice,
        Some(crate::model::SoundSlice { start_us: 500_000, duration_us: None })
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn bmson_same_lane_position_layers_sounds_from_distinct_channels() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Layered keysound",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240,
            "mode_hint": "beat-7k"
        },
        "sound_channels": [
            {"name": "primary.wav", "notes": [{"x": 1, "y": 0, "l": 0, "c": false}]},
            {"name": "layer.wav", "notes": [{"x": 1, "y": 0, "l": 0, "c": false}]}
        ]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");

    let result = import_chart(&path, None, false).unwrap();
    let notes = &result.chart.lane_notes[bmz_core::lane::Lane::Key1.index()];
    assert_eq!(notes.len(), 1);
    let sound_paths = notes[0]
        .sounds()
        .map(|sound_id| {
            result.chart.sounds[sound_id.0 as usize]
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(sound_paths, vec!["primary.wav", "layer.wav"]);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_irregular_meter_lines() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "Irregular",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240
        },
        "lines": [
            { "y": 960 },
            { "y": 1680 },
            { "y": 2640 }
        ],
        "sound_channels": [
            {
                "name": "key.wav",
                "notes": [
                    { "x": 1, "y": 1680, "l": 0, "c": false }
                ]
            }
        ]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    let note = result
        .chart
        .lane_notes
        .iter()
        .flat_map(|lane| lane.iter())
        .find(|note| note.kind == crate::model::NoteKind::Tap)
        .expect("note at pulse 1680");
    assert_eq!(note.tick, bmz_core::time::ChartTick(6_720));
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_bmson_empty_lines_without_bar_lines() {
    let json = r#"{
        "version": "1.0.0",
        "info": {
            "title": "No Barlines",
            "artist": "Test",
            "genre": "Test",
            "level": 1,
            "init_bpm": 120.0,
            "judge_rank": 100.0,
            "total": 100.0,
            "resolution": 240
        },
        "lines": [],
        "sound_channels": [
            {
                "name": "key.wav",
                "notes": [
                    { "x": 1, "y": 960, "l": 0, "c": false }
                ]
            }
        ]
    }"#;
    let path = write_temp_file_with_ext(json, "bmson");
    let result = import_chart(&path, None, false).unwrap();
    assert!(result.chart.bar_lines.is_empty());
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn imports_lnmode_from_bms_header() {
    let text = "\
#TITLE Lnmode Song
#BPM 120
#TOTAL 200
#LNMODE 3
#00011:01
";
    let path = write_temp_bms(text);
    let result = import_chart(&path, None, false).unwrap();
    assert_eq!(result.chart.metadata.long_note_mode, crate::model::LongNoteMode::Hcn);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn import_records_bms_and_pms_source_formats() {
    let text = "#TITLE Source Format\n#BPM 120\n#00111:01\n";
    for (extension, expected) in [
        ("bms", crate::model::ChartSourceFormat::Bms),
        ("pms", crate::model::ChartSourceFormat::Pms),
    ] {
        let path = write_temp_file_with_ext(text, extension);
        let result = import_chart(&path, None, false).unwrap();
        assert_eq!(result.chart.metadata.source_format, expected);
        std::fs::remove_file(path).unwrap();
    }
}
