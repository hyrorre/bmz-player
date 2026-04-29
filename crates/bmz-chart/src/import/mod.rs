pub mod bms_adapter;
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
) -> Result<ImportResult, ImportError> {
    let mut warnings = Vec::new();
    let intermediate = bms_adapter::import_bms_to_intermediate(path, random_seed, &mut warnings)?;
    let chart = normalize::normalize_chart(path, intermediate, &mut warnings)?;
    Ok(ImportResult { chart, warnings })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bmz_core::lane::Lane;

    use crate::hash::compute_chart_identity;
    use crate::model::{NoteKind, TimingEventKind};

    use super::*;

    #[test]
    fn imports_basic_7k_bms_into_playable_chart() {
        let text = "\
#TITLE Integration Song
#ARTIST Test Artist
#BPM 120
#WAV01 key.wav
#WAV02 bgm.wav
#BPM01 180
#STOP01 192
#00001:0002
#00011:0100
#00013:0001
#00108:0100
#00109:0100
#00111:01
";
        let path = write_temp_bms(text);

        let result = import_bms_chart(&path, None).unwrap();
        let expected_identity = compute_chart_identity(text.as_bytes());

        assert!(result.warnings.is_empty());
        assert_eq!(result.chart.identity, expected_identity);
        assert_eq!(result.chart.metadata.title, "Integration Song");
        assert_eq!(result.chart.metadata.artist, "Test Artist");
        assert_eq!(result.chart.total_notes, 3);
        assert_eq!(result.chart.sounds.len(), 2);
        assert_eq!(result.chart.bgm_events.len(), 1);
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
        assert!(result.chart.timing_events.iter().any(|event| matches!(
            event.kind,
            TimingEventKind::Stop { duration_us } if duration_us == 100_000
        )));

        std::fs::remove_file(path).unwrap();
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
}
