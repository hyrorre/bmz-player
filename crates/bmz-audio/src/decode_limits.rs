use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(crate) const MAX_AUDIO_CHANNELS: u16 = 8;
pub(crate) const MAX_AUDIO_SAMPLE_RATE: u32 = 384_000;
pub(crate) const MAX_AUDIO_SECONDS: u64 = 20 * 60;
pub(crate) const MAX_SAMPLE_BYTES: usize = 512 * 1024 * 1024;
pub(crate) const MAX_CHART_SAMPLE_BYTES: usize = 3 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AudioDecodeLimits {
    pub seconds: u64,
    pub bytes: usize,
}

impl Default for AudioDecodeLimits {
    fn default() -> Self {
        Self { seconds: MAX_AUDIO_SECONDS, bytes: MAX_SAMPLE_BYTES }
    }
}

impl AudioDecodeLimits {
    pub fn check_format(self, channels: u16, rate: u32) -> Result<(), String> {
        if channels == 0 || channels > MAX_AUDIO_CHANNELS {
            return Err(format!(
                "audio channel count {channels} exceeds limit {MAX_AUDIO_CHANNELS}"
            ));
        }
        if rate == 0 || rate > MAX_AUDIO_SAMPLE_RATE {
            return Err(format!("audio sample rate {rate} exceeds limit {MAX_AUDIO_SAMPLE_RATE}"));
        }
        Ok(())
    }

    pub fn check_samples(self, channels: u16, rate: u32, samples: usize) -> Result<(), String> {
        self.check_format(channels, rate)?;
        let max_frames = u64::from(rate).saturating_mul(self.seconds);
        if samples as u64 > max_frames.saturating_mul(u64::from(channels)) {
            return Err(format!("audio duration exceeds limit {} seconds", self.seconds));
        }
        if samples > self.bytes / size_of::<f32>() {
            return Err(format!("decoded audio exceeds limit {} bytes", self.bytes));
        }
        Ok(())
    }
}

/// Shared by all workers and fallback decodes belonging to one chart load.
#[derive(Debug)]
pub struct ChartDecodeBudget {
    limit: usize,
    used: AtomicUsize,
    exceeded: AtomicBool,
}

impl ChartDecodeBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self { limit, used: AtomicUsize::new(0), exceeded: AtomicBool::new(false) }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.exceeded.load(Ordering::Acquire) {
            Err(format!("chart decoded audio exceeds limit {} bytes", self.limit))
        } else {
            Ok(())
        }
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit
    }

    fn reserve(&self, bytes: usize) -> Result<(), String> {
        self.check()?;
        if self
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
            .is_err()
        {
            self.exceeded.store(true, Ordering::Release);
            return self.check();
        }
        Ok(())
    }
}

/// Failed/abandoned decodes release their allocation. Completed PCM remains
/// charged until the chart load finishes, including samples served by a cache.
pub(crate) struct DecodeReservation<'a> {
    budget: Option<&'a ChartDecodeBudget>,
    bytes: usize,
    committed: bool,
}

impl<'a> DecodeReservation<'a> {
    pub fn new(budget: Option<&'a ChartDecodeBudget>) -> Self {
        Self { budget, bytes: 0, committed: false }
    }

    pub fn check(&self) -> Result<(), String> {
        self.budget.map_or(Ok(()), ChartDecodeBudget::check)
    }

    pub fn resize(&mut self, bytes: usize) -> Result<(), String> {
        self.check()?;
        if let Some(budget) = self.budget {
            if bytes > self.bytes {
                budget.reserve(bytes - self.bytes)?;
            } else {
                budget.used.fetch_sub(self.bytes - bytes, Ordering::AcqRel);
            }
        }
        self.bytes = bytes;
        self.check()
    }

    pub fn reserve_samples(
        &mut self,
        frames: &mut Vec<f32>,
        required: usize,
        limits: AudioDecodeLimits,
    ) -> Result<(), String> {
        self.check()?;
        if required <= frames.capacity() {
            return Ok(());
        }
        let capacity =
            required.max(frames.capacity().saturating_mul(2)).min(limits.bytes / size_of::<f32>());
        if required > capacity {
            return Err(format!("decoded audio exceeds limit {} bytes", limits.bytes));
        }
        self.resize(capacity * size_of::<f32>())?;
        frames.try_reserve_exact(capacity - frames.len()).map_err(|error| error.to_string())?;
        self.resize(frames.capacity() * size_of::<f32>())
    }

    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for DecodeReservation<'_> {
    fn drop(&mut self) {
        if !self.committed
            && let Some(budget) = self.budget
        {
            budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_and_byte_boundaries_are_checked_without_allocation() {
        let limits = AudioDecodeLimits::default();
        limits.check_format(8, 384_000).unwrap();
        assert!(limits.check_format(9, 48_000).is_err());
        assert!(limits.check_format(2, 384_001).is_err());
        assert!(limits.check_format(0, 48_000).is_err());
        assert!(limits.check_format(2, 0).is_err());
        let duration = AudioDecodeLimits { seconds: 1, bytes: 1 << 20 };
        duration.check_samples(2, 48_000, 96_000).unwrap();
        assert!(duration.check_samples(2, 48_000, 96_001).is_err());
        let bytes = AudioDecodeLimits { bytes: 16, ..limits };
        bytes.check_samples(1, 48_000, 4).unwrap();
        assert!(bytes.check_samples(1, 48_000, 5).is_err());
    }

    #[test]
    fn abandoned_reservations_release_bytes_and_overflow_cancels_the_budget() {
        let budget = ChartDecodeBudget::new(16);
        {
            let mut reservation = DecodeReservation::new(Some(&budget));
            reservation.resize(16).unwrap();
        }
        assert_eq!(budget.used.load(Ordering::Relaxed), 0);
        let mut retained = DecodeReservation::new(Some(&budget));
        retained.resize(16).unwrap();
        retained.commit();
        let mut failed = DecodeReservation::new(Some(&budget));
        assert!(failed.resize(1).is_err());
        assert!(budget.check().is_err());
        assert_eq!(budget.used.load(Ordering::Relaxed), 16);
    }
}
