use bmz_chart::model::PlayableChart;
use bmz_chart::volume::{chart_channel_volume_factor, chart_volume_at_time};
use bmz_core::ids::SoundId;
use bmz_core::time::TimeUs;

use crate::sample::{DecodedSample, SampleBank};

/// プレイ再生に適用する正規化の目標 loudness。
/// DBには解析指標を保存し、適用時に用途別の目標へ変換する。
pub const PLAY_TARGET_LUFS: f32 = -6.0;
/// 選曲プレビューに適用する正規化の目標 loudness。
/// プレイ再生と同じ目標値を使う。
pub const PREVIEW_TARGET_LUFS: f32 = PLAY_TARGET_LUFS;
/// 選曲プレビューの sample peak 上限。true peak ではなく decode 済み PCM の最大値で判定する。
pub const PREVIEW_PEAK_CEILING_DBFS: f32 = -1.0;
/// プレイ / システム BGM は全体目標よりこの値だけ大きい短時間区間を許容する。
pub const LONG_FORM_SHORT_TERM_HEADROOM_LU: f32 = 3.0;
/// プレイ / システム BGM の sample peak 上限。
pub const LONG_FORM_PEAK_CEILING_DBFS: f32 = -1.0;
const MAX_ANALYSIS_DURATION_US: i64 = 10 * 60 * 1_000_000;
const ANALYSIS_CHUNK_FRAMES: usize = 2048;
const ENERGY_BLOCK_DURATION_MS: u64 = 100;
const INTEGRATED_WINDOW_BLOCKS: usize = 4;
const SHORT_TERM_WINDOW_BLOCKS: usize = 30;
const ABSOLUTE_GATE_LUFS: f32 = -70.0;
const RELATIVE_GATE_LU: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoudnessAnalysis {
    pub loudness_lufs: f32,
    pub short_term_lufs: f32,
    pub peak_abs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewLoudnessAnalysis {
    pub loudness_lufs: f32,
    pub short_term_lufs: f32,
    pub peak_abs: f32,
    pub normalization_gain: f32,
}

#[derive(Debug, Clone, Copy)]
struct EnergyBlock {
    sum_square: f64,
    frames: u64,
}

struct LoudnessAccumulator {
    frames_per_block: u64,
    current_sum_square: f64,
    current_frames: u64,
    blocks: Vec<EnergyBlock>,
    peak_abs: f32,
    valid: bool,
}

impl LoudnessAccumulator {
    fn new(sample_rate: u32) -> Option<Self> {
        if sample_rate == 0 {
            return None;
        }
        let frames_per_block = (u64::from(sample_rate) * ENERGY_BLOCK_DURATION_MS / 1_000).max(1);
        Some(Self {
            frames_per_block,
            current_sum_square: 0.0,
            current_frames: 0,
            blocks: Vec::new(),
            peak_abs: 0.0,
            valid: true,
        })
    }

    fn push_stereo(&mut self, left: f32, right: f32) {
        if !left.is_finite() || !right.is_finite() {
            self.valid = false;
            return;
        }
        // Source PCM is sanitized before summing. Keep the absolute maximum of
        // the mix: legitimate simultaneous voices must still trigger protection.
        self.peak_abs = self.peak_abs.max(left.abs()).max(right.abs());
        let left = f64::from(left);
        let right = f64::from(right);
        self.current_sum_square += left * left + right * right;
        self.current_frames += 1;
        if self.current_frames >= self.frames_per_block {
            self.finish_block();
        }
    }

    fn push_interleaved_stereo(&mut self, frames: &[f32]) {
        debug_assert_eq!(frames.len() % 2, 0);
        for frame in frames.as_chunks::<2>().0 {
            self.push_stereo(frame[0], frame[1]);
        }
    }

    fn finish(mut self) -> Option<LoudnessAnalysis> {
        if self.current_frames > 0 {
            self.finish_block();
        }
        if !self.valid || self.peak_abs <= 0.0 || self.blocks.is_empty() {
            return None;
        }

        let integrated_windows = window_mean_squares(&self.blocks, INTEGRATED_WINDOW_BLOCKS);
        let absolute_gated = integrated_windows
            .iter()
            .copied()
            .filter(|mean_square| {
                loudness_lufs(*mean_square).is_some_and(|v| v >= ABSOLUTE_GATE_LUFS)
            })
            .collect::<Vec<_>>();
        if absolute_gated.is_empty() {
            return None;
        }
        let preliminary_mean = mean(&absolute_gated)?;
        let relative_gate = loudness_lufs(preliminary_mean)? - RELATIVE_GATE_LU;
        let gated = absolute_gated
            .iter()
            .copied()
            .filter(|mean_square| loudness_lufs(*mean_square).is_some_and(|v| v >= relative_gate))
            .collect::<Vec<_>>();
        let integrated_lufs = loudness_lufs(mean(&gated)?)?;

        let short_term_lufs = window_mean_squares(&self.blocks, SHORT_TERM_WINDOW_BLOCKS)
            .into_iter()
            .filter_map(loudness_lufs)
            .fold(f32::NEG_INFINITY, f32::max);
        if !short_term_lufs.is_finite() {
            return None;
        }

        Some(LoudnessAnalysis {
            loudness_lufs: integrated_lufs,
            short_term_lufs,
            peak_abs: self.peak_abs,
        })
    }

    fn finish_block(&mut self) {
        self.blocks
            .push(EnergyBlock { sum_square: self.current_sum_square, frames: self.current_frames });
        self.current_sum_square = 0.0;
        self.current_frames = 0;
    }
}

fn window_mean_squares(blocks: &[EnergyBlock], window_blocks: usize) -> Vec<f64> {
    if blocks.is_empty() {
        return Vec::new();
    }
    let window_blocks = window_blocks.min(blocks.len()).max(1);
    blocks
        .windows(window_blocks)
        .filter_map(|window| {
            let sum_square = window.iter().map(|block| block.sum_square).sum::<f64>();
            let frames = window.iter().map(|block| block.frames).sum::<u64>();
            (frames > 0).then_some(sum_square / frames as f64)
        })
        .collect()
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn loudness_lufs(mean_square: f64) -> Option<f32> {
    if mean_square <= f64::MIN_POSITIVE {
        return None;
    }
    let value = (-0.691 + 10.0 * mean_square.log10()) as f32;
    value.is_finite().then_some(value)
}

#[derive(Debug, Clone, Copy)]
struct LoudnessEvent {
    start_frame: u64,
    sound_id: SoundId,
    volume: f32,
}

#[derive(Debug, Clone, Copy)]
struct ActiveLoudnessVoice {
    next_output_frame: u64,
    sound_id: SoundId,
    sample_frame: usize,
    volume: f32,
}

pub fn analyze_chart_loudness(
    chart: &PlayableChart,
    samples: &SampleBank,
    sample_rate: u32,
) -> Option<LoudnessAnalysis> {
    let mut loudness = LoudnessAccumulator::new(sample_rate)?;

    let mut events = collect_loudness_events(chart, sample_rate);
    if events.is_empty() {
        return None;
    }
    events.sort_by_key(|event| (event.start_frame, event.sound_id.0));

    let duration_frames = analysis_duration_frames(chart, samples, sample_rate, &events);
    if duration_frames == 0 {
        return None;
    }

    let mut active = Vec::<ActiveLoudnessVoice>::new();
    let mut mixed_chunk = vec![0.0f32; ANALYSIS_CHUNK_FRAMES * 2];
    let mut next_event = 0usize;
    let mut output_frame = 0u64;

    while output_frame < duration_frames {
        let chunk_frames = ((duration_frames - output_frame) as usize).min(ANALYSIS_CHUNK_FRAMES);
        let chunk_end_frame = output_frame + chunk_frames as u64;
        mixed_chunk[..chunk_frames * 2].fill(0.0);

        while next_event < events.len() && events[next_event].start_frame < chunk_end_frame {
            let event = events[next_event];
            if samples.get(event.sound_id).is_some() && event.volume > 0.0 {
                active.push(ActiveLoudnessVoice {
                    next_output_frame: event.start_frame,
                    sound_id: event.sound_id,
                    sample_frame: 0,
                    volume: event.volume,
                });
            }
            next_event += 1;
        }

        active.retain_mut(|voice| {
            let Some(sample) = samples.get(voice.sound_id) else {
                return false;
            };
            if voice.sample_frame >= sample.frame_count() {
                return false;
            }
            let start_offset = voice.next_output_frame.saturating_sub(output_frame) as usize;
            if start_offset >= chunk_frames {
                return true;
            }
            let mixed_frames = sample.mix_stereo_into(
                voice.sample_frame,
                voice.volume,
                &mut mixed_chunk[start_offset * 2..chunk_frames * 2],
            );
            voice.sample_frame += mixed_frames;
            voice.next_output_frame += mixed_frames as u64;
            voice.sample_frame < sample.frame_count()
        });

        loudness.push_interleaved_stereo(&mixed_chunk[..chunk_frames * 2]);
        output_frame = chunk_end_frame;
    }
    loudness.finish()
}

/// Decode 済み音源を、プレイの合成音声と同じゲーティング規則で解析する。
pub fn analyze_decoded_loudness(sample: &DecodedSample) -> Option<LoudnessAnalysis> {
    if sample.frame_count() == 0 {
        return None;
    }
    let mut loudness = LoudnessAccumulator::new(sample.sample_rate)?;
    for frame in 0..sample.frame_count() {
        let (left, right) = sample.sample_stereo(frame);
        loudness.push_stereo(left, right);
    }
    loudness.finish()
}

/// Decode 済みの選曲プレビューを解析し、loudness と sample peak の両方を満たす
/// 下げ方向のみのゲインを返す。
pub fn analyze_preview_loudness(sample: &DecodedSample) -> Option<PreviewLoudnessAnalysis> {
    let analysis = analyze_decoded_loudness(sample)?;
    let normalization_gain = normalization_gain_for_analysis(
        analysis,
        PREVIEW_TARGET_LUFS,
        PREVIEW_TARGET_LUFS,
        PREVIEW_PEAK_CEILING_DBFS,
        1.0,
    );

    Some(PreviewLoudnessAnalysis {
        loudness_lufs: analysis.loudness_lufs,
        short_term_lufs: analysis.short_term_lufs,
        peak_abs: analysis.peak_abs,
        normalization_gain,
    })
}

/// 全体 loudness だけがある場合の互換用: `PLAY_TARGET_LUFS` (-6) 基準の下げのみゲイン。
/// 通常のプレイ経路では短時間 loudness と sample peak も使う
/// [`play_normalization_gain_for_analysis`] を使用する。
/// DB の `loudness_lufs` から毎回導出する。
pub fn play_normalization_gain_for_loudness(loudness_lufs: f32) -> f32 {
    normalization_gain_for_target(loudness_lufs, PLAY_TARGET_LUFS)
}

pub fn play_normalization_gain_for_analysis(analysis: LoudnessAnalysis) -> f32 {
    play_normalization_gain_for_analysis_with_output_gain(analysis, 1.0)
}

/// プレイ側の master / key / BGM volume が後段で掛かることを考慮して、
/// 最終出力の sample peak が上限を超えない範囲のゲインを返す。
///
/// `output_gain` は正規化ゲイン以外の実効出力倍率。プレイ側では
/// `master_volume * max(key_volume, bgm_volume)` を渡す。
pub fn play_normalization_gain_for_analysis_with_output_gain(
    analysis: LoudnessAnalysis,
    output_gain: f32,
) -> f32 {
    normalization_gain_for_analysis(
        analysis,
        PLAY_TARGET_LUFS,
        PLAY_TARGET_LUFS + LONG_FORM_SHORT_TERM_HEADROOM_LU,
        LONG_FORM_PEAK_CEILING_DBFS,
        output_gain,
    )
}

pub fn system_bgm_normalization_gain_for_analysis(analysis: LoudnessAnalysis) -> f32 {
    play_normalization_gain_for_analysis(analysis)
}

fn normalization_gain_for_analysis(
    analysis: LoudnessAnalysis,
    integrated_target_lufs: f32,
    short_term_target_lufs: f32,
    peak_ceiling_dbfs: f32,
    output_gain: f32,
) -> f32 {
    let integrated_gain =
        normalization_gain_for_target(analysis.loudness_lufs, integrated_target_lufs);
    let short_term_gain =
        normalization_gain_for_target(analysis.short_term_lufs, short_term_target_lufs);
    let peak_ceiling = 10.0f32.powf(peak_ceiling_dbfs / 20.0);
    let output_gain = if output_gain.is_finite() && output_gain >= 0.0 { output_gain } else { 1.0 };
    let peak_gain = if output_gain <= f32::MIN_POSITIVE {
        1.0
    } else {
        (peak_ceiling / (analysis.peak_abs * output_gain)).clamp(0.0, 1.0)
    };
    integrated_gain.min(short_term_gain).min(peak_gain).clamp(0.0, 1.0)
}

fn normalization_gain_for_target(loudness_lufs: f32, target_lufs: f32) -> f32 {
    if !loudness_lufs.is_finite() {
        return 1.0;
    }
    10.0f32.powf((target_lufs - loudness_lufs) / 20.0).clamp(0.0, 1.0)
}

fn collect_loudness_events(chart: &PlayableChart, sample_rate: u32) -> Vec<LoudnessEvent> {
    let mut events = Vec::new();
    for event in &chart.bgm_events {
        let volume =
            chart_channel_volume_factor(chart_volume_at_time(&chart.bgm_volume_events, event.time));
        events.push(LoudnessEvent {
            start_frame: time_to_frame(event.time, sample_rate),
            sound_id: event.sound,
            volume,
        });
    }

    for lane_notes in &chart.lane_notes {
        for note in lane_notes {
            for sound_id in note.sounds() {
                let volume = chart_channel_volume_factor(chart_volume_at_time(
                    &chart.key_volume_events,
                    note.time,
                ));
                events.push(LoudnessEvent {
                    start_frame: time_to_frame(note.time, sample_rate),
                    sound_id,
                    volume,
                });
            }
        }
    }

    for pair in &chart.long_notes {
        if let Some(sound_id) = pair.sound {
            let volume = chart_channel_volume_factor(chart_volume_at_time(
                &chart.key_volume_events,
                pair.start_time,
            ));
            events.push(LoudnessEvent {
                start_frame: time_to_frame(pair.start_time, sample_rate),
                sound_id,
                volume,
            });
        }
    }
    events
}

fn analysis_duration_frames(
    chart: &PlayableChart,
    samples: &SampleBank,
    sample_rate: u32,
    events: &[LoudnessEvent],
) -> u64 {
    let chart_end = time_to_frame(chart.end_time, sample_rate);
    let sample_end = events
        .iter()
        .filter_map(|event| {
            let sample = samples.get(event.sound_id)?;
            Some(event.start_frame.saturating_add(sample.frame_count() as u64))
        })
        .max()
        .unwrap_or(0);
    let max_duration = (MAX_ANALYSIS_DURATION_US as u128 * sample_rate as u128 / 1_000_000) as u64;
    chart_end.max(sample_end).min(max_duration)
}

fn time_to_frame(time: TimeUs, sample_rate: u32) -> u64 {
    (time.0.max(0) as u128 * sample_rate as u128 / 1_000_000) as u64
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use bmz_chart::model::{
        BarLine, BgaAssetRef, ChartMetadata, NoteEvent, NoteKind, PlayableChart, SoundAssetRef,
        SoundEvent,
    };
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::sample::{DecodedSample, SampleBank};

    #[test]
    fn play_normalization_gain_uses_minus_six_target() {
        let at_target = play_normalization_gain_for_loudness(PLAY_TARGET_LUFS);
        assert!((at_target - 1.0).abs() < 0.001);

        let quieter = play_normalization_gain_for_loudness(-12.0);
        assert!((quieter - 1.0).abs() < 0.001);

        let louder = play_normalization_gain_for_loudness(0.0);
        assert!((louder - 10.0f32.powf(-6.0 / 20.0)).abs() < 0.001);
        assert!(louder < 1.0);

        let play_gain = play_normalization_gain_for_loudness(-6.0);
        assert!((play_gain - 1.0).abs() < 0.001);
    }

    #[test]
    fn play_peak_guard_accounts_for_downstream_output_gain() {
        let analysis =
            LoudnessAnalysis { loudness_lufs: -20.0, short_term_lufs: -20.0, peak_abs: 4.0 };
        let peak_ceiling = 10.0f32.powf(LONG_FORM_PEAK_CEILING_DBFS / 20.0);

        let full_scale = play_normalization_gain_for_analysis(analysis);
        let quarter_scale = play_normalization_gain_for_analysis_with_output_gain(analysis, 0.25);

        assert!((analysis.peak_abs * full_scale - peak_ceiling).abs() < 0.001);
        assert!((analysis.peak_abs * quarter_scale * 0.25 - peak_ceiling).abs() < 0.001);
        assert!(quarter_scale > full_scale);
        assert_eq!(play_normalization_gain_for_analysis_with_output_gain(analysis, 0.0), 1.0);
        assert_eq!(
            play_normalization_gain_for_analysis_with_output_gain(analysis, f32::NAN),
            full_scale
        );
        assert_eq!(system_bgm_normalization_gain_for_analysis(analysis), full_scale);
    }

    #[test]
    fn preview_normalization_uses_same_minus_six_target_as_play() {
        assert_eq!(PREVIEW_TARGET_LUFS, PLAY_TARGET_LUFS);

        let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.5; 200] };
        let analysis = analyze_preview_loudness(&sample).unwrap();
        let expected_gain = play_normalization_gain_for_loudness(analysis.loudness_lufs);

        assert!((analysis.normalization_gain - expected_gain).abs() < 0.001);
    }

    #[test]
    fn preview_analysis_keeps_audio_below_target_at_unity() {
        let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.1; 200] };

        let result = analyze_preview_loudness(&sample).unwrap();

        assert!(result.loudness_lufs < PREVIEW_TARGET_LUFS);
        assert!((result.normalization_gain - 1.0).abs() < 0.001);
    }

    #[test]
    fn preview_analysis_attenuates_loud_audio() {
        let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.5; 200] };

        let result = analyze_preview_loudness(&sample).unwrap();
        let normalized_loudness = result.loudness_lufs + 20.0 * result.normalization_gain.log10();

        assert!(result.normalization_gain < 1.0);
        assert!((normalized_loudness - PREVIEW_TARGET_LUFS).abs() < 0.001);
    }

    #[test]
    fn preview_analysis_limits_isolated_peak_to_ceiling() {
        let mut frames = vec![0.01; 200];
        frames[100] = 1.0;
        let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames };

        let result = analyze_preview_loudness(&sample).unwrap();
        let peak_ceiling = 10.0f32.powf(PREVIEW_PEAK_CEILING_DBFS / 20.0);

        assert!(result.loudness_lufs < PREVIEW_TARGET_LUFS);
        assert!(result.normalization_gain < 1.0);
        assert!((result.peak_abs * result.normalization_gain - peak_ceiling).abs() < 0.001);
    }

    #[test]
    fn preview_analysis_rejects_silence_and_repairs_non_finite_audio() {
        let empty = DecodedSample { channels: 2, sample_rate: 48_000, frames: Vec::new() };
        let silent = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.0; 200] };
        let nan = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.1, f32::NAN] };
        let infinite =
            DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.1, f32::INFINITY] };

        assert_eq!(analyze_preview_loudness(&empty), None);
        assert_eq!(analyze_preview_loudness(&silent), None);
        assert_eq!(analyze_preview_loudness(&nan).unwrap().peak_abs, 0.1);
        assert_eq!(analyze_preview_loudness(&infinite).unwrap().peak_abs, 0.1);
    }

    #[test]
    fn corrupt_pcm_does_not_attenuate_the_entire_chart() {
        for channels in [1, 2, 3] {
            for offset in [0, 500] {
                let mut frames = vec![0.2; 1_000 * channels as usize];
                frames[offset..offset + 4].copy_from_slice(&[
                    6405.997,
                    f32::NAN,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                ]);
                let sample = DecodedSample { channels, sample_rate: 1_000, frames };
                let direct = analyze_decoded_loudness(&sample).unwrap();
                assert_eq!(direct.peak_abs, 0.2);
                let mut bank = SampleBank::default();
                bank.insert(SoundId(1), sample);
                let mut chart = chart();
                chart.bgm_events.push(SoundEvent {
                    tick: ChartTick(0),
                    time: TimeUs(0),
                    sound: SoundId(1),
                });
                let analysis = analyze_chart_loudness(&chart, &bank, 1_000).unwrap();
                assert_eq!(analysis.peak_abs, 0.2);
                // This quiet fixture needs no attenuation, including at full output gain.
                assert_eq!(play_normalization_gain_for_analysis(analysis), 1.0);
                let mut engine = crate::engine::AudioEngine::new(1_000);
                engine.samples = bank;
                engine.schedule_all([crate::queue::ScheduledSound::one_shot(
                    0,
                    SoundId(1),
                    1.0,
                    0.0,
                )]);
                let mut output = vec![0.0; 2_000];
                engine.render_stereo(0, &mut output);
                assert!(output.iter().all(|v| v.is_finite() && v.abs() <= 0.2));
            }
        }
    }

    #[test]
    fn normal_overshoot_and_layered_peaks_remain_protected() {
        let mut sample =
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![0.1; 1_000] };
        sample.frames[..3].copy_from_slice(&[1.1, -1.2, 1.36]);
        let analysis = analyze_decoded_loudness(&sample).unwrap();
        assert_eq!(analysis.peak_abs, 1.36);
        let ceiling = 10.0f32.powf(-1.0 / 20.0);
        assert!((play_normalization_gain_for_analysis(analysis) - ceiling / 1.36).abs() < 1e-6);
        let mut bank = SampleBank::default();
        bank.insert(SoundId(1), sample);
        let mut chart = chart();
        for _ in 0..10 {
            chart.bgm_events.push(SoundEvent {
                tick: ChartTick(0),
                time: TimeUs(0),
                sound: SoundId(1),
            });
        }
        let analysis = analyze_chart_loudness(&chart, &bank, 1_000).unwrap();
        assert!((analysis.peak_abs - 13.6).abs() < 1e-5);
        assert!(
            (play_normalization_gain_for_analysis(analysis) * analysis.peak_abs - ceiling).abs()
                < 1e-5
        );
    }

    #[test]
    fn gated_analysis_ignores_a_long_silent_tail() {
        let sample_rate = 1_000;
        let loud = stereo_frames(0.75, 3 * sample_rate as usize);
        let mut with_tail = loud.clone();
        with_tail.extend(stereo_frames(0.0, 15 * sample_rate as usize));

        let loud =
            analyze_decoded_loudness(&DecodedSample { channels: 2, sample_rate, frames: loud })
                .unwrap();
        let with_tail = analyze_decoded_loudness(&DecodedSample {
            channels: 2,
            sample_rate,
            frames: with_tail,
        })
        .unwrap();

        assert!((loud.loudness_lufs - with_tail.loudness_lufs).abs() < 0.5);
        assert!((loud.short_term_lufs - with_tail.short_term_lufs).abs() < 0.001);
    }

    #[test]
    fn preview_short_term_guard_is_stricter_than_long_form_audio() {
        let sample_rate = 1_000;
        let mut frames = stereo_frames(0.5, 3 * sample_rate as usize);
        frames.extend(stereo_frames(0.2, 30 * sample_rate as usize));
        let sample = DecodedSample { channels: 2, sample_rate, frames };

        let preview = analyze_preview_loudness(&sample).unwrap();
        let long_form = analyze_decoded_loudness(&sample).unwrap();
        let play_gain = play_normalization_gain_for_analysis(long_form);

        assert!(preview.loudness_lufs < PREVIEW_TARGET_LUFS);
        assert!(preview.short_term_lufs > PREVIEW_TARGET_LUFS);
        assert!(preview.normalization_gain < 1.0);
        assert!((play_gain - 1.0).abs() < 0.001);
        assert_eq!(system_bgm_normalization_gain_for_analysis(long_form), play_gain);
    }

    #[test]
    fn analyze_chart_loudness_uses_bgm_and_key_events() {
        let mut samples = SampleBank::default();
        samples.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 48] },
        );
        samples.insert(
            SoundId(2),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 48] },
        );

        let mut chart = chart();
        chart.bgm_events.push(SoundEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: SoundId(1),
        });
        chart.lane_notes[0].push(NoteEvent {
            id: NoteId(1),
            lane: bmz_core::lane::Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: Some(SoundId(2)),
            layered_sounds: Vec::new(),
            damage: None,
        });
        chart.end_time = TimeUs(1_000);

        let result = analyze_chart_loudness(&chart, &samples, 48_000).unwrap();
        assert!(result.loudness_lufs.is_finite());
        let gain = play_normalization_gain_for_analysis(result);
        assert!(gain > 0.0);
        assert!(gain <= 1.0);
    }

    #[test]
    fn chunked_chart_analysis_matches_frame_by_frame_reference() {
        let mut samples = SampleBank::default();
        samples.insert(
            SoundId(1),
            DecodedSample {
                channels: 2,
                sample_rate: 48_000,
                frames: (0..10_000).map(|index| (index % 97) as f32 / 97.0 - 0.5).collect(),
            },
        );
        samples.insert(
            SoundId(2),
            DecodedSample {
                channels: 1,
                sample_rate: 48_000,
                frames: (0..3_000).map(|index| (index % 53) as f32 / 106.0).collect(),
            },
        );
        samples.insert(
            SoundId(3),
            DecodedSample {
                channels: 3,
                sample_rate: 48_000,
                frames: (0..9_000).map(|index| (index % 71) as f32 / 142.0 - 0.25).collect(),
            },
        );

        let mut chart = chart();
        chart.bgm_events.extend([
            SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(1) },
            SoundEvent { tick: ChartTick(1), time: TimeUs(42_667), sound: SoundId(3) },
        ]);
        chart.lane_notes[0].push(NoteEvent {
            id: NoteId(1),
            lane: bmz_core::lane::Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(20_000),
            sound: Some(SoundId(2)),
            layered_sounds: Vec::new(),
            damage: None,
        });
        chart.end_time = TimeUs(150_000);

        let chunked = analyze_chart_loudness(&chart, &samples, 48_000).unwrap();
        let reference = analyze_chart_loudness_frame_reference(&chart, &samples, 48_000).unwrap();

        assert!((chunked.loudness_lufs - reference.loudness_lufs).abs() < 0.000_001);
        assert!((chunked.short_term_lufs - reference.short_term_lufs).abs() < 0.000_001);
        assert!((chunked.peak_abs - reference.peak_abs).abs() < 0.000_001);
        assert!(
            (play_normalization_gain_for_analysis(chunked)
                - play_normalization_gain_for_analysis(reference))
            .abs()
                < 0.000_001
        );
    }

    fn analyze_chart_loudness_frame_reference(
        chart: &PlayableChart,
        samples: &SampleBank,
        sample_rate: u32,
    ) -> Option<LoudnessAnalysis> {
        #[derive(Clone, Copy)]
        struct ReferenceVoice {
            sound_id: SoundId,
            sample_frame: usize,
            volume: f32,
        }

        let mut loudness = LoudnessAccumulator::new(sample_rate)?;
        let mut events = collect_loudness_events(chart, sample_rate);
        if events.is_empty() {
            return None;
        }
        events.sort_by_key(|event| (event.start_frame, event.sound_id.0));
        let duration_frames = analysis_duration_frames(chart, samples, sample_rate, &events);
        let mut active = Vec::<ReferenceVoice>::new();
        let mut next_event = 0usize;

        for absolute_frame in 0..duration_frames {
            while next_event < events.len() && events[next_event].start_frame <= absolute_frame {
                let event = events[next_event];
                if samples.get(event.sound_id).is_some() && event.volume > 0.0 {
                    active.push(ReferenceVoice {
                        sound_id: event.sound_id,
                        sample_frame: 0,
                        volume: event.volume,
                    });
                }
                next_event += 1;
            }

            let mut left = 0.0f32;
            let mut right = 0.0f32;
            active.retain_mut(|voice| {
                let Some(sample) = samples.get(voice.sound_id) else {
                    return false;
                };
                if voice.sample_frame >= sample.frame_count() {
                    return false;
                }
                let (sample_left, sample_right) = sample.sample_stereo(voice.sample_frame);
                left += sample_left * voice.volume;
                right += sample_right * voice.volume;
                voice.sample_frame += 1;
                voice.sample_frame < sample.frame_count()
            });
            loudness.push_stereo(left, right);
        }
        loudness.finish()
    }

    fn stereo_frames(value: f32, frame_count: usize) -> Vec<f32> {
        std::iter::repeat_n([value, value], frame_count).flatten().collect()
    }

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata {
                source_format: Default::default(),
                title: String::new(),
                subtitle: String::new(),
                artist: String::new(),
                subartist: String::new(),
                genre: String::new(),
                difficulty_name: String::new(),
                judge_rank: None,
                judge_rank_spec: None,
                play_level: String::new(),
                initial_bpm: 120.0,
                total: None,
                stage_file: String::new(),
                banner_file: String::new(),
                backbmp_file: String::new(),
                preview_file: String::new(),
                volwav_percent: 100,
                has_bga: false,
                has_bms_random: false,
                source_url: String::new(),
                append_url: String::new(),
                bms_headers: BTreeMap::new(),
                key_mode: bmz_core::lane::KeyMode::K7,
                long_note_mode: bmz_chart::model::LongNoteMode::Ln,
                long_note_mode_defined: false,
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::<BarLine>::new(),
            sounds: vec![
                SoundAssetRef { id: SoundId(1), path: "bgm.wav".into(), slice: None },
                SoundAssetRef { id: SoundId(2), path: "key.wav".into(), slice: None },
            ],
            bga_assets: Vec::<BgaAssetRef>::new(),
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }
}
