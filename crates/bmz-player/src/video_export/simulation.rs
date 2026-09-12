use super::*;
use crate::{
    screens::play_snapshot::*,
    system_sound::{SoundSetSelection, SoundType},
};
use bmz_audio::{
    clock::AudioClock,
    queue::{ScheduledSound, ScheduledSoundQueue},
};
use bmz_core::ids::SoundId;
use bmz_gameplay::session::{PlayState, advance_session_frame};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct SceneTiming {
    pub ready_us: i64,
    pub chart_zero_us: i64,
    pub finishmargin_us: i64,
    pub close_us: i64,
}
impl SceneTiming {
    pub fn from_document(d: &bmz_render::skin::SkinDocument) -> Self {
        let ready_us = i64::from(d.loadstart.max(0)) * 1000 + i64::from(d.loadend.max(0)) * 1000;
        Self {
            ready_us,
            chart_zero_us: ready_us + i64::from(d.playstart.max(0)) * 1000,
            finishmargin_us: i64::from(d.finishmargin.max(0)) * 1000,
            close_us: i64::from(d.close.max(0)) * 1000,
        }
    }
}

pub struct Simulation {
    pub prepared: prepare::Prepared,
    timing: SceneTiming,
    pub cursor: u64,
    processed: bool,
    queue: ScheduledSoundQueue,
    pub exit_us: Option<i64>,
    finish_us: Option<i64>,
    fadeout_us: Option<i64>,
    fadeout_duration_us: i64,
    fc_duration_us: i64,
    system_handle: bmz_audio::command::AudioEngineHandle,
    system_processor: bmz_audio::command::CommandedAudioEngine,
    skin_audio: crate::skin_audio::SkinAudioRuntime,
    sounds: HashMap<SoundType, SoundId>,
    ready_played: bool,
    fadeout_triggered: bool,
    skin_events: Vec<bmz_gameplay::session::SkinRuntimeEvent>,
}

impl Simulation {
    pub fn new(
        mut prepared: prepare::Prepared,
        timing: SceneTiming,
        fadeout_ms: i32,
        fc_ms: i32,
        system_handle: bmz_audio::command::AudioEngineHandle,
        skin_audio: crate::skin_audio::SkinAudioRuntime,
        paths: &AppPaths,
    ) -> Result<Self> {
        prepared.play.session.audio_clock = AudioClock::with_position(
            48_000,
            0,
            -timing.chart_zero_us,
            Arc::new(AtomicU64::new(0)),
            true,
        );
        let config = &prepared.profile.system_sound;
        let selection = SoundSetSelection {
            bgm_dir: paths.resolve_optional_path_ref(&config.bgm_dir)?.and_then(|p| {
                crate::system_sound::scan_sound_sets(&p, "select.wav").into_iter().next()
            }),
            se_dir: paths.resolve_optional_path_ref(&config.se_dir)?.and_then(|p| {
                crate::system_sound::scan_sound_sets(&p, "clear.wav").into_iter().next()
            }),
            default_dir: paths.resolve_optional_path_ref(&config.default_sound_dir)?,
        };
        let mut sounds = HashMap::new();
        use bmz_audio::loader::SampleLoader;
        let mut loader = bmz_audio::ffmpeg_loader::FfmpegSampleLoader::default();
        for (index, kind) in SoundType::ALL.into_iter().enumerate() {
            if let Some(path) = selection.resolve(kind) {
                let sample = loader.load(&path)?;
                let id = SoundId(100_000 + index as u32);
                ensure!(system_handle.insert_sample(id, sample), "system sound queue full");
                sounds.insert(kind, id);
            }
        }
        let result = Self {
            prepared,
            timing,
            cursor: 0,
            processed: false,
            queue: ScheduledSoundQueue::new(),
            exit_us: None,
            finish_us: None,
            fadeout_us: None,
            fadeout_duration_us: i64::from(fadeout_ms) * 1000,
            fc_duration_us: i64::from(fc_ms) * 1000,
            system_processor: system_handle.processor(),
            system_handle,
            skin_audio,
            sounds,
            ready_played: false,
            fadeout_triggered: false,
            skin_events: Vec::new(),
        };
        let volume = result.system_volume();
        result.skin_audio.start_scene(result.system_bgm_volume(), volume);
        Ok(result)
    }
    fn system_volume(&self) -> f32 {
        let mix = &self.prepared.profile.audio_mix;
        crate::config::play::volume_unit_to_f32(mix.master_volume)
            * crate::config::play::volume_unit_to_f32(mix.system_se_volume)
    }
    fn system_bgm_volume(&self) -> f32 {
        let mix = &self.prepared.profile.audio_mix;
        crate::config::play::volume_unit_to_f32(mix.master_volume)
            * crate::config::play::volume_unit_to_f32(mix.system_bgm_volume)
    }
    fn sound(&self, kind: SoundType) -> Result<()> {
        if let Some(id) = self.sounds.get(&kind) {
            ensure!(
                self.system_handle.schedule_sound(ScheduledSound::one_shot(
                    self.cursor,
                    *id,
                    self.system_volume(),
                    0.0
                )),
                "system sound queue full"
            );
        }
        Ok(())
    }
    fn process(&mut self) -> Result<()> {
        if self.processed {
            return Ok(());
        }
        self.processed = true;
        let scene_us = (u128::from(self.cursor) * 1_000_000 / 48_000) as i64;
        if scene_us >= self.timing.ready_us && !self.ready_played {
            self.sound(SoundType::PlayReady)?;
            self.ready_played = true;
            self.skin_audio.trigger_timer(40, self.system_bgm_volume(), self.system_volume());
        }
        let session = &mut self.prepared.play.session;
        session.audio_clock.current_frame.store(self.cursor, Ordering::Relaxed);
        if matches!(session.state, PlayState::Finished | PlayState::Failed) {
            let now = session.audio_clock.now();
            bmz_gameplay::session::apply_auto_key_release(session, now);
            bmz_gameplay::session::update_recent_inputs(session, &[], now);
            bmz_gameplay::session::update_recent_judgements(session, &[], now);
        }
        // The live scratch accumulator consumes whole milliseconds. Preserve its
        // fractional interval when the offline owner ticks at audio-sample rate.
        let scratch_anchor = session.scratch_angle_last_render_at;
        let frame = advance_session_frame(session, &mut self.queue);
        if let Some(anchor) = scratch_anchor
            && session.audio_clock.now().0 - anchor.0 < 1000
        {
            session.scratch_angle_last_render_at = Some(anchor);
        }
        self.prepared.play.audio.schedule_all(self.queue.drain_all());
        for (id, volume) in frame.keysound_volumes {
            self.prepared.play.audio.set_sound_volume(id, volume);
        }
        self.skin_events.extend(frame.skin_events);
        if !frame.mine_hits.is_empty() {
            self.sound(SoundType::Landmine)?;
        }
        if self.prepared.profile.play.guide_se {
            for judge in &frame.judgements {
                use bmz_core::judge::Judge;
                self.sound(match judge.judge {
                    Judge::PGreat => SoundType::GuideSePGreat,
                    Judge::Great => SoundType::GuideSeGreat,
                    Judge::Good => SoundType::GuideSeGood,
                    Judge::Bad => SoundType::GuideSeBad,
                    Judge::Poor => SoundType::GuideSePoor,
                    Judge::EmptyPoor => SoundType::GuideSeMiss,
                })?;
            }
        }
        if self.finish_us.is_none()
            && matches!(frame.state, PlayState::Finished | PlayState::Failed)
        {
            self.finish_us = Some(scene_us);
            if frame.state == PlayState::Failed {
                self.exit_us = Some(scene_us + self.timing.close_us);
                self.sound(SoundType::PlayStop)?;
                self.skin_audio.trigger_timer(3, self.system_bgm_volume(), self.system_volume());
            } else {
                let snapshot = self.snapshot(scene_us, &BgaFrameCatalog::new());
                let remaining = snapshot
                    .full_combo_elapsed_ms
                    .map_or(0, |elapsed| (self.fc_duration_us - i64::from(elapsed) * 1000).max(0));
                self.fadeout_us = Some(scene_us + self.timing.finishmargin_us.max(remaining));
                self.exit_us = self.fadeout_us.map(|start| start + self.fadeout_duration_us);
            }
        }
        if !self.fadeout_triggered && self.fadeout_us.is_some_and(|start| scene_us >= start) {
            self.skin_audio.trigger_timer(2, self.system_bgm_volume(), self.system_volume());
            self.fadeout_triggered = true;
        }
        Ok(())
    }
    pub fn advance_to(
        &mut self,
        target: u64,
        mut write: impl FnMut(&[f32]) -> Result<()>,
    ) -> Result<()> {
        // A sample-clock simulation keeps judgement/HCN/volume changes independent
        // of the requested video FPS. No audio callback or wall clock is consulted.
        let mut block = Vec::with_capacity(8192);
        while self.cursor < target {
            self.process()?;
            let mut pcm = [0.0; 2];
            let mut system = [0.0; 2];
            self.prepared.play.audio.render_stereo(self.cursor, &mut pcm);
            ensure!(
                self.system_processor.render_stereo(self.cursor, &mut system),
                "offline audio command processing failed"
            );
            let ended = self
                .exit_us
                .is_some_and(|end| (u128::from(self.cursor) * 1_000_000 / 48_000) as i64 >= end);
            block.extend(if ended { [0.0, 0.0] } else { [pcm[0] + system[0], pcm[1] + system[1]] });
            self.cursor += 1;
            self.processed = false;
            if block.len() >= 8192 {
                write(&block)?;
                block.clear();
            }
        }
        write(&block)?;
        self.process()
    }
    pub fn acknowledge_frame(&mut self) {
        self.skin_events.clear();
    }
    pub fn snapshot(
        &self,
        scene_us: i64,
        catalog: &BgaFrameCatalog,
    ) -> bmz_render::snapshot::RenderSnapshot {
        let play = &self.prepared.play;
        let session = &play.session;
        let chart_us = (scene_us - self.timing.chart_zero_us)
            .max(-self.timing.chart_zero_us + self.timing.ready_us);
        let target = play.target_option.target_ex_score_with_best(
            bmz_gameplay::score::scored_note_count(&session.chart),
            self.prepared.best,
        );
        let mut snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
            session,
            TimeUs(chart_us),
            &session.recent_judgements,
            self.prepared.best,
            self.prepared.ghost.as_deref(),
            target,
            catalog,
            &play.render_snapshot_cache,
        );
        snapshot.play_elapsed_time = TimeUs(scene_us);
        refresh_play_skin_visuals(&mut snapshot, session);
        snapshot.operating_time_ms = (scene_us / 1000).min(i64::from(i32::MAX)) as i32;
        snapshot.ready_elapsed_time =
            (scene_us >= self.timing.ready_us).then_some(TimeUs(scene_us - self.timing.ready_us));
        snapshot.resources_loaded = true;
        snapshot.resource_load_progress = 1.0;
        snapshot.player_name = self.prepared.profile.display_name.clone();
        snapshot.target = play.target_name.clone();
        snapshot.skin_attempt = play.skin_attempt;
        snapshot.skin_events = self.skin_events.clone();
        snapshot.fadeout_elapsed_ms = self
            .fadeout_us
            .filter(|start| scene_us >= *start)
            .map(|start| ((scene_us - start) / 1000) as i32);
        snapshot.failed_elapsed_ms = self
            .finish_us
            .filter(|_| session.state == PlayState::Failed)
            .map(|start| ((scene_us - start) / 1000) as i32);
        snapshot.music_end_elapsed_ms = self
            .finish_us
            .filter(|_| session.state != PlayState::Failed)
            .map(|start| ((scene_us - start) / 1000) as i32);
        apply_fast_slow_display_filter(
            &mut snapshot,
            self.prepared.profile.judge.fast_slow_display_threshold_ms,
            self.prepared.profile.judge.fast_slow_display_scope,
        );
        snapshot
    }
    pub fn estimated_exit_us(&self) -> i64 {
        self.exit_us.unwrap_or(
            self.timing.chart_zero_us
                + self.prepared.play.session.chart.end_time.0
                + bmz_gameplay::session::SESSION_END_MARGIN_US
                + self.timing.finishmargin_us
                + self.fadeout_duration_us,
        )
    }
}
