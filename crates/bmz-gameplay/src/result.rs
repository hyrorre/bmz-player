use bmz_chart::model::PlayableChart;
use bmz_core::clear::{ClearType, GaugeType};

use crate::gauge::GaugeState;
use crate::score::{ScoreState, compute_clear_type};
use crate::session::PlayState;

#[derive(Debug, Clone)]
pub struct PlayResult {
    pub chart_sha256: [u8; 32],
    pub clear_type: ClearType,
    pub gauge_type: GaugeType,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub score: ScoreState,
    pub autoplay: bool,
}

impl PlayResult {
    pub fn from_states(
        chart: &PlayableChart,
        score: &ScoreState,
        gauge: &GaugeState,
        state: PlayState,
        autoplay: bool,
    ) -> Self {
        let failed = state == PlayState::Failed;
        let result_gauge = gauge.result_gauge();
        Self {
            chart_sha256: chart.identity.file_sha256,
            clear_type: compute_clear_type(failed, score, gauge),
            gauge_type: result_gauge.definition.gauge_type,
            gauge_value: result_gauge.value,
            total_notes: chart.total_notes,
            score: score.clone(),
            autoplay,
        }
    }
}

#[cfg(test)]
mod tests {
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use super::*;
    use crate::gauge::{GaugeRuntimeDefinition, SingleGaugeState};
    use crate::judge::model::JudgementEvent;

    #[test]
    fn play_result_computes_clear_type_and_copies_score() {
        let chart = chart();
        let mut score = ScoreState::default();
        score.apply(&JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(0),
        });
        let gauge = gauge(82.0);

        let result = PlayResult::from_states(&chart, &score, &gauge, PlayState::Finished, false);

        assert_eq!(result.chart_sha256, chart.identity.file_sha256);
        assert_eq!(result.clear_type, ClearType::Max);
        assert_eq!(result.gauge_type, GaugeType::Normal);
        assert_eq!(result.gauge_value, 82.0);
        assert_eq!(result.score.ex_score(), 2);
        assert!(!result.autoplay);
    }

    #[test]
    fn play_result_uses_auto_shift_result_gauge() {
        let chart = chart();
        let mut score = ScoreState::default();
        score.past_notes = 1;
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
            .unwrap()
            .value = 0.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
            .unwrap()
            .value = 0.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Normal)
            .unwrap()
            .value = 70.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Easy)
            .unwrap()
            .value = 82.0;

        let result = PlayResult::from_states(&chart, &score, &gauge, PlayState::Finished, false);

        assert_eq!(result.clear_type, ClearType::Easy);
        assert_eq!(result.gauge_type, GaugeType::Easy);
        assert_eq!(result.gauge_value, 82.0);
    }

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"chart"),
            metadata: ChartMetadata {
                title: "chart".to_string(),
                initial_bpm: 120.0,
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_000_000),
        }
    }

    fn gauge(value: f32) -> GaugeState {
        GaugeState {
            selected: GaugeType::Normal,
            original: GaugeType::Normal,
            auto_shift: false,
            auto_shift_mode: crate::gauge::GaugeAutoShiftMode::Off,
            gauges: vec![SingleGaugeState {
                definition: GaugeRuntimeDefinition {
                    gauge_type: GaugeType::Normal,
                    clear_type: Some(ClearType::Normal),
                    min: 0.0,
                    max: 100.0,
                    init: 20.0,
                    border: 80.0,
                    values: [0.0; 6],
                    guts: &[],
                },
                value,
            }],
        }
    }
}
