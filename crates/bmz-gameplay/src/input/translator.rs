use bmz_audio::clock::AudioClock;
use bmz_core::input::{InputEvent, InputSource};

use crate::session::PlayOffsets;

use super::backend::{DeviceInputEvent, DeviceTimestamp, PhysicalControl};
use super::binding::LaneBinding;

pub struct InputTimingContext<'a> {
    pub audio_clock: &'a AudioClock,
    pub offsets: PlayOffsets,
}

pub trait InputTranslator {
    fn translate(
        &mut self,
        event: DeviceInputEvent,
        ctx: &InputTimingContext<'_>,
    ) -> Option<InputEvent>;
}

#[derive(Debug, Clone)]
pub struct DefaultInputTranslator {
    pub binding: LaneBinding,
}

impl InputTranslator for DefaultInputTranslator {
    fn translate(
        &mut self,
        event: DeviceInputEvent,
        ctx: &InputTimingContext<'_>,
    ) -> Option<InputEvent> {
        let lane = self.binding.resolve(event.device, &event.control)?;
        let time = estimate_audio_time(&event.timestamp, ctx);
        Some(InputEvent { lane, kind: event.kind, time, source: InputSource::Human })
    }
}

fn estimate_audio_time(
    _timestamp: &DeviceTimestamp,
    ctx: &InputTimingContext<'_>,
) -> bmz_core::time::TimeUs {
    let now = ctx.audio_clock.now();
    bmz_core::time::TimeUs(now.0 + ctx.offsets.input_offset_us)
}

pub fn keyboard_control(name: impl Into<String>) -> PhysicalControl {
    PhysicalControl::KeyboardKey(name.into())
}
