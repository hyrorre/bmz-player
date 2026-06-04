pub mod bms_rs_adapter;
pub mod bmson_adapter;
pub mod bmson_timing;
pub mod decode;
pub mod error;
pub mod intermediate;
pub mod long_note;
pub mod normalize;

use std::path::Path;

use crate::model::PlayableChart;

use self::error::{ImportError, ImportWarning};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChartFileFormat {
    Bms,
    Bmson,
    Pms,
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub chart: PlayableChart,
    pub warnings: Vec<ImportWarning>,
}

/// 拡張子に応じて BMS / BMSON を import する。
pub fn import_chart(
    path: &Path,
    random_seed: Option<u64>,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let intermediate = match chart_file_format(path) {
        ChartFileFormat::Bmson => bmson_adapter::import_bmson_to_intermediate(path, &mut warnings)?,
        ChartFileFormat::Bms => {
            bms_rs_adapter::import_bms_to_intermediate(path, random_seed, &mut warnings)?
        }
        ChartFileFormat::Pms => {
            bms_rs_adapter::import_pms_to_intermediate(path, random_seed, &mut warnings)?
        }
    };
    let chart =
        normalize::normalize_chart(path, intermediate, &mut warnings, check_resource_existence)?;
    Ok(ImportResult { chart, warnings })
}

pub fn import_bms_chart(
    path: &Path,
    random_seed: Option<u64>,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    import_chart(path, random_seed, check_resource_existence)
}

fn chart_file_format(path: &Path) -> ChartFileFormat {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
    {
        Some(ext) if ext == "bmson" => ChartFileFormat::Bmson,
        Some(ext) if ext == "pms" => ChartFileFormat::Pms,
        _ => ChartFileFormat::Bms,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bmz_core::lane::Lane;

    use crate::hash::compute_chart_identity;
    use crate::model::{BgaEventKind, NoteKind, TimingEventKind};

    use super::*;

    #[test]
    fn imports_basic_7k_bms_into_playable_chart() {
        let text = "\
#TITLE Integration Song
#ARTIST Test Artist
#BPM 120
#TOTAL 200
#WAV01 key.wav
#WAV02 bgm.wav
#BMP01 bga.png
#BPM01 180
#STOP01 192
#00001:0002
#00004:0001
#00011:0100
#00013:0001
#00108:0100
#00109:0100
#00111:01
";
        let path = write_temp_bms(text);
        let base_dir = path.parent().unwrap().to_path_buf();
        write_temp_file(&base_dir.join("key.wav"));
        write_temp_file(&base_dir.join("bgm.wav"));
        write_temp_file(&base_dir.join("bga.png"));

        let result = import_bms_chart(&path, None, true).unwrap();
        let expected_identity = compute_chart_identity(text.as_bytes());

        assert!(result.warnings.is_empty(), "warnings: {:?}", result.warnings);
        assert_eq!(result.chart.identity, expected_identity);
        assert_eq!(result.chart.metadata.title, "Integration Song");
        assert_eq!(result.chart.metadata.artist, "Test Artist");
        assert_eq!(result.chart.total_notes, 3);
        assert_eq!(result.chart.sounds.len(), 2);
        assert_eq!(result.chart.bga_assets.len(), 1);
        assert_eq!(result.chart.bgm_events.len(), 1);
        assert_eq!(result.chart.bga_events.len(), 1);
        assert_eq!(result.chart.bga_events[0].kind, BgaEventKind::Base);
        assert_eq!(result.chart.notes_for_lane(Lane::Key1).len(), 2);
        assert_eq!(result.chart.notes_for_lane(Lane::Key3).len(), 1);

        let first = &result.chart.notes_for_lane(Lane::Key1)[0];
        assert_eq!(first.kind, NoteKind::Tap);
        assert_eq!(first.time.0, 0);
        assert!(first.sound.is_some());

        let second = &result.chart.notes_for_lane(Lane::Key3)[0];
        assert_eq!(second.kind, NoteKind::Tap);
        assert_eq!(second.time.0, 1_000_000);

        assert!(result.chart.timing_events.iter().any(|event| matches!(
            event.kind,
            TimingEventKind::BpmChange { bpm } if bpm == 180.0
        )));
        // STOP 値 192 (1 measure) を BPM 120 で適用 → 2_000_000us (beatoraja 準拠)。
        assert!(result.chart.timing_events.iter().any(|event| matches!(
            event.kind,
            TimingEventKind::Stop { duration_us } if duration_us == 2_000_000
        )));

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(base_dir.join("key.wav")).unwrap();
        std::fs::remove_file(base_dir.join("bgm.wav")).unwrap();
        std::fs::remove_file(base_dir.join("bga.png")).unwrap();
    }

    #[test]
    fn imports_mine_notes_with_damage() {
        let text = "\
#TITLE Mine Song
#BPM 120
#TOTAL 200
#001D1:0008000C
";
        let path = write_temp_bms(text);
        let result = import_bms_chart(&path, None, false).unwrap();

        let mines: Vec<_> = result
            .chart
            .notes_for_lane(Lane::Key1)
            .iter()
            .filter(|n| n.kind == NoteKind::Mine)
            .collect();
        assert_eq!(mines.len(), 2);
        assert_eq!(mines[0].damage, Some(8));
        assert_eq!(mines[1].damage, Some(12));
        // total_notes は Tap/LongStart のみ。Mine はスコア対象外。
        assert_eq!(result.chart.total_notes, 0);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn imports_random_branch_with_deterministic_seed() {
        // RANDOM 2 / IF 1 を含むので、seed=1 で同じ結果になることを確認する。
        let text = "\
#TITLE Random Song
#BPM 120
#TOTAL 200
#00011:01010101
#RANDOM 2
#IF 1
#00211:01010101
#ENDIF
#ENDRANDOM
";
        let path = write_temp_bms(text);
        let result_a = import_bms_chart(&path, Some(1), false).unwrap();
        let result_b = import_bms_chart(&path, Some(1), false).unwrap();
        assert_eq!(
            result_a.chart.notes_for_lane(Lane::Key1).len(),
            result_b.chart.notes_for_lane(Lane::Key1).len(),
            "fixed seed should give identical note count"
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn classifies_bms_rs_warning_with_code() {
        // `#LNOBJ ZZ` を指定しているのに `ZZ` を参照する行が無いため
        // ParseUndefinedObject 警告が出る。
        let text = "\
#TITLE Diagnostic
#BPM 120
#TOTAL 200
#LNOBJ ZZ
#00011:01
";
        let path = write_temp_bms(text);
        let result = import_bms_chart(&path, None, false).unwrap();
        let has_undefined = result.warnings.iter().any(|w| {
            matches!(
                w,
                crate::import::error::ImportWarning::ParserDiagnostic { code, .. }
                    if code == "ParseUndefinedObject"
            )
        });
        assert!(has_undefined, "warnings: {:?}", result.warnings);
        std::fs::remove_file(&path).unwrap();
    }

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
        assert_eq!(
            result.chart.speed_events.len(),
            1,
            "speed events: {:?}",
            result.chart.speed_events
        );
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
        assert!(result.chart.text_events.iter().any(|event| event.text == "Hello World"));
        assert!(result.chart.text_events.iter().any(|event| event.text == "Test Message"));
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
        assert!(result.chart.timing_events.iter().any(|event| {
            matches!(event.kind, TimingEventKind::BpmChange { bpm } if bpm == 180.0)
        }));
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
        assert_eq!(result.chart.metadata.title, "Bmson Song");
        assert_eq!(result.chart.metadata.long_note_mode, crate::model::LongNoteMode::Ln);
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
            "sound_channels": []
        }"#;
        let path = write_temp_file_with_ext(json, "bmson");
        let result = import_chart(&path, None, false).unwrap();
        assert_eq!(result.chart.metadata.long_note_mode, crate::model::LongNoteMode::Hcn);
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

    fn write_temp_file_with_ext(text: &str, ext: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-chart-import-{}-{stamp}.{ext}", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
        path
    }

    fn write_temp_bms(text: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-chart-import-{}-{stamp}.bms", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.sync_all().unwrap();
        path
    }

    fn write_temp_file(path: &Path) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"").unwrap();
        file.sync_all().unwrap();
    }
}
