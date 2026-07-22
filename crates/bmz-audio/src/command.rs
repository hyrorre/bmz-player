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
    pub fn apply(self, engine: &mut AudioEngine) {
        match self {
            Self::InsertSample { id, sample } => engine.insert_sample(id, sample),
            Self::ReserveSampleSlot { id } => engine.reserve_sample_slot(id),
            Self::InsertPreparedSample { id, sample } => engine.insert_prepared_sample(id, sample),
            Self::Schedule(sound) => engine.schedule(sound),
            Self::ScheduleAll(sounds) => engine.schedule_all(sounds),
            Self::StopSound { id } => engine.stop_sound(id),
            Self::StopSoundWithFadeOut { id, fade_out_frames } => {
                engine.stop_sound_with_fade_out(id, fade_out_frames);
            }
            Self::SetMasterGain { gain } => engine.set_master_gain(gain),
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
struct AudioCommandQueueInner {
    queue: Mutex<VecDeque<AudioEngineCommand>>,
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
}

#[derive(Debug)]
pub struct CommandedAudioEngine {
    engine: Arc<Mutex<AudioEngine>>,
    inner: Arc<AudioCommandQueueInner>,
    command_scratch: Vec<AudioEngineCommand>,
}

impl AudioEngineHandle {
    pub fn new(engine: AudioEngine) -> Self {
        Self::with_capacity(engine, DEFAULT_AUDIO_COMMAND_QUEUE_CAPACITY)
    }

    pub fn with_capacity(engine: AudioEngine, capacity: usize) -> Self {
        let output_sample_rate = engine.output_sample_rate();
        let idle = engine.is_idle();
        Self {
            engine: Arc::new(Mutex::new(engine)),
            inner: Arc::new(AudioCommandQueueInner {
                queue: Mutex::new(VecDeque::new()),
                capacity: capacity.max(1),
                counters: AudioCommandQueueCounters::default(),
                output_sample_rate: AtomicU32::new(output_sample_rate),
                idle: AtomicBool::new(idle),
                last_drop_warn_ms: AtomicU64::new(0),
            }),
        }
    }

    pub fn processor(&self) -> CommandedAudioEngine {
        CommandedAudioEngine {
            engine: Arc::clone(&self.engine),
            inner: Arc::clone(&self.inner),
            command_scratch: Vec::new(),
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

    pub fn push_command(&self, command: AudioEngineCommand) -> bool {
        self.push_commands(vec![command])
    }

    pub fn push_commands(&self, commands: Vec<AudioEngineCommand>) -> bool {
        if commands.is_empty() {
            return true;
        }
        match self.inner.queue.lock() {
            Ok(mut queue) => {
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
                queue.extend(commands);
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
                if queue.len().saturating_sub(coalescible).saturating_add(1) > self.inner.capacity {
                    self.inner.note_dropped(1, "queue full");
                    return Err(command);
                }
                let coalesced = coalesce_pending_command(&mut queue, &command);
                if coalesced != 0 {
                    self.inner.counters.coalesced.fetch_add(coalesced as u64, Ordering::Relaxed);
                }
                queue.push_back(command);
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
        self.apply_pending_commands(&mut engine);
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
        self.apply_pending_commands(&mut engine);
        self.inner.output_sample_rate.store(engine.output_sample_rate(), Ordering::Relaxed);
        self.inner.idle.store(engine.is_idle(), Ordering::Relaxed);
    }

    fn apply_pending_commands(&mut self, engine: &mut AudioEngine) {
        self.command_scratch.clear();
        match self.inner.queue.try_lock() {
            Ok(mut queue) => {
                self.command_scratch.reserve(queue.len());
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
        for command in self.command_scratch.drain(..) {
            command.apply(engine);
        }
        if drained != 0 {
            self.inner.counters.drained.fetch_add(drained, Ordering::Relaxed);
        }
    }
}

impl AudioCommandQueueInner {
    fn diagnostics(&self) -> AudioCommandQueueDiagnostics {
        AudioCommandQueueDiagnostics {
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
    queue: &VecDeque<AudioEngineCommand>,
    incoming: &[AudioEngineCommand],
) -> usize {
    queue
        .iter()
        .filter(|pending| incoming.iter().any(|next| command_supersedes(next, pending)))
        .count()
}

fn coalesce_pending_commands(
    queue: &mut VecDeque<AudioEngineCommand>,
    incoming: &[AudioEngineCommand],
) -> usize {
    let before = queue.len();
    queue.retain(|pending| !incoming.iter().any(|next| command_supersedes(next, pending)));
    before.saturating_sub(queue.len())
}

fn is_pending_command_coalescible(
    queue: &VecDeque<AudioEngineCommand>,
    incoming: &AudioEngineCommand,
) -> bool {
    queue.iter().any(|pending| command_supersedes(incoming, pending))
}

fn coalesce_pending_command(
    queue: &mut VecDeque<AudioEngineCommand>,
    incoming: &AudioEngineCommand,
) -> usize {
    let before = queue.len();
    queue.retain(|pending| !command_supersedes(incoming, pending));
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
