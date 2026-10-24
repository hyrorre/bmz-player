use bmz_chart::model::PlayableChart;
use bmz_core::clear::{ClearType, GaugeType};

use crate::gauge::GaugeState;
use crate::score::{ScoreState, compute_clear_type, scored_note_count};
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
        Self::from_states_with_total_notes(
            chart,
            score,
            gauge,
            scored_note_count(chart),
            state,
            autoplay,
        )
    }

    pub fn from_states_with_total_notes(
        chart: &PlayableChart,
        score: &ScoreState,
        gauge: &GaugeState,
        scored_total_notes: u32,
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
            total_notes: scored_total_notes,
            score: score.clone(),
            autoplay,
        }
    }

    pub fn record_bp(&self) -> u32 {
        if self.clear_type == ClearType::Failed {
            self.score.bp_with_unprocessed_notes(self.total_notes)
        } else {
            self.score.bp()
        }
    }

    pub fn record_cb(&self) -> u32 {
        if self.clear_type == ClearType::Failed {
            self.score.cb_with_unprocessed_notes(self.total_notes)
        } else {
            self.score.cb()
        }
    }
}

#[cfg(test)]
mod tests {
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{
        ChartMetadata, LongNoteMode, LongNotePair, LongNoteStyle, PlayableChart,
    };
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

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
            affects_score: true,
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
    fn failed_play_result_counts_unprocessed_notes_for_record_bp_and_cb() {
        let chart = chart();
        let score = ScoreState::default();
        let gauge = gauge(0.0);

        let result = PlayResult::from_states(&chart, &score, &gauge, PlayState::Failed, false);

        assert_eq!(result.total_notes, chart.total_notes);
        assert_eq!(result.record_bp(), chart.total_notes);
        assert_eq!(result.record_cb(), chart.total_notes);
    }

    #[test]
    fn play_result_counts_cn_end_as_scored_note() {
        let mut chart = chart();
        chart.long_notes.push(LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::LnObj,
            mode: Some(LongNoteMode::Cn),
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(0),
            end_time: TimeUs(1_000_000),
            sound: None,
        });
        let score = ScoreState::default();
        let gauge = gauge(0.0);

        let result = PlayResult::from_states(&chart, &score, &gauge, PlayState::Failed, false);

        assert_eq!(chart.total_notes, 1);
        assert_eq!(result.total_notes, 2);
        assert_eq!(result.record_bp(), 2);
        assert_eq!(result.record_cb(), 2);
    }

    #[test]
    fn play_result_uses_auto_shift_result_gauge() {
        let chart = chart();
        let score = ScoreState { past_notes: 1, ..Default::default() };
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hazard)
            .unwrap()
            .value = 0.0;
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

    fn judged_score(judge: Judge) -> ScoreState {
        let mut score = ScoreState::default();
        score.apply(&JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(0),
            affects_score: true,
        });
        score
    }

    fn assist_easy_gauge(value: f32) -> GaugeState {
        let mut gauge = GaugeState::new(GaugeType::AssistEasy, 160.0, 1000);
        gauge.set_initial_value(value);
        gauge
    }

    /// GAS の全ゲージを 0 にしてから、指定ゲージだけをクリア値にする。
    fn auto_shift_gauge_with_only(qualified: GaugeType, bottom: GaugeType) -> GaugeState {
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);
        gauge.set_bottom_shiftable_gauge(bottom);
        for gauge in &mut gauge.gauges {
            gauge.value = 0.0;
        }
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == qualified)
            .unwrap()
            .value = 100.0;
        gauge.selected = bottom;
        gauge
    }

    #[test]
    fn assist_easy_gauge_clear_uses_light_assist_lamp() {
        let chart = chart();
        // 1ノーツを見逃して FC を切る。
        let not_full_combo = judged_score(Judge::Poor);

        let result = PlayResult::from_states(
            &chart,
            &not_full_combo,
            &assist_easy_gauge(100.0),
            PlayState::Finished,
            false,
        );

        assert_eq!(result.clear_type, ClearType::LightAssistEasy);
        assert_eq!(result.gauge_type, GaugeType::AssistEasy);
    }

    #[test]
    fn assist_easy_gauge_keeps_combo_lamps_and_failed() {
        let chart = chart();
        let gauge = assist_easy_gauge(100.0);
        for (judge, expected) in [
            (Judge::Good, ClearType::FullCombo),
            (Judge::Great, ClearType::Perfect),
            (Judge::PGreat, ClearType::Max),
        ] {
            let result = PlayResult::from_states(
                &chart,
                &judged_score(judge),
                &gauge,
                PlayState::Finished,
                false,
            );
            assert_eq!(result.clear_type, expected, "{judge:?}");
        }

        let below_border = PlayResult::from_states(
            &chart,
            &judged_score(Judge::Poor),
            &assist_easy_gauge(10.0),
            PlayState::Finished,
            false,
        );
        assert_eq!(below_border.clear_type, ClearType::Failed);
        let closed = PlayResult::from_states(
            &chart,
            &judged_score(Judge::PGreat),
            &gauge,
            PlayState::Failed,
            false,
        );
        assert_eq!(closed.clear_type, ClearType::Failed);
    }

    #[test]
    fn auto_shift_clear_lamp_follows_final_result_gauge() {
        let chart = chart();
        let score = judged_score(Judge::Poor);
        for (qualified, expected) in [
            (GaugeType::AssistEasy, ClearType::LightAssistEasy),
            (GaugeType::Easy, ClearType::Easy),
            (GaugeType::Normal, ClearType::Normal),
            (GaugeType::Hard, ClearType::Hard),
            (GaugeType::ExHard, ClearType::ExHard),
        ] {
            let gauge = auto_shift_gauge_with_only(qualified, GaugeType::AssistEasy);

            let result =
                PlayResult::from_states(&chart, &score, &gauge, PlayState::Finished, false);

            assert_eq!(result.clear_type, expected, "{qualified:?}");
            assert_eq!(result.gauge_type, qualified);
        }
    }

    #[test]
    fn auto_shift_bottom_gauge_still_blocks_assist_easy_lamp() {
        let chart = chart();
        let score = judged_score(Judge::Poor);
        let gauge = auto_shift_gauge_with_only(GaugeType::AssistEasy, GaugeType::Easy);

        let result = PlayResult::from_states(&chart, &score, &gauge, PlayState::Finished, false);

        assert_eq!(result.clear_type, ClearType::Failed);
        assert_eq!(result.gauge_type, GaugeType::Easy);
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
            bottom_shiftable_gauge: GaugeType::AssistEasy,
            gauges: vec![SingleGaugeState {
                definition: GaugeRuntimeDefinition {
                    gauge_type: GaugeType::Normal,
                    clear_type: Some(ClearType::Normal),
                    min: 0.0,
                    max: 100.0,
                    init: 20.0,
                    border: 80.0,
                    death: 0.0,
                    values: [0.0; 6],
                    guts: &[],
                },
                value,
            }],
        }
    }
}
