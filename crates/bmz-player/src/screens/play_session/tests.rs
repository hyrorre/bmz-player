use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use bmz_audio::loader::LoadedSampleStatus;
use bmz_chart::hash::compute_chart_identity;
use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef, SoundSlice};
use bmz_core::clear::GaugeType;
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::input::InputKind;
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::input::backend::{
    BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
};
use bmz_gameplay::input::translator::InputTimingContext;
use bmz_gameplay::rule::RuleMode;
use rusqlite::Connection;

use super::*;
use crate::config::profile_config::{BaseHispeedConfig, FloatingPolicyConfig, HispeedConfigPreset};
use crate::storage::common::configure_connection;
use crate::storage::library_db::{ChartImportRecord, ChartNormalizationAnalysis, LibraryDatabase};
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

fn class_gauge_values(session: &GameSession) -> [f32; 6] {
    session
        .gauge
        .gauges
        .iter()
        .find(|g| g.definition.gauge_type == GaugeType::Class)
        .map(|g| g.definition.values)
        .expect("Class gauge present")
}

fn chart() -> PlayableChart {
    PlayableChart {
        identity: compute_chart_identity(b"session"),
        metadata: ChartMetadata {
            title: "session".to_string(),
            initial_bpm: 120.0,
            total: Some(160.0),
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
        sounds: Vec::<SoundAssetRef>::new(),
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: TimeUs(0),
    }
}

fn preloaded_play_session(chart: PlayableChart) -> PreloadedPlaySession {
    let source_ln_profile = ChartLnProfile::from_chart(&chart);
    let score_key =
        ScoreKey::new(chart.identity.file_sha256, crate::ln_policy::LnScorePolicy::AutoLn);
    let chart = Arc::new(chart);
    PreloadedPlaySession {
        render_snapshot_cache: crate::screens::play_snapshot::PlayRenderSnapshotCache::from_chart(
            &chart,
        ),
        chart,
        skin_attempt: Default::default(),
        source_ln_profile,
        chart_length_ms: 0,
        audio: AudioEngine::new(48_000),
        sample_report: Vec::new(),
        chart_normalization_gain: 1.0,
        applied_arrange: AppliedArrange::default(),
        score_key,
        assist_runtime: AssistRuntime::default(),
        score_save_disabled: false,
        opponent_chart: None,
    }
}

#[test]
fn play_target_display_name_survives_placeholder_and_session_preparation() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for (target, name, expected_name) in [
        (TargetOption::RankAaa, Some("ライバル_AAA"), "ライバル_AAA"),
        (TargetOption::RankAaa, Some("AAA"), "AAA"),
        (TargetOption::RankAaa, Some("NONE"), "NONE"),
        (TargetOption::RankAaa, Some("RANK_AAA"), "RANK_AAA"),
        (TargetOption::RankAaa, None, "RANK AAA"),
        (TargetOption::IrTop, None, ""),
        (TargetOption::None, None, ""),
    ] {
        let options = PlaySessionOptions {
            target,
            resolved_target: name
                .map(|name| ResolvedTarget { name: name.to_string(), ex_score: 123 }),
            ..Default::default()
        };
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();
        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
        assert_eq!(snapshot.target, target.as_string());
        assert_eq!(snapshot.resolved_target_name.as_deref(), name);
        if name.is_some() {
            assert_eq!(snapshot.target_ex_score, Some(123));
        }

        let prepared = build_prepared_play_session_from_preloaded(
            preloaded_play_session(chart()),
            &profile,
            options,
            Box::new(BufferedInputBackend::default()),
        );
        assert_eq!(prepared.target_option, target);
        assert_eq!(prepared.target_name, expected_name);
        assert_eq!(prepared.resolved_target.as_ref().map(|target| target.name.as_str()), name);
    }
}

#[test]
fn selected_rival_without_score_survives_session_preparation() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for target in [TargetOption::None, TargetOption::RankAaa, TargetOption::IrTop] {
        let options = PlaySessionOptions {
            target,
            rival_name: Some("未プレイ_ライバル".to_string()),
            ..Default::default()
        };
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();
        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
        assert_eq!(snapshot.resolved_target_name.as_deref(), Some("未プレイ_ライバル"));
        let prepared = build_prepared_play_session_from_preloaded(
            preloaded_play_session(chart()),
            &profile,
            options,
            Box::new(BufferedInputBackend::default()),
        );
        assert_eq!(prepared.target_name, "未プレイ_ライバル");
        assert_eq!(prepared.target_option, target);
        assert!(prepared.resolved_target.is_none());
    }
}

#[test]
fn cloned_preload_reuses_loaded_pcm_without_playback_state() {
    let mut preloaded = preloaded_play_session(chart());
    preloaded.audio.insert_sample(
        SoundId(7),
        bmz_audio::sample::DecodedSample {
            channels: 1,
            sample_rate: 48_000,
            frames: vec![0.25, 0.5],
        },
    );
    preloaded.audio.play_now(SoundId(7), 1.0, false);

    let reused = preloaded.clone_loaded_resources();

    assert!(Arc::ptr_eq(&preloaded.chart, &reused.chart));
    assert_eq!(reused.audio.samples.source_count(), 1);
    assert_eq!(reused.audio.samples.region_count(), 1);
    assert!(reused.audio.is_idle());
}

#[test]
fn sound_preload_counts_distinct_sources_and_regions() {
    let mut chart = chart();
    chart.sounds = vec![
        SoundAssetRef {
            id: SoundId(0),
            path: "long-source.wav".into(),
            slice: Some(SoundSlice { start_us: 0, duration_us: Some(500_000) }),
        },
        SoundAssetRef {
            id: SoundId(1),
            path: "long-source.wav".into(),
            slice: Some(SoundSlice { start_us: 500_000, duration_us: Some(500_000) }),
        },
        SoundAssetRef { id: SoundId(2), path: "keysound.wav".into(), slice: None },
    ];

    assert_eq!(
        sound_preload_counts(&chart),
        SoundPreloadCounts { source_count: 2, region_count: 3 }
    );
}

fn note(id: u32, lane: Lane, time_us: i64) -> NoteEvent {
    use bmz_core::time::ChartTick;

    NoteEvent {
        id: NoteId(id),
        lane,
        kind: NoteKind::Tap,
        tick: ChartTick((time_us / 1_000) as u64),
        time: TimeUs(time_us),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    }
}

fn chart_with_two_notes_same_lane() -> PlayableChart {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
    chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_020_000));
    chart
}

fn lanes_for_notes(chart: &PlayableChart) -> Vec<(NoteId, Lane)> {
    let mut lanes: Vec<_> =
        chart.lane_notes.iter().flatten().map(|note| (note.id, note.lane)).collect();
    lanes.sort_by_key(|(id, _)| *id);
    lanes
}

fn write_temp_bms(text: &str) -> std::path::PathBuf {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path =
        std::env::temp_dir().join(format!("bmz-play-session-{}-{stamp}.bms", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path
}

fn write_temp_bms_with_wav(text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir();
    let bms_path = dir.join(format!("bmz-prepared-session-{}-{stamp}.bms", std::process::id()));
    let wav_name = format!("bmz-prepared-session-{}-{stamp}.wav", std::process::id());
    let wav_path = dir.join(&wav_name);
    std::fs::write(&bms_path, text.replace("test.wav", &wav_name)).unwrap();
    std::fs::write(&wav_path, [wav_header(1, 1, 48_000, 16, 2).as_slice(), &[0x00, 0x40]].concat())
        .unwrap();
    (bms_path, wav_path)
}

fn write_temp_bms_with_two_wavs(
    text: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir();
    let prefix = format!("bmz-retry-audio-{}-{stamp}", std::process::id());
    let bms_path = dir.join(format!("{prefix}.bms"));
    let bgm_path = dir.join(format!("{prefix}-bgm.wav"));
    let key_path = dir.join(format!("{prefix}-key.wav"));
    let bms = text
        .replace("bgm.wav", bgm_path.file_name().unwrap().to_str().unwrap())
        .replace("key.wav", key_path.file_name().unwrap().to_str().unwrap());
    std::fs::write(&bms_path, bms).unwrap();
    std::fs::write(
        &bgm_path,
        [wav_header(1, 1, 48_000, 16, 2).as_slice(), &16_384_i16.to_le_bytes()].concat(),
    )
    .unwrap();
    std::fs::write(
        &key_path,
        [wav_header(1, 1, 48_000, 16, 2).as_slice(), &(-16_384_i16).to_le_bytes()].concat(),
    )
    .unwrap();
    (bms_path, bgm_path, key_path)
}

fn wav_header(format: u16, channels: u16, sample_rate: u32, bits: u16, data_len: u32) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * bits as u32 / 8;
    let block_align = channels * bits / 8;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/cases_05.rs"]
mod cases_05;
#[path = "tests/cases_06.rs"]
mod cases_06;
#[path = "tests/hsfix.rs"]
mod hsfix;
