pub mod bms_rs_adapter;
pub mod decode;
pub mod error;
pub mod intermediate;
pub mod long_note;
pub mod normalize;

use std::path::Path;

use crate::model::PlayableChart;

use self::error::{ImportError, ImportWarning};

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub chart: PlayableChart,
    pub warnings: Vec<ImportWarning>,
}

pub fn import_bms_chart(
    path: &Path,
    random_seed: Option<u64>,
    check_resource_existence: bool,
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let intermediate =
        bms_rs_adapter::import_bms_to_intermediate(path, random_seed, &mut warnings)?;
    let chart =
        normalize::normalize_chart(path, intermediate, &mut warnings, check_resource_existence)?;
    Ok(ImportResult { chart, warnings })
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
