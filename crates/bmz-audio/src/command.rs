use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use bmz_core::ids::SoundId;

use crate::engine::AudioEngine;
use crate::queue::{AudioScheduler, ScheduledSound};
use crate::sample::{DecodedSample, SampleBank};

pub const DEFAULT_AUDIO_COMMAND_QUEUE_CAPACITY: usize = 8_192;

/// コマンド drop 警告の最小間隔。キューが詰まりっぱなしのときに
/// フレーム毎の warn でログを溢れさせないための rate limit。
const DROP_WARN_INTERVAL_MS: u64 = 1_000;

#[derive(Debug)]
pub enum AudioEngineCommand {
    InsertSample {
        id: SoundId,
        sample: DecodedSample,
    },
    ReserveSampleSlot {
        id: SoundId,
    },
    InsertPreparedSample {
        id: SoundId,
        sample: DecodedSample,
    },
    Schedule(ScheduledSound),
    ScheduleAll(Vec<ScheduledSound>),
    ClearPlayback,
    SetPlaybackPaused {
        paused: bool,
    },
    StopSound {
        id: SoundId,
    },
    StopSoundWithFadeOut {
        id: SoundId,
        fade_out_frames: u32,
    },
    SetMasterGain {
        gain: f32,
    },
    ApplyPlaybackRateChange {
        change: crate::clock::PlaybackRateChange,
    },
    SetSoundVolume {
        id: SoundId,
        volume: f32,
    },
    PlayNow {
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
    },
    PlayNowWithVoiceLimit {
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        max_voices: usize,
    },
    PlayNowWithFadeIn {
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
    },
    PlayNowWithFadeInAndFadeOut {
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
        fade_out_frames: u32,
    },
}

impl AudioEngineCommand {
    pub fn apply(self, engine: &mut AudioEngine, output_frame: u64) {
        match self {
            Self::InsertSample { id, sample } => engine.insert_sample(id, sample),
            Self::ReserveSampleSlot { id } => engine.reserve_sample_slot(id),
            Self::InsertPreparedSample { id, sample } => engine.insert_prepared_sample(id, sample),
            Self::Schedule(sound) => engine.schedule(sound),
            Self::ScheduleAll(sounds) => engine.schedule_all(sounds),
            Self::ClearPlayback => engine.clear_playback(),
            Self::SetPlaybackPaused { paused } => {
                engine.set_playback_paused(paused, output_frame);
            }
            Self::StopSound { id } => engine.stop_sound(id),
            Self::StopSoundWithFadeOut { id, fade_out_frames } => {
                engine.stop_sound_with_fade_out(id, fade_out_frames);
            }
            Self::SetMasterGain { gain } => engine.set_master_gain(gain),
            Self::ApplyPlaybackRateChange { change } => {
                engine.apply_playback_rate_change(change);
            }
            Self::SetSoundVolume { id, volume } => engine.set_sound_volume(id, volume),
            Self::PlayNow { sound_id, volume, loop_playback } => {
                engine.play_now(sound_id, volume, loop_playback);
            }
            Self::PlayNowWithVoiceLimit { sound_id, volume, loop_playback, max_voices } => {
                engine.play_now_with_voice_limit(sound_id, volume, loop_playback, max_voices);
            }
            Self::PlayNowWithFadeIn { sound_id, volume, loop_playback, fade_in_frames } => {
                engine.play_now_with_fade_in(sound_id, volume, loop_playback, fade_in_frames);
            }
            Self::PlayNowWithFadeInAndFadeOut {
                sound_id,
                volume,
                loop_playback,
                fade_in_frames,
                fade_out_frames,
            } => {
                engine.play_now_with_fade_in_and_fade_out(
                    sound_id,
                    volume,
                    loop_playback,
                    fade_in_frames,
                    fade_out_frames,
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioCommandQueueDiagnostics {
    pub enqueue_to_apply_ns: bmz_core::latency::DistributionSummary,
    pub scheduled_sound_count: u64,
    pub scheduling_late_frames: u64,
    pub scheduling_max_late_frames: u64,
    pub submitted: u64,
    pub dropped: u64,
    pub drained: u64,
    pub coalesced: u64,
    pub drain_lock_misses: u64,
    pub engine_lock_misses: u64,
    pub max_depth: u64,
}

#[derive(Debug)]
struct AudioCommandQueueCounters {
    enqueue_to_apply_ns: bmz_core::latency::AtomicLatencyHistogram,
    scheduled_sound_count: AtomicU64,
    scheduling_late_frames: AtomicU64,
    scheduling_max_late_frames: AtomicU64,
    submitted: AtomicU64,
    dropped: AtomicU64,
    drained: AtomicU64,
    coalesced: AtomicU64,
    drain_lock_misses: AtomicU64,
    engine_lock_misses: AtomicU64,
    max_depth: AtomicU64,
}

impl Default for AudioCommandQueueCounters {
    fn default() -> Self {
        Self {
            enqueue_to_apply_ns: Default::default(),
            scheduled_sound_count: AtomicU64::new(0),
            scheduling_late_frames: AtomicU64::new(0),
            scheduling_max_late_frames: AtomicU64::new(0),
            submitted: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            drained: AtomicU64::new(0),
            coalesced: AtomicU64::new(0),
            drain_lock_misses: AtomicU64::new(0),
            engine_lock_misses: AtomicU64::new(0),
            max_depth: AtomicU64::new(0),
        }
    }
}

#[derive(Debug)]
struct QueuedCommand {
    enqueued: Option<std::time::Instant>,
    command: AudioEngineCommand,
    cancelled: Option<Arc<AtomicBool>>,
}

#[cfg(test)]
mod delivery_timing_tests {
    use super::*;
    #[test]
    fn command_age_is_observed_at_apply_without_changing_command() {
        let handle = AudioEngineHandle::new(AudioEngine::new(48_000));
        handle.inner.queue.lock().unwrap().push_back(QueuedCommand {
            enqueued: Some(std::time::Instant::now() - std::time::Duration::from_millis(1)),
            command: AudioEngineCommand::SetMasterGain { gain: 0.5 },
            cancelled: None,
        });
        handle.processor().apply_pending_commands_for_tests();
        let summary = handle.diagnostics().enqueue_to_apply_ns;
        assert_eq!(summary.count, 1);
        assert!(summary.max >= 1_000_000);
        assert_eq!(handle.diagnostics().drained, 1);
    }
}
impl QueuedCommand {
    fn is_cancelled(&self) -> bool {
        self.cancelled.as_ref().is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
    }
}

#[derive(Debug)]
struct AudioCommandQueueInner {
    queue: Mutex<VecDeque<QueuedCommand>>,
    capacity: usize,
    counters: AudioCommandQueueCounters,
    output_sample_rate: AtomicU32,
    idle: AtomicBool,
    last_drop_warn_ms: AtomicU64,
}

impl AudioCommandQueueInner {
    /// drop カウンタを進めつつ、rate limit 付きで警告ログを出す。
    /// silent drop のままだとキー音の欠落が診断できないため。
    fn note_dropped(&self, count: u64, reason: &'static str) {
        let dropped_total = self.counters.dropped.fetch_add(count, Ordering::Relaxed) + count;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        let last = self.last_drop_warn_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) >= DROP_WARN_INTERVAL_MS
            && self
                .last_drop_warn_ms
                .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            tracing::warn!(dropped = count, dropped_total, reason, "audio engine command dropped");
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioEngineHandle {
    engine: Arc<Mutex<AudioEngine>>,
    inner: Arc<AudioCommandQueueInner>,
    cancelled: Option<Arc<AtomicBool>>,
}

#[derive(Debug)]
pub struct CommandedAudioEngine {
    engine: Arc<Mutex<AudioEngine>>,
    inner: Arc<AudioCommandQueueInner>,
    command_scratch: Vec<QueuedCommand>,
}

impl AudioEngineHandle {
    /// Commands retain the play lifetime. Retirement rejects both queued commands
    /// and concurrent producers without waiting for gameplay or the callback.
    pub fn for_play(&self, cancelled: Arc<AtomicBool>) -> Self {
        Self { engine: self.engine.clone(), inner: self.inner.clone(), cancelled: Some(cancelled) }
    }
    pub fn new(engine: AudioEngine) -> Self {
        Self::with_capacity(engine, DEFAULT_AUDIO_COMMAND_QUEUE_CAPACITY)
    }

    pub fn with_capacity(mut engine: AudioEngine, capacity: usize) -> Self {
        // Allocate the ordinary playback working set before attaching a callback.
        // A whole command batch can become due in the same audio buffer.
        engine.queue.reserve(capacity.max(1));
        engine.mixer.voices.reserve(capacity.max(1));
        let output_sample_rate = engine.output_sample_rate();
        let idle = engine.is_idle();
        Self {
            engine: Arc::new(Mutex::new(engine)),
            inner: Arc::new(AudioCommandQueueInner {
                queue: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
                capacity: capacity.max(1),
                counters: AudioCommandQueueCounters::default(),
                output_sample_rate: AtomicU32::new(output_sample_rate),
                idle: AtomicBool::new(idle),
                last_drop_warn_ms: AtomicU64::new(0),
            }),
            cancelled: None,
        }
    }

    pub fn processor(&self) -> CommandedAudioEngine {
        CommandedAudioEngine {
            engine: Arc::clone(&self.engine),
            inner: Arc::clone(&self.inner),
            command_scratch: Vec::with_capacity(self.inner.capacity),
        }
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.inner.output_sample_rate.load(Ordering::Relaxed)
    }

    pub fn is_idle(&self) -> bool {
        self.inner.idle.load(Ordering::Relaxed)
    }

    pub fn diagnostics(&self) -> AudioCommandQueueDiagnostics {
        self.inner.diagnostics()
    }

    pub fn set_output_sample_rate(&self, rate: u32) {
        let Ok(mut engine) = self.engine.lock() else {
            return;
        };
        engine.set_output_sample_rate(rate);
        self.inner.output_sample_rate.store(engine.output_sample_rate(), Ordering::Relaxed);
        self.inner.idle.store(engine.is_idle(), Ordering::Relaxed);
    }

    pub fn clone_sample_bank(&self) -> Option<(u32, SampleBank)> {
        let Ok(engine) = self.engine.try_lock() else {
            return None;
        };
        Some((engine.output_sample_rate(), engine.samples.clone()))
    }

    /// 現在の voice / schedule を破棄し、その直後にシーク位置の carry-over 音を
    /// 同じ command batch で登録して再開する。古い frame 基準の音が新しい再生へ漏れない。
    pub fn replace_playback(&self, sounds: Vec<ScheduledSound>) -> bool {
        let mut commands = Vec::with_capacity(usize::from(!sounds.is_empty()) + 2);
        commands.push(AudioEngineCommand::SetPlaybackPaused { paused: false });
        commands.push(AudioEngineCommand::ClearPlayback);
        if !sounds.is_empty() {
            commands.push(AudioEngineCommand::ScheduleAll(sounds));
        }
        self.push_commands(commands)
    }

    /// 一時停止中のviewer seek用。既存のpause区間を閉じて再生内容を差し替え、
    /// 同じcallback frameを新しいpause開始点として設定する。
    pub fn replace_playback_paused(&self, sounds: Vec<ScheduledSound>) -> bool {
        let mut commands = Vec::with_capacity(usize::from(!sounds.is_empty()) + 3);
        commands.push(AudioEngineCommand::SetPlaybackPaused { paused: false });
        commands.push(AudioEngineCommand::ClearPlayback);
        if !sounds.is_empty() {
            commands.push(AudioEngineCommand::ScheduleAll(sounds));
        }
        commands.push(AudioEngineCommand::SetPlaybackPaused { paused: true });
        self.push_commands(commands)
    }

    pub fn push_command(&self, command: AudioEngineCommand) -> bool {
        self.push_command_or_return(command).is_ok()
    }

    pub fn push_commands(&self, commands: Vec<AudioEngineCommand>) -> bool {
        if commands.is_empty() {
            return true;
        }
        match self.inner.queue.lock() {
            Ok(mut queue) => {
                if self
                    .cancelled
                    .as_ref()
                    .is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
                {
                    return false;
                }
                let coalescible = count_coalescible_pending_commands(&queue, &commands);
                if queue.len().saturating_sub(coalescible).saturating_add(commands.len())
                    > self.inner.capacity
                {
                    self.inner.note_dropped(commands.len() as u64, "queue full");
                    return false;
                }
                let coalesced = coalesce_pending_commands(&mut queue, &commands);
                if coalesced != 0 {
                    self.inner.counters.coalesced.fetch_add(coalesced as u64, Ordering::Relaxed);
                }
                let command_count = commands.len() as u64;
                queue.extend(commands.into_iter().map(|command| QueuedCommand {
                    enqueued:
                        bmz_core::latency::diagnostics_enabled().then(std::time::Instant::now),
                    command,
                    cancelled: self.cancelled.clone(),
                }));
                self.inner.counters.submitted.fetch_add(command_count, Ordering::Relaxed);
                update_atomic_max(&self.inner.counters.max_depth, queue.len() as u64);
                true
            }
            Err(_) => {
                self.inner.note_dropped(commands.len() as u64, "queue lock poisoned");
                false
            }
        }
    }

    pub fn insert_sample(&self, id: SoundId, sample: DecodedSample) -> bool {
        self.push_command(AudioEngineCommand::InsertSample { id, sample })
    }

    pub fn reserve_sample_slot(&self, id: SoundId) -> bool {
        self.push_command(AudioEngineCommand::ReserveSampleSlot { id })
    }

    pub fn insert_prepared_sample(&self, id: SoundId, sample: DecodedSample) -> bool {
        self.push_command(AudioEngineCommand::InsertPreparedSample { id, sample })
    }

    pub fn schedule_sound(&self, sound: ScheduledSound) -> bool {
        self.push_command(AudioEngineCommand::Schedule(sound))
    }

    pub fn schedule_all(&self, sounds: Vec<ScheduledSound>) -> bool {
        if sounds.is_empty() {
            return true;
        }
        self.push_command(AudioEngineCommand::ScheduleAll(sounds))
    }

    pub fn try_schedule_all(&self, sounds: Vec<ScheduledSound>) -> Result<(), Vec<ScheduledSound>> {
        if sounds.is_empty() {
            return Ok(());
        }
        match self.push_command_or_return(AudioEngineCommand::ScheduleAll(sounds)) {
            Ok(()) => Ok(()),
            Err(AudioEngineCommand::ScheduleAll(sounds)) => Err(sounds),
            Err(_) => unreachable!("schedule_all command returned a different command"),
        }
    }

    pub fn stop_sound(&self, id: SoundId) -> bool {
        self.push_command(AudioEngineCommand::StopSound { id })
    }

    pub fn stop_sound_with_fade_out(&self, id: SoundId, fade_out_frames: u32) -> bool {
        self.push_command(AudioEngineCommand::StopSoundWithFadeOut { id, fade_out_frames })
    }

    pub fn set_master_gain(&self, gain: f32) -> bool {
        self.push_command(AudioEngineCommand::SetMasterGain { gain })
    }

    pub fn set_playback_paused(&self, paused: bool) -> bool {
        self.push_command(AudioEngineCommand::SetPlaybackPaused { paused })
    }

    pub fn apply_playback_rate_change(&self, change: crate::clock::PlaybackRateChange) -> bool {
        self.push_command(AudioEngineCommand::ApplyPlaybackRateChange { change })
    }

    pub fn set_sound_volume(&self, id: SoundId, volume: f32) -> bool {
        self.push_command(AudioEngineCommand::SetSoundVolume { id, volume })
    }

    pub fn play_now(&self, sound_id: SoundId, volume: f32, loop_playback: bool) -> bool {
        self.push_command(AudioEngineCommand::PlayNow { sound_id, volume, loop_playback })
    }

    pub fn play_now_with_voice_limit(
        &self,
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        max_voices: usize,
    ) -> bool {
        self.push_command(AudioEngineCommand::PlayNowWithVoiceLimit {
            sound_id,
            volume,
            loop_playback,
            max_voices,
        })
    }

    pub fn play_now_with_fade_in(
        &self,
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
    ) -> bool {
        self.push_command(AudioEngineCommand::PlayNowWithFadeIn {
            sound_id,
            volume,
            loop_playback,
            fade_in_frames,
        })
    }

    pub fn play_now_with_fade_in_and_fade_out(
        &self,
        sound_id: SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
        fade_out_frames: u32,
    ) -> bool {
        self.push_command(AudioEngineCommand::PlayNowWithFadeInAndFadeOut {
            sound_id,
            volume,
            loop_playback,
            fade_in_frames,
            fade_out_frames,
        })
    }

    fn push_command_or_return(
        &self,
        command: AudioEngineCommand,
    ) -> Result<(), AudioEngineCommand> {
        match self.inner.queue.lock() {
            Ok(mut queue) => {
                let coalescible = usize::from(is_pending_command_coalescible(&queue, &command));
                if self
                    .cancelled
                    .as_ref()
                    .is_some_and(|cancelled| cancelled.load(Ordering::Acquire))
                {
                    return Err(command);
                }
                if queue.len().saturating_sub(coalescible).saturating_add(1) > self.inner.capacity {
                    self.inner.note_dropped(1, "queue full");
                    return Err(command);
                }
                let coalesced = coalesce_pending_command(&mut queue, &command);
                if coalesced != 0 {
                    self.inner.counters.coalesced.fetch_add(coalesced as u64, Ordering::Relaxed);
                }
                queue.push_back(QueuedCommand {
                    enqueued: bmz_core::latency::diagnostics_enabled()
                        .then(std::time::Instant::now),
                    command,
                    cancelled: self.cancelled.clone(),
                });
                self.inner.counters.submitted.fetch_add(1, Ordering::Relaxed);
                update_atomic_max(&self.inner.counters.max_depth, queue.len() as u64);
                Ok(())
            }
            Err(_) => {
                self.inner.note_dropped(1, "queue lock poisoned");
                Err(command)
            }
        }
    }
}

impl AudioScheduler for AudioEngineHandle {
    fn schedule(&mut self, sound: ScheduledSound) {
        self.schedule_sound(sound);
    }
}

impl CommandedAudioEngine {
    pub fn render_stereo(&mut self, output_start_frame: u64, output: &mut [f32]) -> bool {
        let engine = Arc::clone(&self.engine);
        let mut engine = match engine.try_lock() {
            Ok(engine) => engine,
            Err(TryLockError::WouldBlock) => {
                self.inner.counters.engine_lock_misses.fetch_add(1, Ordering::Relaxed);
                output.fill(0.0);
                return false;
            }
            Err(TryLockError::Poisoned(_)) => {
                self.inner.counters.engine_lock_misses.fetch_add(1, Ordering::Relaxed);
                output.fill(0.0);
                return false;
            }
        };
        self.apply_pending_commands(&mut engine, output_start_frame);
        engine.render_stereo(output_start_frame, output);
        self.inner.output_sample_rate.store(engine.output_sample_rate(), Ordering::Relaxed);
        self.inner.idle.store(engine.is_idle(), Ordering::Relaxed);
        true
    }

    pub fn apply_pending_commands_for_tests(&mut self) {
        let engine = Arc::clone(&self.engine);
        let Ok(mut engine) = engine.lock() else {
            return;
        };
        self.apply_pending_commands(&mut engine, 0);
        self.inner.output_sample_rate.store(engine.output_sample_rate(), Ordering::Relaxed);
        self.inner.idle.store(engine.is_idle(), Ordering::Relaxed);
    }

    fn apply_pending_commands(&mut self, engine: &mut AudioEngine, output_frame: u64) {
        self.command_scratch.clear();
        match self.inner.queue.try_lock() {
            Ok(mut queue) => {
                debug_assert!(self.command_scratch.capacity() >= queue.len());
                while let Some(command) = queue.pop_front() {
                    self.command_scratch.push(command);
                }
            }
            Err(TryLockError::WouldBlock) => {
                self.inner.counters.drain_lock_misses.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(TryLockError::Poisoned(_)) => {
                self.inner.counters.drain_lock_misses.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        let drained = self.command_scratch.len() as u64;
        for queued in self.command_scratch.drain(..) {
            if queued.is_cancelled() {
                continue;
            }
            if let Some(enqueued) = queued.enqueued {
                self.inner
                    .counters
                    .enqueue_to_apply_ns
                    .record(enqueued.elapsed().as_nanos().min(u64::MAX as u128) as u64);
            }
            let command = queued.command;
            // Only atomic counters here: the audio callback must never log,
            // allocate a diagnostic buffer, or wait for the diagnostics reader.
            let sounds = match &command {
                AudioEngineCommand::Schedule(sound) => std::slice::from_ref(sound),
                AudioEngineCommand::ScheduleAll(sounds) => sounds.as_slice(),
                _ => &[],
            };
            for sound in sounds {
                let late = output_frame.saturating_sub(sound.start_frame);
                self.inner.counters.scheduled_sound_count.fetch_add(1, Ordering::Relaxed);
                self.inner.counters.scheduling_late_frames.fetch_add(late, Ordering::Relaxed);
                self.inner.counters.scheduling_max_late_frames.fetch_max(late, Ordering::Relaxed);
            }
            command.apply(engine, output_frame);
        }
        if drained != 0 {
            self.inner.counters.drained.fetch_add(drained, Ordering::Relaxed);
        }
    }
}

impl AudioCommandQueueInner {
    fn diagnostics(&self) -> AudioCommandQueueDiagnostics {
        AudioCommandQueueDiagnostics {
            enqueue_to_apply_ns: self.counters.enqueue_to_apply_ns.summary(),
            scheduled_sound_count: self.counters.scheduled_sound_count.load(Ordering::Relaxed),
            scheduling_late_frames: self.counters.scheduling_late_frames.load(Ordering::Relaxed),
            scheduling_max_late_frames: self
                .counters
                .scheduling_max_late_frames
                .load(Ordering::Relaxed),
            submitted: self.counters.submitted.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
            drained: self.counters.drained.load(Ordering::Relaxed),
            coalesced: self.counters.coalesced.load(Ordering::Relaxed),
            drain_lock_misses: self.counters.drain_lock_misses.load(Ordering::Relaxed),
            engine_lock_misses: self.counters.engine_lock_misses.load(Ordering::Relaxed),
            max_depth: self.counters.max_depth.load(Ordering::Relaxed),
        }
    }
}

fn count_coalescible_pending_commands(
    queue: &VecDeque<QueuedCommand>,
    incoming: &[AudioEngineCommand],
) -> usize {
    queue
        .iter()
        .filter(|pending| incoming.iter().any(|next| command_supersedes(next, &pending.command)))
        .count()
}

fn coalesce_pending_commands(
    queue: &mut VecDeque<QueuedCommand>,
    incoming: &[AudioEngineCommand],
) -> usize {
    let before = queue.len();
    queue.retain(|pending| !incoming.iter().any(|next| command_supersedes(next, &pending.command)));
    before.saturating_sub(queue.len())
}

fn is_pending_command_coalescible(
    queue: &VecDeque<QueuedCommand>,
    incoming: &AudioEngineCommand,
) -> bool {
    queue.iter().any(|pending| command_supersedes(incoming, &pending.command))
}

fn coalesce_pending_command(
    queue: &mut VecDeque<QueuedCommand>,
    incoming: &AudioEngineCommand,
) -> usize {
    let before = queue.len();
    queue.retain(|pending| !command_supersedes(incoming, &pending.command));
    before.saturating_sub(queue.len())
}

fn command_supersedes(incoming: &AudioEngineCommand, pending: &AudioEngineCommand) -> bool {
    match (incoming, pending) {
        (AudioEngineCommand::SetMasterGain { .. }, AudioEngineCommand::SetMasterGain { .. }) => {
            true
        }
        (
            AudioEngineCommand::SetSoundVolume { id: incoming_id, .. },
            AudioEngineCommand::SetSoundVolume { id: pending_id, .. },
        ) => incoming_id == pending_id,
        _ => false,
    }
}

fn update_atomic_max(atomic: &AtomicU64, value: u64) {
    let mut current = atomic.load(Ordering::Relaxed);
    while value > current {
        match atomic.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::DecodedSample;

    #[test]
    fn retired_play_rejects_queued_and_concurrent_audio_commands() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 8] },
        );
        let handle = AudioEngineHandle::new(engine);
        let retired = Arc::new(AtomicBool::new(false));
        let old = handle.for_play(retired.clone());
        assert!(old.schedule_sound(ScheduledSound::one_shot(0, SoundId(1), 0.25, 0.0)));
        retired.store(true, Ordering::Release);
        assert!(!old.schedule_sound(ScheduledSound::one_shot(0, SoundId(1), 0.25, 0.0)));
        let current = handle.for_play(Arc::new(AtomicBool::new(false)));
        assert!(current.schedule_sound(ScheduledSound::one_shot(0, SoundId(1), 0.75, 0.0)));
        let mut processor = handle.processor();
        let mut output = [0.0; 2];
        assert!(processor.render_stereo(0, &mut output));
        assert_eq!(output, [0.75; 2]);
        assert_eq!(handle.diagnostics().scheduled_sound_count, 1);
    }

    #[test]
    fn command_queue_applies_commands_before_rendering() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 8);
        let mut processor = handle.processor();
        handle.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        handle.play_now(SoundId(1), 0.25, false);

        let mut output = vec![0.0; 4];
        assert!(processor.render_stereo(0, &mut output));

        assert_eq!(output, vec![0.25, 0.25, 0.25, 0.25]);
        assert_eq!(handle.diagnostics().submitted, 2);
        assert_eq!(handle.diagnostics().drained, 2);
    }

    #[test]
    fn replace_playback_clears_older_schedule_in_the_same_batch() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25] },
        );
        engine.insert_sample(
            SoundId(2),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.75] },
        );
        let handle = AudioEngineHandle::with_capacity(engine, 8);
        let mut processor = handle.processor();
        assert!(handle.schedule_all(vec![ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0,)]));
        assert!(handle.replace_playback(vec![ScheduledSound::one_shot(0, SoundId(2), 1.0, 0.0,)]));

        let mut output = vec![0.0; 2];
        assert!(processor.render_stereo(0, &mut output));

        assert_eq!(output, vec![0.75, 0.75]);
    }

    #[test]
    fn replace_playback_resumes_a_waiting_engine() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] },
        );
        let handle = AudioEngineHandle::with_capacity(engine, 8);
        let mut processor = handle.processor();
        let mut output = vec![0.0; 2];

        assert!(handle.set_playback_paused(true));
        assert!(processor.render_stereo(0, &mut output));
        assert_eq!(output, vec![0.0, 0.0]);

        assert!(
            handle.replace_playback(vec![ScheduledSound::one_shot(100, SoundId(1), 1.0, 0.0,)])
        );
        assert!(processor.render_stereo(100, &mut output));
        assert_eq!(output, vec![1.0, 1.0]);
    }

    #[test]
    fn playback_pause_freezes_active_voice_until_resume() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 0.75, 0.5, 0.25] },
        );
        let handle = AudioEngineHandle::with_capacity(engine, 8);
        let mut processor = handle.processor();
        assert!(handle.schedule_sound(ScheduledSound::one_shot(100, SoundId(1), 1.0, 0.0,)));

        let mut output = vec![0.0; 2];
        assert!(processor.render_stereo(100, &mut output));
        assert_eq!(output, vec![1.0, 1.0]);

        assert!(handle.set_playback_paused(true));
        assert!(processor.render_stereo(101, &mut output));
        assert_eq!(output, vec![0.0, 0.0]);
        assert!(processor.render_stereo(110, &mut output));
        assert_eq!(output, vec![0.0, 0.0]);

        assert!(handle.set_playback_paused(false));
        assert!(processor.render_stereo(111, &mut output));
        assert_eq!(output, vec![0.75, 0.75]);
    }

    #[test]
    fn paused_playback_replacement_reanchors_pause_for_new_schedule() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] },
        );
        let handle = AudioEngineHandle::with_capacity(engine, 8);
        let mut processor = handle.processor();

        assert!(handle.set_playback_paused(true));
        let mut output = vec![0.0; 2];
        assert!(processor.render_stereo(100, &mut output));
        assert!(handle.replace_playback_paused(vec![ScheduledSound::one_shot(
            200,
            SoundId(1),
            1.0,
            0.0,
        )]));
        assert!(processor.render_stereo(200, &mut output));
        assert_eq!(output, vec![0.0, 0.0]);

        assert!(handle.set_playback_paused(false));
        assert!(processor.render_stereo(300, &mut output));
        assert_eq!(output, vec![1.0, 1.0]);
    }

    #[test]
    fn command_queue_drops_when_capacity_is_full() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 1);

        assert!(handle.set_master_gain(0.5));
        assert!(!handle.play_now(SoundId(1), 0.25, false));

        let diagnostics = handle.diagnostics();
        assert_eq!(diagnostics.submitted, 1);
        assert_eq!(diagnostics.dropped, 1);
        assert_eq!(diagnostics.max_depth, 1);
    }

    #[test]
    fn command_queue_coalesces_pending_volume_updates() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 8);
        let mut processor = handle.processor();
        handle.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        processor.apply_pending_commands_for_tests();
        handle.play_now(SoundId(1), 1.0, true);
        let mut output = vec![0.0; 2];
        processor.render_stereo(0, &mut output);

        assert!(handle.set_sound_volume(SoundId(1), 0.5));
        assert!(handle.set_sound_volume(SoundId(1), 0.25));
        let mut output = vec![0.0; 2];
        processor.render_stereo(1, &mut output);

        assert_eq!(output, vec![0.25, 0.25]);
        assert_eq!(handle.diagnostics().coalesced, 1);
    }

    #[test]
    fn command_queue_keeps_sequential_playback_rate_anchors() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 8);
        let mut processor = handle.processor();

        for change in [
            crate::clock::PlaybackRateChange {
                anchor_output_frame: 100,
                old_rate_percent: 100,
                new_rate_percent: 25,
            },
            crate::clock::PlaybackRateChange {
                anchor_output_frame: 120,
                old_rate_percent: 25,
                new_rate_percent: 50,
            },
        ] {
            assert!(handle.apply_playback_rate_change(change));
        }

        assert_eq!(handle.diagnostics().coalesced, 0);
        processor.apply_pending_commands_for_tests();
        assert_eq!(handle.diagnostics().drained, 2);
        assert_eq!(handle.engine.lock().unwrap().mixer.playback_rate, 0.5);
    }

    #[test]
    fn command_queue_applies_play_now_with_fade_out() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 8);
        let mut processor = handle.processor();
        handle.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0] },
        );
        handle.play_now_with_fade_in_and_fade_out(SoundId(1), 1.0, false, 0, 2);

        let mut output = vec![0.0; 6];
        assert!(processor.render_stereo(0, &mut output));

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn command_queue_updates_idle_snapshot_after_render() {
        let handle = AudioEngineHandle::with_capacity(AudioEngine::default(), 8);
        let mut processor = handle.processor();
        handle.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] },
        );
        handle.play_now(SoundId(1), 1.0, true);

        let mut output = vec![0.0; 2];
        processor.render_stereo(0, &mut output);

        assert!(!handle.is_idle());
    }
}
