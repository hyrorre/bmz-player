//! Optional output-PCM tap. The callback only copies samples into a bounded atomic
//! ring; window/export threads perform FFTs, at most once per 1/60 second of audio.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, fence},
};

const SIZE: usize = 2048;
pub const SPECTRUM_BANDS: usize = 32;

#[derive(Debug)]
pub struct AudioSpectrum {
    enabled: AtomicBool,
    reset_requested: AtomicBool,
    version: AtomicU64,
    frames: AtomicU64,
    rate: AtomicU32,
    pcm: [[AtomicU32; 2]; SIZE],
    analysis: Mutex<Analysis>,
    /// The last analyzed bands, readable without `analysis` for a contended reader.
    published: [AtomicU32; SPECTRUM_BANDS],
}

#[derive(Debug, Default)]
struct Analysis {
    version: u64,
    frames: u64,
    bands: [f32; SPECTRUM_BANDS],
}

impl Default for AudioSpectrum {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            reset_requested: AtomicBool::new(false),
            version: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            rate: AtomicU32::new(0),
            pcm: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            analysis: Mutex::new(Analysis::default()),
            published: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl AudioSpectrum {
    pub fn set_enabled(&self, enabled: bool) {
        if enabled && !self.enabled.load(Ordering::Acquire) {
            self.publish(&[0.0; SPECTRUM_BANDS]);
            self.reset_requested.store(true, Ordering::Release);
        }
        self.enabled.store(enabled, Ordering::Release);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    fn publish(&self, bands: &[f32; SPECTRUM_BANDS]) {
        for (slot, value) in self.published.iter().zip(bands) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    fn published(&self) -> [f32; SPECTRUM_BANDS] {
        std::array::from_fn(|band| f32::from_bits(self.published[band].load(Ordering::Relaxed)))
    }

    /// Single writer: called with exclusive access to the owning AudioEngine.
    pub(crate) fn record(&self, stereo: &[f32], rate: u32) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        self.version.fetch_add(1, Ordering::AcqRel);
        let reset = self.reset_requested.swap(false, Ordering::AcqRel)
            || self.rate.load(Ordering::Relaxed) != rate;
        let start = if reset { 0 } else { self.frames.load(Ordering::Relaxed) };
        let count = stereo.len() / 2;
        for i in count.saturating_sub(SIZE)..count {
            for channel in 0..2 {
                let value = stereo[2 * i + channel];
                self.pcm[(start as usize + i) % SIZE][channel]
                    .store(if value.is_finite() { value.to_bits() } else { 0 }, Ordering::Relaxed);
            }
        }
        self.frames.store(start.saturating_add(count as u64), Ordering::Relaxed);
        self.rate.store(rate, Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn clear(&self) {
        // Contended readers must not show audio from before the reset.
        self.publish(&[0.0; SPECTRUM_BANDS]);
        self.reset_requested.store(true, Ordering::Release);
    }

    /// Never called by the audio callback. Contention/in-flight writes reuse the
    /// preceding display frame instead of making either thread wait for PCM.
    pub fn bands(&self) -> [f32; SPECTRUM_BANDS] {
        if !self.enabled.load(Ordering::Acquire) || self.reset_requested.load(Ordering::Acquire) {
            return [0.0; SPECTRUM_BANDS];
        }
        let Ok(mut cache) = self.analysis.try_lock() else {
            return self.published();
        };
        let version = self.version.load(Ordering::Acquire);
        if !version.is_multiple_of(2) || version == cache.version {
            return cache.bands;
        }
        let frames = self.frames.load(Ordering::Relaxed);
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return [0.0; SPECTRUM_BANDS];
        }
        if frames > cache.frames
            && frames - cache.frames < u64::from(rate / 60)
            && cache.version != 0
        {
            return cache.bands;
        }
        let valid = frames.min(SIZE as u64) as usize;
        let mut pcm = [[0.0; SIZE]; 2];
        for i in 0..valid {
            for (channel, samples) in pcm.iter_mut().enumerate() {
                samples[SIZE - valid + i] = f32::from_bits(
                    self.pcm[(frames as usize - valid + i) % SIZE][channel].load(Ordering::Relaxed),
                );
            }
        }
        fence(Ordering::Acquire);
        if self.version.load(Ordering::Relaxed) != version
            || self.reset_requested.load(Ordering::Acquire)
        {
            return cache.bands;
        }
        cache.bands = analyze(pcm, rate);
        cache.frames = frames;
        cache.version = version;
        self.publish(&cache.bands);
        cache.bands
    }
}

fn analyze(mut pcm: [[f32; SIZE]; 2], rate: u32) -> [f32; SPECTRUM_BANDS] {
    let mut power = [0.0; SIZE / 2];
    for samples in &mut pcm {
        for (i, sample) in samples.iter_mut().enumerate() {
            *sample *= 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (SIZE - 1) as f32).cos();
        }
        let mut imaginary = [0.0; SIZE];
        fft(samples, &mut imaginary);
        for i in 1..SIZE / 2 {
            power[i] += (samples[i] * samples[i] + imaginary[i] * imaginary[i]) * 8.0
                / (SIZE * SIZE) as f32;
        }
    }
    let high = (rate as f32 / 2.0).clamp(41.0, 20_000.0);
    std::array::from_fn(|band| {
        let start = (40.0 * (high / 40.0).powf(band as f32 / SPECTRUM_BANDS as f32) * SIZE as f32
            / rate as f32)
            .floor() as usize;
        let end =
            (40.0 * (high / 40.0).powf((band + 1) as f32 / SPECTRUM_BANDS as f32) * SIZE as f32
                / rate as f32)
                .ceil() as usize;
        let peak = power
            [start.clamp(1, SIZE / 2 - 1)..end.clamp(start.clamp(1, SIZE / 2 - 1) + 1, SIZE / 2)]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        ((10.0 * peak.max(1e-12).log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    })
}

// Fixed-size radix-2 FFT: no dependency, allocation, or work on the audio thread.
fn fft(real: &mut [f32; SIZE], imaginary: &mut [f32; SIZE]) {
    for i in 0..SIZE {
        let j = i.reverse_bits() >> (usize::BITS - SIZE.trailing_zeros());
        if i < j {
            real.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= SIZE {
        let angle = -std::f32::consts::TAU / length as f32;
        let (step_i, step_r) = angle.sin_cos();
        for start in (0..SIZE).step_by(length) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for offset in 0..length / 2 {
                let a = start + offset;
                let b = a + length / 2;
                let tr = wr * real[b] - wi * imaginary[b];
                let ti = wr * imaginary[b] + wi * real[b];
                real[b] = real[a] - tr;
                imaginary[b] = imaginary[a] - ti;
                real[a] += tr;
                imaginary[a] += ti;
                (wr, wi) = (wr * step_r - wi * step_i, wr * step_i + wi * step_r);
            }
        }
        length *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tone(hz: f32, inverted: bool) -> Vec<f32> {
        (0..SIZE)
            .flat_map(|i| {
                let sample = (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin() * 0.5;
                [sample, if inverted { -sample } else { sample }]
            })
            .collect()
    }
    #[test]
    fn spectrum_tracks_frequency_and_preserves_antiphase_stereo() {
        let tap = AudioSpectrum::default();
        tap.set_enabled(true);
        tap.record(&tone(375.0, false), 48_000);
        let low = tap.bands();
        tap.record(&tone(6000.0, true), 48_000);
        let high = tap.bands();
        let peak = |v: [f32; 32]| v.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert!(peak(high) > peak(low) + 8);
        assert!(high[peak(high)] > 0.8);
        assert!(high[peak(low)] < 0.1);
    }
    #[test]
    fn disabled_silent_and_reset_captures_do_not_leak_previous_audio() {
        let tap = AudioSpectrum::default();
        tap.record(&tone(1000.0, false), 48_000);
        assert_eq!(tap.bands(), [0.0; 32]);
        tap.set_enabled(true);
        tap.record(&tone(1000.0, false), 48_000);
        assert!(tap.bands().iter().any(|v| *v > 0.5));
        tap.clear();
        assert_eq!(tap.bands(), [0.0; 32]);
        tap.record(&[0.0; 128], 48_000);
        assert_eq!(tap.bands(), [0.0; 32]);
        tap.record(&vec![f32::NAN; SIZE * 2], 48_000);
        assert_eq!(tap.bands(), [0.0; 32]);
        tap.set_enabled(false);
        assert_eq!(tap.bands(), [0.0; 32]);
    }
    #[test]
    fn contended_reader_reuses_the_last_analysis_without_pre_reset_audio() {
        let tap = AudioSpectrum::default();
        tap.set_enabled(true);
        tap.record(&tone(1000.0, false), 48_000);
        let analyzed = tap.bands();
        assert!(analyzed.iter().any(|v| *v > 0.5));
        {
            // Another reader (window frame or export snapshot) holds the analysis.
            let _held = tap.analysis.lock().unwrap();
            assert_eq!(tap.bands(), analyzed);
            tap.clear();
            tap.record(&tone(1000.0, false), 48_000);
            assert_eq!(tap.bands(), [0.0; 32]);
        }
        assert!(tap.bands().iter().any(|v| *v > 0.5));
    }
}
