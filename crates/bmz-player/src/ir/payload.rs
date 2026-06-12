use bmz_chart::model::{LongNoteMode, NoteKind, PlayableChart, TimingEventKind};
use bmz_core::input::InputDeviceKind;
use bmz_gameplay::gauge::gauge_total_for_chart;
use bmz_gameplay::result::PlayResult;

use crate::ln_policy::{ChartLnProfile, LnScorePolicy};
use crate::select_options::{ArrangeOption, DoubleOptionScoreBucket};
use crate::storage::common::hash_to_hex;
use crate::storage::score_db::encode_beatoraja_ghost;

use super::types::{
    IrChartBpm, IrChartFeatures, IrChartLnProfile, IrChartNotes, IrChartPayload, IrChartUrls,
    IrClientInfo, IrEffectiveLnMode, IrJudgePayload, IrJudgeSidePayload, IrResultPayload,
    IrRulePayload, IrScoreSubmission,
};

#[derive(Debug, Clone)]
pub struct IrSubmissionContext {
    pub played_at: i64,
    pub ln_policy: LnScorePolicy,
    pub effective_ln_mode: LongNoteMode,
    pub gauge_option: String,
    pub device_type: InputDeviceKind,
    pub idempotency_key: String,
    pub arrange: ArrangeOption,
    pub double_option: DoubleOptionScoreBucket,
    pub arrange_seed: Option<i64>,
    pub random_seed: Option<i64>,
    pub rule_mode: String,
    /// 保存済みリプレイファイルの SHA256 (hex)。リプレイが無ければ None。
    pub replay_hash: Option<String>,
}

pub fn build_score_submission(
    chart: &PlayableChart,
    result: &PlayResult,
    context: IrSubmissionContext,
) -> IrScoreSubmission {
    let judges = &result.score.judges;
    let mut play_options = std::collections::BTreeMap::new();
    play_options.insert(
        "device_type".to_string(),
        serde_json::Value::String(context.device_type.as_str().to_string()),
    );
    play_options.insert(
        "option".to_string(),
        serde_json::Value::String(arrange_option_ir(context.arrange).to_string()),
    );
    play_options.insert(
        "double_option".to_string(),
        serde_json::Value::String(context.double_option.ir_value().to_string()),
    );
    if context.arrange.uses_seed()
        && let Some(seed) = context.arrange_seed
    {
        play_options.insert("seed".to_string(), serde_json::json!(seed));
    }
    if let Some(seed) = context.random_seed {
        play_options.insert("random_seed".to_string(), serde_json::json!(seed));
    }
    if !context.rule_mode.is_empty() {
        play_options
            .insert("rule_mode".to_string(), serde_json::Value::String(context.rule_mode.clone()));
    }
    let ghost =
        encode_beatoraja_ghost(&result.score.ghost).ok().filter(|encoded| !encoded.is_empty());

    IrScoreSubmission {
        client: IrClientInfo {
            name: "BMZ".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        },
        chart: build_ir_chart_payload(chart),
        rule: IrRulePayload {
            play_mode: play_mode_payload(chart.metadata.key_mode.as_str()),
            key_mode: chart.metadata.key_mode.as_str().to_string(),
            gauge: context.gauge_option,
            ln_policy: context.ln_policy,
            effective_ln_mode: effective_ln_mode_payload(context.effective_ln_mode),
            judge_algorithm: "bmz_v1".to_string(),
            scoring: "bms_ex_score_v1".to_string(),
            rule_mode: context.rule_mode.clone(),
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
            pass_notes: None,
            min_bp: result.record_bp(),
            min_cb: result.record_cb(),
            ghost,
        },
        play_options,
        replay: context.replay_hash.map(|hash| super::types::IrReplayPayload {
            hash,
            format: "bmz-replay-v1".to_string(),
            upload_intent: "later".to_string(),
        }),
        evidence: Default::default(),
        idempotency_key: context.idempotency_key,
    }
}

pub fn build_ir_chart_payload(chart: &PlayableChart) -> IrChartPayload {
    let ln_profile = ChartLnProfile::from_chart(chart);
    let (min_bpm, max_bpm) = chart_bpm_range(chart);
    let gauge_total = gauge_total_for_chart(chart.metadata.total, chart.total_notes);
    let ln_count = chart.long_notes.len() as u32;
    let cn_count =
        chart.long_notes.iter().filter(|pair| pair.mode == Some(LongNoteMode::Cn)).count() as u32;
    let hcn_count =
        chart.long_notes.iter().filter(|pair| pair.mode == Some(LongNoteMode::Hcn)).count() as u32;
    let has_mine = chart.lane_notes.iter().flatten().any(|note| note.kind == NoteKind::Mine);
    let mine_count =
        chart.lane_notes.iter().flatten().filter(|note| note.kind == NoteKind::Mine).count() as u32;

    IrChartPayload {
        sha256: hash_to_hex(&chart.identity.file_sha256),
        md5: Some(hash_to_hex(&chart.identity.file_md5)),
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
        mode: chart.metadata.key_mode.as_str().to_string(),
        level: parse_play_level(&chart.metadata.play_level),
        total: Some(gauge_total),
        judge: chart.metadata.judge_rank,
        bpm: Some(IrChartBpm { min: Some(min_bpm), max: Some(max_bpm) }),
        notes: IrChartNotes {
            total: chart.total_notes,
            ln: ln_count,
            cn: cn_count,
            hcn: hcn_count,
            mine: mine_count,
        },
        features: IrChartFeatures {
            random: chart.metadata.has_bms_random,
            stop: chart
                .timing_events
                .iter()
                .any(|event| matches!(event.kind, TimingEventKind::Stop { .. })),
            ln: ln_profile.has_undefined_ln || ln_profile.has_defined_ln,
            cn: ln_profile.has_defined_cn,
            hcn: ln_profile.has_defined_hcn,
            mine: has_mine,
        },
        urls: chart_registry_urls(chart),
        headers: chart.metadata.bms_headers.clone(),
    }
}

fn chart_registry_urls(chart: &PlayableChart) -> Option<IrChartUrls> {
    let source = non_empty_string(&chart.metadata.source_url);
    let append = non_empty_string(&chart.metadata.append_url);
    if source.is_none() && append.is_none() { None } else { Some(IrChartUrls { source, append }) }
}

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() { None } else { Some(value.to_string()) }
}

fn chart_bpm_range(chart: &PlayableChart) -> (f64, f64) {
    let mut min_bpm = chart.metadata.initial_bpm;
    let mut max_bpm = chart.metadata.initial_bpm;
    for event in &chart.timing_events {
        if let TimingEventKind::BpmChange { bpm } = event.kind {
            min_bpm = min_bpm.min(bpm);
            max_bpm = max_bpm.max(bpm);
        }
    }
    (min_bpm, max_bpm)
}

fn parse_play_level(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { trimmed.parse().ok() }
}

fn effective_ln_mode_payload(mode: LongNoteMode) -> IrEffectiveLnMode {
    match mode {
        LongNoteMode::Ln => IrEffectiveLnMode::Ln,
        LongNoteMode::Cn => IrEffectiveLnMode::Cn,
        LongNoteMode::Hcn => IrEffectiveLnMode::Hcn,
    }
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

fn arrange_option_ir(arrange: ArrangeOption) -> &'static str {
    match arrange {
        ArrangeOption::Normal => "normal",
        ArrangeOption::Mirror => "mirror",
        ArrangeOption::Random => "random",
        ArrangeOption::RRandom => "r-random",
        ArrangeOption::SRandom => "s-random",
        ArrangeOption::Spiral => "spiral",
        ArrangeOption::HRandom => "h-random",
        ArrangeOption::AllScratch => "all-scr",
        ArrangeOption::RandomEx => "random-ex",
        ArrangeOption::SRandomEx => "s-random-ex",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::lane::KeyMode;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;

    use crate::select_options::ArrangeOption;

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
                device_type: InputDeviceKind::Controller,
                idempotency_key: "score_local_1".to_string(),
                arrange: ArrangeOption::Random,
                double_option: DoubleOptionScoreBucket::Battle,
                arrange_seed: Some(42),
                random_seed: Some(42),
                rule_mode: "Beatoraja".to_string(),
                replay_hash: Some("ab".repeat(32)),
            },
        );

        assert_eq!(payload.result.min_bp, 6);
        assert_eq!(payload.result.min_cb, 3);
        assert_eq!(payload.result.pass_notes, None);
        assert_eq!(payload.rule.ln_policy, LnScorePolicy::ForceLn);
        assert_eq!(payload.rule.rule_mode, "Beatoraja");
        assert_eq!(payload.rule.key_mode, "7K");
        assert_eq!(
            payload.play_options.get("device_type"),
            Some(&serde_json::Value::String("controller".to_string()))
        );
        assert_eq!(
            payload.play_options.get("option"),
            Some(&serde_json::Value::String("random".to_string()))
        );
        assert_eq!(
            payload.play_options.get("double_option"),
            Some(&serde_json::Value::String("battle".to_string()))
        );
        assert_eq!(payload.play_options.get("seed"), Some(&serde_json::json!(42)));
        assert_eq!(payload.play_options.get("random_seed"), Some(&serde_json::json!(42)));
        let replay = payload.replay.expect("replay payload");
        assert_eq!(replay.hash, "ab".repeat(32));
        assert_eq!(replay.format, "bmz-replay-v1");
        assert_eq!(replay.upload_intent, "later");
    }

    #[test]
    fn build_ir_chart_payload_includes_registry_metadata() {
        let chart = PlayableChart {
            identity: ChartIdentity { file_md5: [0xAB; 16], file_sha256: [0xCD; 32] },
            metadata: ChartMetadata {
                title: "Song".to_string(),
                play_level: "12".to_string(),
                judge_rank: Some(3),
                total: Some(400.0),
                initial_bpm: 180.0,
                has_bms_random: true,
                source_url: "http://example.com/bms".to_string(),
                append_url: "http://example.com/append".to_string(),
                bms_headers: BTreeMap::from([
                    ("TITLE".to_string(), "Song".to_string()),
                    ("URL".to_string(), "http://example.com/bms".to_string()),
                ]),
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
            total_notes: 100,
            end_time: bmz_core::time::TimeUs(0),
        };

        let payload = build_ir_chart_payload(&chart);

        assert_eq!(payload.mode, "7K");
        assert_eq!(payload.md5.as_deref(), Some("abababababababababababababababab"));
        assert_eq!(payload.level, Some(12));
        assert_eq!(payload.judge, Some(3));
        assert_eq!(payload.total, Some(400.0));
        assert!(payload.features.random);
        assert_eq!(payload.bpm.and_then(|bpm| bpm.min), Some(180.0));
        assert!(!payload.ln_profile.has_defined_ln);
        assert_eq!(
            payload.urls.as_ref().and_then(|urls| urls.source.as_deref()),
            Some("http://example.com/bms")
        );
        assert_eq!(
            payload.urls.as_ref().and_then(|urls| urls.append.as_deref()),
            Some("http://example.com/append")
        );
        assert_eq!(payload.headers.get("TITLE"), Some(&"Song".to_string()));
    }
}
