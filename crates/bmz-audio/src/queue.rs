use bmz_core::ids::SoundId;

#[derive(Debug, Clone, Copy)]
pub struct ScheduledSound {
    pub start_frame: u64,
    pub sound_id: SoundId,
    pub volume: f32,
    pub pan: f32,
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

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
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
        ScheduledSound { start_frame, sound_id: SoundId(sound_id), volume: 1.0, pan: 0.0 }
    }
}
