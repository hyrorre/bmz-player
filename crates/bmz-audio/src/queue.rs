use crate::clock::{OutputBufferTime, duration_to_frames};
use bmz_core::ids::SoundId;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestartPolicy {
    #[default]
    Overlap,
    StopSameSound,
}

#[derive(Debug, Clone, Copy)]
pub struct ScheduledSound {
    /// Start at the head of the next rendered buffer. Pending immediate
    /// requests have no output frame or system deadline yet.
    pub immediate: bool,
    /// Live playback deadline in the system monotonic clock. Resolved only when
    /// its output buffer is rendered. With no deadline and `immediate == false`,
    /// `start_frame` is an absolute output frame, including frame zero.
    pub start_time: Option<Instant>,
    pub start_frame: u64,
    /// Sample frame to begin reading from when this voice starts.
    ///
    /// Viewer seek uses this for a BGM event that began before the requested
    /// chart position but is still playing there. Late callback catch-up is
    /// applied on top of this offset.
    pub sample_offset_frames: u64,
    pub sound_id: SoundId,
    pub volume: f32,
    pub pan: f32,
    /// `true` のときはサンプル末尾でループ再生する(BGM 等)。
    /// `false` なら従来通り 1 回再生して voice を破棄する。
    pub loop_playback: bool,
    /// Number of output frames used for an initial attack fade.
    pub fade_in_frames: u32,
    /// Fade out from the voice's first frame. Zero disables the fade.
    pub fade_out_frames: u32,
    /// If rendering reaches this sound late, advance the sample cursor by the
    /// missed frames. Chart-timeline sounds use this to stay synced to the
    /// audio clock; immediate UI sounds keep it false so their first audible
    /// buffer starts from the sample head.
    pub catch_up: bool,
    pub restart_policy: RestartPolicy,
}

impl ScheduledSound {
    pub fn immediate(sound_id: SoundId, volume: f32, pan: f32) -> Self {
        Self { immediate: true, catch_up: false, ..Self::one_shot(0, sound_id, volume, pan) }
    }

    /// 既存呼び出し互換用: ループしない通常のスケジュール音。
    pub fn one_shot(start_frame: u64, sound_id: SoundId, volume: f32, pan: f32) -> Self {
        Self {
            immediate: false,
            start_time: None,
            start_frame,
            sample_offset_frames: 0,
            sound_id,
            volume,
            pan,
            loop_playback: false,
            fade_in_frames: 0,
            fade_out_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }
    }
}

pub trait AudioScheduler {
    fn schedule(&mut self, sound: ScheduledSound);
}

#[derive(Debug, Default, Clone)]
pub struct ScheduledSoundQueue {
    sounds: Vec<QueuedSound>,
    next_order: u64,
}

#[derive(Debug, Clone)]
struct QueuedSound {
    sound: ScheduledSound,
    // Keep request order across time resolution, retiming and batch transfers.
    order: u64,
    // A late deadline can quantize or clamp to the head of the current
    // buffer. It must still precede requests starting exactly at that head.
    late_at_buffer_start: bool,
}

impl QueuedSound {
    fn key(&self) -> (Option<Instant>, bool, u64, u32, bool, u64) {
        let sound = self.sound;
        let time = if sound.immediate { None } else { sound.start_time };
        let frame = if sound.immediate || time.is_some() { 0 } else { sound.start_frame };
        (time, sound.immediate, frame, sound.sound_id.0, !self.late_at_buffer_start, self.order)
    }
}

impl ScheduledSoundQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reserve(&mut self, additional: usize) {
        self.sounds.reserve(additional);
    }

    pub fn drain_until_frame(&mut self, frame: u64) -> Vec<ScheduledSound> {
        self.drain_due(frame).collect()
    }

    /// `drain_until_frame` のアロケーションなし版。期日到来分をiteratorとして返し、
    /// 呼び出し側で直接消費する。オーディオコールバック(`render_stereo`)から
    /// 毎回呼ばれるため、中間 `Vec` を確保しない。
    pub fn drain_due(&mut self, frame: u64) -> impl ExactSizeIterator<Item = ScheduledSound> + '_ {
        let split = self.sounds.partition_point(|queued| {
            let sound = queued.sound;
            !sound.immediate && sound.start_time.is_none() && sound.start_frame <= frame
        });
        self.sounds.drain(..split).map(|queued| queued.sound)
    }

    pub fn drain_all(&mut self) -> impl ExactSizeIterator<Item = ScheduledSound> + '_ {
        self.sounds.drain(..).map(|queued| queued.sound)
    }

    /// Transfers requests to another queue without losing their original order.
    pub fn drain_in_request_order(&mut self) -> impl ExactSizeIterator<Item = ScheduledSound> + '_ {
        self.sounds.sort_unstable_by_key(|queued| queued.order);
        self.drain_all()
    }

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    pub fn clear(&mut self) {
        self.sounds.clear();
    }

    /// Keep future deadlines in system time. Only this buffer's events become
    /// device frames, so a clock-rate error cannot accumulate over the song.
    pub(crate) fn resolve_times(
        &mut self,
        frame: u64,
        time: Option<OutputBufferTime>,
        frames: usize,
        rate: u32,
    ) {
        if frames == 0 {
            return;
        }
        let span_ns =
            time.map_or(0, |time| time.end.saturating_duration_since(time.start).as_nanos());
        let mut changed = false;
        for queued in &mut self.sounds {
            let sound = &mut queued.sound;
            if sound.immediate {
                sound.immediate = false;
                sound.start_time = None;
                sound.start_frame = frame;
                changed = true;
                continue;
            }
            let Some(deadline) = sound.start_time else {
                continue;
            };
            let Some(time) = time else { break };
            // Timed entries are sorted. Do not scan the rest of the lookahead
            // window on every small device buffer.
            if deadline >= time.end {
                break;
            }
            sound.start_frame = if deadline >= time.start {
                // Smoothing changes the represented time span, not the number
                // of PCM frames. Map the whole range into this buffer. A deadline
                // inside the range means `end > start`, so the span is not zero.
                let elapsed_ns = deadline.duration_since(time.start).as_nanos();
                let offset = elapsed_ns * frames as u128 / span_ns;
                frame.saturating_add(offset as u64)
            } else {
                frame.saturating_sub(duration_to_frames(time.start.duration_since(deadline), rate))
            };
            queued.late_at_buffer_start = deadline < time.start && sound.start_frame == frame;
            sound.start_time = None;
            changed = true;
        }
        if changed {
            self.sort();
        }
    }

    pub fn shift_times(&mut self, duration: Duration) {
        for queued in &mut self.sounds {
            let sound = &mut queued.sound;
            sound.start_time =
                sound.start_time.map(|time| time.checked_add(duration).unwrap_or(time));
        }
    }

    fn sort(&mut self) {
        self.sounds.sort_unstable_by_key(QueuedSound::key);
    }

    pub fn shift_output_frames(&mut self, frames: u64) {
        if frames == 0 {
            return;
        }
        for queued in &mut self.sounds {
            let sound = &mut queued.sound;
            if !sound.immediate {
                sound.start_frame = sound.start_frame.saturating_add(frames);
            }
        }
    }

    /// 述語が `true` を返すスケジュール音だけを保持する。`stop_sound` 等で使う。
    pub fn retain(&mut self, mut keep: impl FnMut(&ScheduledSound) -> bool) {
        self.sounds.retain(|queued| keep(&queued.sound));
    }

    /// 指定 sound_id の再生待ち音量を更新する。ループ BGM や preview の
    /// 設定変更を、次に mixer へ渡る前の queue に反映するために使う。
    pub fn set_volume_for_sound(&mut self, id: SoundId, volume: f32) {
        for queued in &mut self.sounds {
            let sound = &mut queued.sound;
            if sound.sound_id == id {
                sound.volume = volume;
            }
        }
    }

    pub fn apply_playback_rate_change(&mut self, change: crate::clock::PlaybackRateChange) {
        for queued in &mut self.sounds {
            let sound = &mut queued.sound;
            if sound.immediate {
                continue;
            }
            sound.start_frame = retime_output_frame(sound.start_frame, change);
            if let (Some(deadline), Some(anchor)) = (sound.start_time, change.changed_at)
                && deadline > anchor
            {
                let nanos = deadline.duration_since(anchor).as_nanos()
                    * u128::from(change.old_rate_percent)
                    / u128::from(change.new_rate_percent);
                sound.start_time = anchor
                    .checked_add(Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64));
            }
        }
        self.sort();
    }
}

pub(crate) fn retime_output_frame(frame: u64, change: crate::clock::PlaybackRateChange) -> u64 {
    if frame <= change.anchor_output_frame {
        return frame;
    }
    let distance = frame - change.anchor_output_frame;
    let scaled = u128::from(distance).saturating_mul(u128::from(change.old_rate_percent))
        / u128::from(change.new_rate_percent);
    change.anchor_output_frame.saturating_add(scaled.min(u128::from(u64::MAX)) as u64)
}

impl AudioScheduler for ScheduledSoundQueue {
    fn schedule(&mut self, sound: ScheduledSound) {
        if self.sounds.is_empty() {
            self.next_order = 0;
        }
        let queued = QueuedSound { sound, order: self.next_order, late_at_buffer_start: false };
        self.next_order += 1;
        let key = queued.key();
        let index = self.sounds.partition_point(|existing| existing.key() < key);
        self.sounds.insert(index, queued);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_sound_queue_drains_in_frame_order() {
        let mut queue = ScheduledSoundQueue::new();
        queue.schedule(sound(20, 2));
        queue.schedule(sound(10, 1));
        queue.schedule(sound(10, 0));

        let drained = queue.drain_until_frame(10);

        assert_eq!(drained.iter().map(|sound| sound.sound_id.0).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.drain_until_frame(20)[0].sound_id.0, 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn playback_rate_change_retimes_future_sounds_around_anchor() {
        let mut queue = ScheduledSoundQueue::new();
        queue.schedule(sound(50, 1));
        queue.schedule(sound(400, 2));

        queue.apply_playback_rate_change(crate::clock::PlaybackRateChange {
            changed_at: None,
            anchor_output_frame: 100,
            old_rate_percent: 100,
            new_rate_percent: 300,
        });

        assert_eq!(queue.drain_until_frame(99)[0].start_frame, 50);
        assert_eq!(queue.drain_until_frame(200)[0].start_frame, 200);
    }

    #[test]
    fn playback_rate_change_expands_future_schedule_when_slowing_down() {
        assert_eq!(
            retime_output_frame(
                200,
                crate::clock::PlaybackRateChange {
                    changed_at: None,
                    anchor_output_frame: 100,
                    old_rate_percent: 100,
                    new_rate_percent: 25,
                },
            ),
            500
        );
    }

    fn sound(start_frame: u64, sound_id: u32) -> ScheduledSound {
        ScheduledSound {
            immediate: false,
            start_time: None,
            start_frame,
            sample_offset_frames: 0,
            sound_id: SoundId(sound_id),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            fade_out_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }
    }

    fn resolve(queue: &mut ScheduledSoundQueue, frame: u64, start: Instant) {
        queue.resolve_times(
            frame,
            Some(OutputBufferTime::for_frames(start, 480, 48_000)),
            480,
            48_000,
        );
    }

    #[test]
    fn live_deadlines_wait_for_current_buffer_and_survive_pause_and_rate_changes() {
        let base = Instant::now();
        let mut queue = ScheduledSoundQueue::new();
        let mut event = sound(0, 1);
        event.start_time = Some(base + Duration::from_millis(100));
        queue.schedule(event);
        resolve(&mut queue, 99_000, base);
        assert!(queue.drain_until_frame(u64::MAX).is_empty());
        queue.apply_playback_rate_change(crate::clock::PlaybackRateChange {
            changed_at: Some(base),
            anchor_output_frame: 99_000,
            old_rate_percent: 100,
            new_rate_percent: 200,
        });
        queue.shift_times(Duration::from_secs(10));
        resolve(&mut queue, 7_000, base + Duration::from_millis(10_045));
        let event = queue.drain_until_frame(7_479).pop().unwrap();
        assert_eq!(event.start_frame, 7_240);
        assert!(queue.is_empty());
    }

    #[test]
    fn late_live_events_preserve_the_catch_up_policy_and_boundary_is_exclusive() {
        let base = Instant::now();
        let mut queue = ScheduledSoundQueue::new();
        for (id, offset, catch_up) in [(1, 0, true), (2, 0, false), (3, 20, false)] {
            let mut event = sound(0, id);
            event.start_time = Some(base + Duration::from_millis(offset));
            event.catch_up = catch_up;
            queue.schedule(event);
        }
        resolve(&mut queue, 1000, base + Duration::from_millis(10));
        let due = queue.drain_until_frame(u64::MAX);
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].start_frame, 520);
        assert!(due[0].catch_up);
        assert!(!due[1].catch_up);
        assert_eq!(queue.len(), 1);
        resolve(&mut queue, 1480, base + Duration::from_millis(20));
        assert_eq!(queue.drain_until_frame(1480)[0].sound_id, SoundId(3));
    }

    #[test]
    fn adjusted_time_ranges_resolve_into_the_current_buffer() {
        let start = Instant::now();
        for span in [Duration::from_millis(8), Duration::from_millis(12)] {
            let time = OutputBufferTime { start, end: start + span };
            let mut queue = ScheduledSoundQueue::new();
            for (id, deadline) in [
                start - Duration::from_millis(1),
                start,
                start + span / 2,
                time.end - Duration::from_nanos(1),
                time.end,
            ]
            .into_iter()
            .enumerate()
            {
                let mut event = sound(0, id as u32);
                event.start_time = Some(deadline);
                queue.schedule(event);
            }
            queue.resolve_times(100_000, Some(time), 480, 48_000);
            let due = queue.drain_until_frame(100_479);
            let starts: Vec<_> = due.iter().map(|event| event.start_frame).collect();
            // Real lateness still uses the sample rate. In-range deadlines
            // cover all 480 frames, and the end belongs to the next buffer.
            assert_eq!(starts, [99_952, 100_000, 100_240, 100_479], "{span:?}");
            assert_eq!(queue.len(), 1);
            assert!(queue.drain_until_frame(u64::MAX).is_empty());
        }
    }

    #[test]
    fn resolving_tied_events_keeps_last_request_wins_order_without_stable_sort_allocation() {
        let base = Instant::now();
        let mut queue = ScheduledSoundQueue::new();
        // Mixed IDs force a reorder when timed events become frame events.
        // Same-time requests for the same WAV must retain their gain/policy order.
        for index in 0..200 {
            let mut event = sound(0, index % 3);
            event.start_time = Some(base);
            event.volume = index as f32;
            queue.schedule(event);
        }
        resolve(&mut queue, 1000, base);
        let events = queue.drain_until_frame(1479);
        for id in 0..3 {
            let volumes: Vec<_> = events
                .iter()
                .filter(|e| e.sound_id == SoundId(id))
                .map(|e| e.volume as u32)
                .collect();
            assert_eq!(volumes, (0..200).filter(|index| index % 3 == id).collect::<Vec<_>>());
        }
    }

    fn timed_bgm(deadline: Instant) -> ScheduledSound {
        let mut event = sound(0, 1);
        event.start_time = Some(deadline);
        event.catch_up = true;
        event
    }

    #[test]
    fn stall_with_contiguous_frames_does_not_pile_up_late_bgm() {
        let base = Instant::now();
        let mut queue = ScheduledSoundQueue::new();
        for ms in [15, 25, 35] {
            queue.schedule(timed_bgm(base + Duration::from_millis(ms)));
        }
        resolve(&mut queue, 100_000, base);
        resolve(&mut queue, 100_480, base + Duration::from_millis(60));
        let starts: Vec<_> =
            queue.drain_until_frame(u64::MAX).iter().map(|sound| sound.start_frame).collect();
        assert_eq!(starts, [100_480 - 45 * 48, 100_480 - 35 * 48, 100_480 - 25 * 48]);
    }
}
