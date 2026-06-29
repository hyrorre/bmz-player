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

    fn sound(start_frame: u64, sound_id: u32) -> ScheduledSound {
        ScheduledSound {
            start_frame,
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
