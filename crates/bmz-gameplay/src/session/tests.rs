use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, SoundAssetRef, SoundEvent};
use bmz_core::chart::ChartIdentity;
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::input::{InputDeviceKind, InputSource};
use bmz_core::judge::TimingSide;
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

use crate::input::backend::NullInputBackend;
use crate::input::binding::LaneBinding;
use crate::input::system::InputSystem;
use crate::input::translator::DefaultInputTranslator;
use crate::judge::model::{JudgeWindow, ScratchPressSuppression};

use super::*;
use crate::score::scored_note_count;

#[derive(Default)]
struct TestAudio {
    scheduled: Vec<ScheduledSound>,
}

impl AudioScheduler for TestAudio {
    fn schedule(&mut self, sound: ScheduledSound) {
        self.scheduled.push(sound);
    }
}

/// Key1 に HCN ロングノート (0s 〜 1s, キー音 SoundId(7)) を持つ譜面。
fn chart_with_hcn_long_note() -> PlayableChart {
    let mut chart = chart_with_keysound();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::LongStart,
        tick: ChartTick(0),
        time: TimeUs(0),
        sound: Some(SoundId(7)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::LongEnd,
        tick: ChartTick(192),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.long_notes.push(bmz_chart::model::LongNotePair {
        lane: Lane::Key1,
        style: bmz_chart::model::LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Hcn),
        start_note_id: NoteId(1),
        end_note_id: NoteId(2),
        start_tick: ChartTick(0),
        end_tick: ChartTick(192),
        start_time: TimeUs(0),
        end_time: TimeUs(1_000_000),
        sound: Some(SoundId(7)),
    });
    chart
}

fn human_press(time: TimeUs) -> InputEvent {
    InputEvent {
        lane: Lane::Key1,
        kind: InputKind::Press,
        time,
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    }
}

fn human_release(time: TimeUs) -> InputEvent {
    InputEvent {
        lane: Lane::Key1,
        kind: InputKind::Release,
        time,
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    }
}

fn chart_with_invisible_keysound() -> PlayableChart {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::Invisible,
        tick: ChartTick(96),
        time: TimeUs(500_000),
        sound: Some(SoundId(8)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.sounds.push(SoundAssetRef { id: SoundId(8), path: "hidden.wav".into(), slice: None });
    chart.end_time = TimeUs(500_000);
    chart
}

fn ln_chart_with_start_sound_and_end_sound(end_sound: Option<SoundId>) -> PlayableChart {
    let mut chart = chart_with_hcn_long_note();
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.long_notes[0].mode = Some(LongNoteMode::Ln);
    chart.lane_notes[Lane::Key1.index()][1].sound = end_sound;
    if let Some(sound_id) = end_sound {
        chart.sounds.push(SoundAssetRef {
            id: sound_id,
            path: format!("sound-{}.wav", sound_id.0).into(),
            slice: None,
        });
    }
    chart
}

fn chart_with_mine(time: TimeUs, damage: f64) -> PlayableChart {
    let mut chart = chart_with_keysound();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(7),
        lane: Lane::Key1,
        kind: NoteKind::Mine,
        tick: ChartTick(0),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: Some(damage),
    });
    chart.total_notes = 0;
    chart.end_time = time;
    chart
}

fn session_with_autoplay(chart: PlayableChart) -> GameSession {
    let chart = Arc::new(chart);
    let timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    GameSession {
        session_mode_index: 0,
        chart: Arc::clone(&chart),
        play_config_key_mode: chart.metadata.key_mode,
        primary_key_mode: chart.metadata.key_mode,
        scored_total_notes: scored_note_count(&chart),
        assist: Default::default(),
        timing_map,
        audio_clock: AudioClock::stopped(48_000),
        input_system: InputSystem {
            backend: Box::new(NullInputBackend),
            translator: Box::new(DefaultInputTranslator {
                binding: LaneBinding { entries: Vec::new() },
            }),
            bounce_filter: Default::default(),
        },
        judge: JudgeEngine::new(JudgeWindow::symmetric(
            16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
        )),
        base_judge_window: JudgeWindow::symmetric(
            16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
        ),
        base_judge_windows: JudgeWindows::uniform(JudgeWindow::symmetric(
            16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000,
        )),
        rule_mode: RuleMode::Beatoraja,
        score: ScoreState::default(),
        opponent_score: None,
        battle_opponent: None,
        course_combo_carry: 0,
        course_combo_carry_active: false,
        course_max_combo: 0,
        gauge: GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, chart.total_notes),
        opponent_gauge: None,
        replay_recorder: ReplayRecorder::default(),
        replay_player: None,
        replay_lane_projection: None,
        replay_lane_mask: None,
        display_only_lane_mask: [false; LANE_COUNT],
        autoplay: Some(AutoplayController::default()),
        recent_inputs: Vec::new(),
        lane_keyon_started_at: Default::default(),
        lane_keyoff_started_at: Default::default(),
        lane_scratch_direction: Default::default(),
        lane_scratch_angle_delta_ms: Default::default(),
        scratch_angle_last_render_at: None,
        lane_auto_release_at: Default::default(),
        recent_judgements: Vec::new(),
        recent_display_judgements: Vec::new(),
        pending_skin_events: Vec::new(),
        next_skin_event_sequence: 0,
        result_judgements: Default::default(),
        hit_error_ring: HitErrorRing::default(),
        gauge_increase_started_at: None,
        opponent_gauge_increase_started_at: None,
        gauge_max_started_at: None,
        opponent_gauge_max_started_at: None,
        full_combo_started_at: None,
        opponent_full_combo_started_at: None,
        bgm_scheduler: BgmScheduler::default(),
        auto_keysound_scheduler: AutoKeysoundScheduler::default(),
        offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
        input_offset_auto_adjust_enabled: false,
        input_offset_auto_adjust: None,
        audio_mix: PlayAudioMix {
            master_volume: 1.0,
            chart_normalization_gain: 1.0,
            normalize_chart_volume: true,
            key_volume: 1.0,
            bgm_volume: 1.0,
            auto_keysound: false,
            auto_keysound_fallback: false,
            auto_keysound_mine: true,
        },
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        constant_enabled: false,
        constant_fade_ms: 100,
        guide_se_enabled: false,
        note_retention: false,
        hsfix_base_bpm: 120.0,
        lift: 0.0,
        lane_cover: 0.0,
        lane_cover_visible: true,
        lane_cover_changing: false,
        lanecover_enabled: false,
        lift_enabled: true,
        hidden_enabled: false,
        hispeed_auto_adjust: false,
        hidden_cover: 0.0,
        skin_offsets: Vec::new(),
        bga_enabled: true,
        poor_bga_duration_us: 500_000,
        bga_stretch: 1,
        show_ln_tail_cap: false,
        lane_hcn_timer: [None; LANE_COUNT],
        lane_hcn_keysound_muted: [None; LANE_COUNT],
        pending_keysounds: Vec::new(),
        pending_keysound_volumes: Vec::new(),
        hsfix_index: 0,
        input_timestamp_anchor: None,
        pending_mine_hits: Vec::new(),
        state: PlayState::Ready,
        last_hcn_gauge_at: None,
    }
}

fn chart_with_keysound() -> PlayableChart {
    let note = NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(0),
        sound: Some(SoundId(7)),
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[Lane::Key1.index()].push(note);

    PlayableChart {
        identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: ChartMetadata::default(),
        lane_notes,
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
        sounds: vec![SoundAssetRef { id: SoundId(7), path: "sound.wav".into(), slice: None }],
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: TimeUs(0),
    }
}

fn chart_with_bgm() -> PlayableChart {
    PlayableChart {
        identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: ChartMetadata::default(),
        lane_notes: std::array::from_fn(|_| Vec::new()),
        long_notes: Vec::new(),
        bgm_events: vec![SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(3) }],
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
        sounds: vec![SoundAssetRef { id: SoundId(3), path: "bgm.wav".into(), slice: None }],
        bga_assets: Vec::new(),
        total_notes: 0,
        end_time: TimeUs(0),
    }
}

fn judgement_event(judge: Judge, delta_us: i64) -> JudgementEvent {
    JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge,
        side: if delta_us < 0 { TimingSide::Fast } else { TimingSide::Slow },
        delta: TimeUs(delta_us),
        time: TimeUs(0),
        affects_score: true,
    }
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
