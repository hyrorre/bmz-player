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
