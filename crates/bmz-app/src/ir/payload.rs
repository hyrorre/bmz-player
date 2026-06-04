use bmz_chart::model::{LongNoteMode, PlayableChart};
use bmz_gameplay::result::PlayResult;

use crate::ln_policy::{ChartLnProfile, LnScorePolicy};
use crate::storage::common::hash_to_hex;

use super::types::{
    IrChartFeatures, IrChartLnProfile, IrChartNotes, IrChartPayload, IrClientInfo,
    IrEffectiveLnMode, IrJudgePayload, IrJudgeSidePayload, IrResultPayload, IrRulePayload,
    IrScoreSubmission,
};

#[derive(Debug, Clone)]
pub struct IrSubmissionContext {
    pub played_at: i64,
    pub ln_policy: LnScorePolicy,
    pub effective_ln_mode: LongNoteMode,
    pub gauge_option: String,
    pub idempotency_key: String,
}

pub fn build_score_submission(
    chart: &PlayableChart,
    result: &PlayResult,
    context: IrSubmissionContext,
) -> IrScoreSubmission {
    let judges = &result.score.judges;
    let ln_profile = ChartLnProfile::from_chart(chart);
    IrScoreSubmission {
        client: IrClientInfo {
            name: "BMZ".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        },
        chart: IrChartPayload {
            sha256: hash_to_hex(&result.chart_sha256),
            md5: None,
            ln_profile: IrChartLnProfile {
                has_undefined_ln: ln_profile.has_undefined_ln,
                has_defined_ln: ln_profile.has_defined_ln,
                has_defined_cn: ln_profile.has_defined_cn,
                has_defined_hcn: ln_profile.has_defined_hcn,
            },
            title: chart.metadata.title.clone(),
            subtitle: chart.metadata.subtitle.clone(),
            genre: chart.metadata.genre.clone(),
            artist: chart.metadata.artist.clone(),
            subartists: subartists(&chart.metadata.subartist),
            mode: key_mode_payload(chart.metadata.key_mode.as_str()),
            notes: IrChartNotes {
                total: chart.total_notes,
                ln: chart.long_notes.len() as u32,
                cn: chart
                    .long_notes
                    .iter()
                    .filter(|pair| pair.mode == Some(LongNoteMode::Cn))
                    .count() as u32,
                hcn: chart
                    .long_notes
                    .iter()
                    .filter(|pair| pair.mode == Some(LongNoteMode::Hcn))
                    .count() as u32,
                mine: chart.lane_notes.iter().flatten().filter(|note| note.damage.is_some()).count()
                    as u32,
            },
            features: IrChartFeatures {
                random: false,
                stop: chart.timing_events.iter().any(|event| {
                    matches!(event.kind, bmz_chart::model::TimingEventKind::Stop { .. })
                }),
                ln: ln_profile.has_undefined_ln || ln_profile.has_defined_ln,
                cn: ln_profile.has_defined_cn,
                hcn: ln_profile.has_defined_hcn,
                mine: chart.lane_notes.iter().flatten().any(|note| note.damage.is_some()),
            },
        },
        rule: IrRulePayload {
            play_mode: play_mode_payload(chart.metadata.key_mode.as_str()),
            key_mode: key_mode_payload(chart.metadata.key_mode.as_str()),
            gauge: context.gauge_option,
            ln_policy: context.ln_policy,
            effective_ln_mode: effective_ln_mode_payload(context.effective_ln_mode),
            judge_algorithm: "bmz_v1".to_string(),
            scoring: "bms_ex_score_v1".to_string(),
        },
        result: IrResultPayload {
            clear: result.clear_type.as_str().to_string(),
            played_at: context.played_at,
            judges: IrJudgePayload {
                fast: IrJudgeSidePayload {
                    pgreat: judges.fast_pgreat,
                    great: judges.fast_great,
                    good: judges.fast_good,
                    bad: judges.fast_bad,
                    poor: judges.fast_poor,
                    empty_poor: judges.fast_empty_poor,
                },
                slow: IrJudgeSidePayload {
                    pgreat: judges.slow_pgreat,
                    great: judges.slow_great,
                    good: judges.slow_good,
                    bad: judges.slow_bad,
                    poor: judges.slow_poor,
                    empty_poor: judges.slow_empty_poor,
                },
            },
            ex_score: result.score.ex_score(),
            max_combo: result.score.max_combo,
            notes: result.total_notes,
            pass_notes: result.score.past_notes,
            min_bp: result.score.bp(),
            min_cb: result.score.cb(),
        },
        play_options: Default::default(),
        replay: None,
        evidence: Default::default(),
        idempotency_key: context.idempotency_key,
    }
}

fn effective_ln_mode_payload(mode: LongNoteMode) -> IrEffectiveLnMode {
    match mode {
        LongNoteMode::Ln => IrEffectiveLnMode::Ln,
        LongNoteMode::Cn => IrEffectiveLnMode::Cn,
        LongNoteMode::Hcn => IrEffectiveLnMode::Hcn,
    }
}

fn key_mode_payload(value: &str) -> String {
    match value {
        "5K" => "keys_5",
        "7K" => "keys_7",
        "10K" => "keys_10",
        "14K" => "keys_14",
        _ => "unknown",
    }
    .to_string()
}

fn play_mode_payload(value: &str) -> String {
    match value {
        "10K" | "14K" => "double",
        _ => "single",
    }
    .to_string()
}

fn subartists(value: &str) -> Vec<String> {
    if value.is_empty() { Vec::new() } else { vec![value.to_string()] }
}

#[cfg(test)]
mod tests {
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::lane::KeyMode;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;

    use super::*;

    #[test]
    fn build_score_submission_uses_bmz_bp_and_cb() {
        let chart = PlayableChart {
            identity: ChartIdentity { file_md5: [1; 16], file_sha256: [2; 32] },
            metadata: ChartMetadata {
                title: "Title".to_string(),
                key_mode: KeyMode::K7,
                ..ChartMetadata::default()
            },
            lane_notes: Default::default(),
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
            bga_asset_by_bmp_key: Default::default(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: bmz_core::time::TimeUs(0),
        };
        let mut score = ScoreState::default();
        score.judges.fast_bad = 1;
        score.judges.slow_poor = 2;
        score.judges.fast_empty_poor = 3;
        let result = PlayResult {
            chart_sha256: [2; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes: 1,
            score,
            autoplay: false,
        };

        let payload = build_score_submission(
            &chart,
            &result,
            IrSubmissionContext {
                played_at: 100,
                ln_policy: LnScorePolicy::ForceLn,
                effective_ln_mode: LongNoteMode::Ln,
                gauge_option: "normal".to_string(),
                idempotency_key: "score_local_1".to_string(),
            },
        );

        assert_eq!(payload.result.min_bp, 6);
        assert_eq!(payload.result.min_cb, 3);
        assert_eq!(payload.rule.ln_policy, LnScorePolicy::ForceLn);
        assert_eq!(payload.rule.key_mode, "keys_7");
    }
}
