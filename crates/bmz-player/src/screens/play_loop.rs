use anyhow::{Result, anyhow};
use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::queue::AudioScheduler;
use bmz_core::time::TimeUs;
use bmz_gameplay::session::{
    FrameOutput, GameSession, PlayState, SessionFrame, advance_session_frame,
    apply_auto_key_release, compute_frame_times, update_recent_inputs, update_recent_judgements,
};
use bmz_render::snapshot::RenderSnapshot;

use crate::audio::RunningPlaySession;
use crate::config::profile_config::{IrConfig, ReplayConfig};
use crate::paths::ProfilePaths;
use crate::screens::play_finish::{
    FinishSessionResultOnceRequest, FinishedPlaySession, finish_session_result,
    finish_session_result_once,
};
use crate::screens::play_session::AppliedArrange;
use crate::screens::play_snapshot::{
    BgaFrameCatalog, build_render_snapshot_with_target_and_bga_frames,
};
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PlayAdvanceOutcome {
    Playing(FrameOutput<RenderSnapshot>),
    Finished { frame: FrameOutput<RenderSnapshot>, finished: FinishedPlaySession },
}

impl PlayAdvanceOutcome {
    pub fn frame(&self) -> &FrameOutput<RenderSnapshot> {
        match self {
            Self::Playing(frame) | Self::Finished { frame, .. } => frame,
        }
    }

    pub fn finished(&self) -> Option<&FinishedPlaySession> {
        match self {
            Self::Playing(_) => None,
            Self::Finished { finished, .. } => Some(finished),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished().is_some()
    }
}

pub fn advance_play_screen(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    best_ex_score: Option<u32>,
) -> FrameOutput<RenderSnapshot> {
    advance_play_screen_with_bga_frames(
        session,
        audio,
        best_ex_score,
        None,
        None,
        &BgaFrameCatalog::new(),
    )
}

pub fn advance_play_screen_with_bga_frames(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> FrameOutput<RenderSnapshot> {
    let frame = advance_session_frame(session, audio);
    let render_snapshot = build_render_snapshot_with_target_and_bga_frames(
        session,
        frame.times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
    );
    FrameOutput {
        render_snapshot,
        judgements: frame.judgements,
        mine_hits: frame.mine_hits,
        state: frame.state,
    }
}

pub fn advance_play_screen_until_result(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    played_at: i64,
    applied_arrange: &AppliedArrange,
) -> Result<PlayAdvanceOutcome> {
    let frame = advance_play_screen(session, audio, None);
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let mut finished = finish_session_result(
            score_db,
            profile_paths,
            replay_config,
            ir_config,
            session,
            played_at,
            applied_arrange,
            None,
            crate::storage::score_db::ScoreKey::new(
                session.chart.identity.file_sha256,
                crate::ln_policy::score_ln_policy(
                    crate::ln_policy::LnPolicySetting::AutoLn,
                    crate::ln_policy::ChartLnProfile::from_chart(&session.chart),
                ),
            ),
            false,
        )?;
        let mut result_graph = crate::screens::result_model::ResultGraphCollector::default();
        result_graph.record_frame(&frame);
        finished.summary.graph = result_graph.snapshot_for_session(session);
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

pub fn advance_play_screen_with_shared_audio(
    session: &mut GameSession,
    audio: &SharedAudioEngine,
    best_ex_score: Option<u32>,
) -> Result<FrameOutput<RenderSnapshot>> {
    let mut audio = audio.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
    Ok(advance_play_screen(session, &mut *audio, best_ex_score))
}

/// `SessionFrame`(audio スケジューリング結果)から、ロック不要な render
/// snapshot を構築して `FrameOutput` を組み立てる。重い処理はここに集約し、
/// audio エンジンロックの外で実行する。
fn frame_output_from_session_frame(
    session: &GameSession,
    frame: SessionFrame,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> FrameOutput<RenderSnapshot> {
    let render_snapshot = build_render_snapshot_with_target_and_bga_frames(
        session,
        frame.times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
    );
    FrameOutput {
        render_snapshot,
        judgements: frame.judgements,
        mine_hits: frame.mine_hits,
        state: frame.state,
    }
}

pub fn advance_running_play_session(
    running: &mut RunningPlaySession,
) -> Result<FrameOutput<RenderSnapshot>> {
    // audio エンジンロックは音のスケジューリング(advance_session_frame)中だけ
    // 保持する。重い render snapshot 構築をロック外に出すことで、audio callback の
    // try_lock スキップ(= 全バックエンドで音切れ)を防ぐ。
    let frame = {
        let mut audio =
            running.audio.engine.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
        advance_session_frame(&mut running.session, &mut *audio)
    };
    let mut output = frame_output_from_session_frame(
        &running.session,
        frame,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
    );
    apply_play_arrange_to_snapshot(&mut output.render_snapshot, running.applied_arrange.arrange);
    Ok(output)
}

pub fn advance_running_play_session_until_result(
    running: &mut RunningPlaySession,
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    played_at: i64,
) -> Result<PlayAdvanceOutcome> {
    let session_frame = {
        let mut audio =
            running.audio.engine.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
        advance_session_frame(&mut running.session, &mut *audio)
    };
    let mut frame = frame_output_from_session_frame(
        &running.session,
        session_frame,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
    );
    apply_play_arrange_to_snapshot(&mut frame.render_snapshot, running.applied_arrange.arrange);
    running.result_graph.record_frame(&frame);
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let mut finished = finish_session_result_once(
            &mut running.finished,
            score_db,
            FinishSessionResultOnceRequest {
                profile_paths,
                replay_config,
                ir_config,
                session: &running.session,
                played_at,
                applied_arrange: &running.applied_arrange,
                target_ex_score: running.target_ex_score,
                score_key: running.score_key,
                practice_mode: running.practice_mode,
            },
        )?;
        finished.summary.graph = running.result_graph.snapshot_for_session(&running.session);
        running.finished = Some(finished.clone());
        // ここでは音声を止めない。スケジュール済みの BGM/キー音は
        // オーディオ出力スレッド側で曲の最後まで鳴り切る。出力の解放は
        // リザルト画面側 (advance_draining_audio) がドレイン完了後に行う。
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

/// `play_ending` 中に skin 側へ渡す壁時計ベースの timer 値。
#[derive(Debug, Clone, Copy)]
pub struct PlayEndingSkinTimers {
    pub play_elapsed_time: TimeUs,
    pub ready_elapsed_time: Option<TimeUs>,
    pub backbmp_background: bool,
    pub failed_elapsed_ms: Option<i32>,
    pub music_end_elapsed_ms: Option<i32>,
    pub fadeout_elapsed_ms: Option<i32>,
}

/// 終了演出中に gameplay を止めたまま、オーディオクロックに追従して描画 snapshot を更新する。
pub fn refresh_play_ending_snapshot(
    running: &mut RunningPlaySession,
    timers: PlayEndingSkinTimers,
) -> RenderSnapshot {
    let mut snapshot = refresh_play_ending_snapshot_with_session(
        &mut running.session,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
        timers,
    );
    apply_play_arrange_to_snapshot(&mut snapshot, running.applied_arrange.arrange);
    snapshot
}

fn apply_play_arrange_to_snapshot(
    snapshot: &mut RenderSnapshot,
    arrange: crate::select_options::ArrangeOption,
) {
    snapshot.arrange = arrange.as_str().to_string();
}

pub fn refresh_play_ending_snapshot_with_session(
    session: &mut GameSession,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    timers: PlayEndingSkinTimers,
) -> RenderSnapshot {
    let times = compute_frame_times(session);
    apply_auto_key_release(session, times.audio_now);
    update_recent_judgements(session, &[], times.render_now);
    update_recent_inputs(session, &[], times.render_now);

    let mut snapshot = build_render_snapshot_with_target_and_bga_frames(
        session,
        times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
    );
    snapshot.play_elapsed_time = timers.play_elapsed_time;
    snapshot.ready_elapsed_time = timers.ready_elapsed_time;
    snapshot.backbmp_background = timers.backbmp_background;
    snapshot.failed_elapsed_ms = timers.failed_elapsed_ms;
    snapshot.music_end_elapsed_ms = timers.music_end_elapsed_ms;
    snapshot.fadeout_elapsed_ms = timers.fadeout_elapsed_ms;
    snapshot
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use bmz_audio::backend::cpal::SharedAudioEngine;
    use bmz_audio::clock::AudioClock;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::queue::{AudioScheduler, ScheduledSound};
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::Judge;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use bmz_gameplay::judge::model::JudgementEvent;

    use crate::config::profile_config::ProfileConfig;
    use crate::config::profile_config::ReplayConfig;
    use crate::screens::play_session::{PlaySessionOptions, build_game_session};
    use crate::select_options::ArrangeOption;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};
    use crate::storage::score_db::ScoreDatabase;

    use super::*;

    #[derive(Default)]
    struct TestAudio {
        scheduled: Vec<ScheduledSound>,
    }

    impl AudioScheduler for TestAudio {
        fn schedule(&mut self, sound: ScheduledSound) {
            self.scheduled.push(sound);
        }
    }

    #[test]
    fn advance_play_screen_returns_snapshot() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let mut audio = TestAudio::default();

        let frame = advance_play_screen(&mut session, &mut audio, None);

        assert_eq!(frame.render_snapshot.time, TimeUs(0));
        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    }

    #[test]
    fn apply_play_arrange_to_snapshot_sets_skin_label() {
        let mut snapshot = RenderSnapshot::default();

        apply_play_arrange_to_snapshot(&mut snapshot, ArrangeOption::Mirror);

        assert_eq!(snapshot.arrange, "MIRROR");
    }

    #[test]
    fn advance_play_screen_with_shared_audio_locks_engine() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));

        let frame = advance_play_screen_with_shared_audio(&mut session, &audio, None).unwrap();

        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    }

    #[test]
    fn advance_play_screen_until_result_returns_finished_outcome() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(finished_chart()), &profile, PlaySessionOptions::default());
        let mut audio = TestAudio::default();
        let root = make_temp_dir("advance-finished");
        let paths = crate::paths::ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: false,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };

        let outcome = advance_play_screen_until_result(
            &mut session,
            &mut audio,
            &mut score_db,
            &paths,
            &replay_config,
            &crate::config::profile_config::IrConfig::default(),
            1_700_000_200,
            &AppliedArrange::default(),
        )
        .unwrap();

        assert!(matches!(outcome, PlayAdvanceOutcome::Finished { .. }));
        assert!(outcome.is_finished());
        assert!(outcome.finished().is_some());
        assert_eq!(outcome.frame().state, PlayState::Finished);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refresh_play_ending_snapshot_advances_note_scroll_with_audio_clock() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;
        session.audio_clock = test_running_audio_clock(0);

        let timers = PlayEndingSkinTimers {
            play_elapsed_time: TimeUs(0),
            ready_elapsed_time: None,
            backbmp_background: false,
            failed_elapsed_ms: None,
            music_end_elapsed_ms: None,
            fadeout_elapsed_ms: None,
        };
        let early = refresh_play_ending_snapshot_with_session(
            &mut session,
            None,
            None,
            None,
            &BgaFrameCatalog::new(),
            timers,
        );
        assert_eq!(early.visible_notes[Lane::Key1.index()][0].y, 0.5);

        advance_test_audio_clock(&mut session, 750_000);
        let later = refresh_play_ending_snapshot_with_session(
            &mut session,
            None,
            None,
            None,
            &BgaFrameCatalog::new(),
            timers,
        );
        assert_eq!(later.time, TimeUs(750_000));
        assert_eq!(later.visible_notes[Lane::Key1.index()][0].y, 0.125);
    }

    #[test]
    fn refresh_play_ending_snapshot_expires_old_judgements() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        session.recent_judgements.push(JudgementEvent {
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: bmz_core::judge::TimingSide::Fast,
            delta: TimeUs(0),
            time: TimeUs(0),
            note_id: Some(NoteId(1)),
        });
        session.audio_clock = test_running_audio_clock(0);

        let timers = PlayEndingSkinTimers {
            play_elapsed_time: TimeUs(0),
            ready_elapsed_time: None,
            backbmp_background: false,
            failed_elapsed_ms: None,
            music_end_elapsed_ms: None,
            fadeout_elapsed_ms: None,
        };
        let visible = refresh_play_ending_snapshot_with_session(
            &mut session,
            None,
            None,
            None,
            &BgaFrameCatalog::new(),
            timers,
        );
        assert_eq!(visible.recent_judgements.len(), 1);

        advance_test_audio_clock(&mut session, 900_000);
        let expired = refresh_play_ending_snapshot_with_session(
            &mut session,
            None,
            None,
            None,
            &BgaFrameCatalog::new(),
            timers,
        );
        assert!(expired.recent_judgements.is_empty());
    }

    fn test_running_audio_clock(elapsed_us: i64) -> AudioClock {
        let sample_rate = 48_000;
        let current_frame = Arc::new(AtomicU64::new(us_to_frames(elapsed_us, sample_rate)));
        AudioClock::with_position(sample_rate, 0, 0, current_frame, true)
    }

    fn advance_test_audio_clock(session: &mut GameSession, elapsed_us: i64) {
        let sample_rate = session.audio_clock.sample_rate;
        session
            .audio_clock
            .current_frame
            .store(us_to_frames(elapsed_us, sample_rate), Ordering::Relaxed);
    }

    fn us_to_frames(elapsed_us: i64, sample_rate: u32) -> u64 {
        ((elapsed_us.max(0) as u128 * sample_rate as u128) / 1_000_000u128) as u64
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: compute_chart_identity(b"play-loop"),
            metadata: ChartMetadata {
                title: "play-loop".to_string(),
                initial_bpm: 120.0,
                total: Some(160.0),
                ..Default::default()
            },
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
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_000_000),
        }
    }

    fn finished_chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"finished-play-loop"),
            metadata: ChartMetadata {
                title: "finished-play-loop".to_string(),
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
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(-1_000_000),
        }
    }

    fn make_temp_dir(label: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-player-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
