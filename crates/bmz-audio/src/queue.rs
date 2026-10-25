use bmz_core::ids::SoundId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestartPolicy {
    #[default]
    Overlap,
    StopSameSound,
}

#[derive(Debug, Clone, Copy)]
pub struct ScheduledSound {
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
    /// If rendering reaches this sound late, advance the sample cursor by the
    /// missed frames. Chart-timeline sounds use this to stay synced to the
    /// audio clock; immediate UI sounds keep it false so their first audible
    /// buffer starts from the sample head.
    pub catch_up: bool,
    pub restart_policy: RestartPolicy,
}

impl ScheduledSound {
    /// 既存呼び出し互換用: ループしない通常のスケジュール音。
    pub fn one_shot(start_frame: u64, sound_id: SoundId, volume: f32, pan: f32) -> Self {
        Self {
            start_frame,
            sample_offset_frames: 0,
            sound_id,
            volume,
            pan,
            loop_playback: false,
            fade_in_frames: 0,
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
    sounds: Vec<ScheduledSound>,
}

impl ScheduledSoundQueue {
    pub fn new() -> Self {
        Self { sounds: Vec::new() }
    }

    pub(crate) fn reserve(&mut self, additional: usize) {
        self.sounds.reserve(additional);
    }

    pub fn drain_until_frame(&mut self, frame: u64) -> Vec<ScheduledSound> {
        let split = self.sounds.partition_point(|sound| sound.start_frame <= frame);
        self.sounds.drain(..split).collect()
    }

    /// `drain_until_frame` のアロケーションなし版。期日到来分を `Drain` として返し、
    /// 呼び出し側で直接消費する。オーディオコールバック(`render_stereo`)から
    /// 毎回呼ばれるため、中間 `Vec` を確保しない。
    pub fn drain_due(&mut self, frame: u64) -> std::vec::Drain<'_, ScheduledSound> {
        let split = self.sounds.partition_point(|sound| sound.start_frame <= frame);
        self.sounds.drain(..split)
    }

    pub fn drain_all(&mut self) -> std::vec::Drain<'_, ScheduledSound> {
        self.sounds.drain(..)
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

    pub fn shift_output_frames(&mut self, frames: u64) {
        if frames == 0 {
            return;
        }
        for sound in &mut self.sounds {
            sound.start_frame = sound.start_frame.saturating_add(frames);
        }
    }

    /// 述語が `true` を返すスケジュール音だけを保持する。`stop_sound` 等で使う。
    pub fn retain(&mut self, mut keep: impl FnMut(&ScheduledSound) -> bool) {
        self.sounds.retain(|sound| keep(sound));
    }

    /// 指定 sound_id の再生待ち音量を更新する。ループ BGM や preview の
    /// 設定変更を、次に mixer へ渡る前の queue に反映するために使う。
    pub fn set_volume_for_sound(&mut self, id: SoundId, volume: f32) {
        for sound in &mut self.sounds {
            if sound.sound_id == id {
                sound.volume = volume;
            }
        }
    }

    pub fn apply_playback_rate_change(&mut self, change: crate::clock::PlaybackRateChange) {
        for sound in &mut self.sounds {
            sound.start_frame = retime_output_frame(sound.start_frame, change);
        }
        self.sounds.sort_by_key(|sound| (sound.start_frame, sound.sound_id.0));
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
        let index = self.sounds.partition_point(|queued| {
            (queued.start_frame, queued.sound_id.0) <= (sound.start_frame, sound.sound_id.0)
        });
        self.sounds.insert(index, sound);
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
            start_frame,
            sample_offset_frames: 0,
            sound_id: SoundId(sound_id),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }
    }
}
