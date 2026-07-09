use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use bmz_audio::engine::AudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::{LoadedSampleStatus, SampleLoader, load_chart_samples};
use bmz_audio::queue::{RestartPolicy, ScheduledSound};
use bmz_audio::sample::DecodedSample;
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::{NoteKind, PlayableChart};
use bmz_chart::sound_asset::sound_asset_candidates;
use bmz_chart::volume::{chart_channel_volume_factor, chart_volume_at_time};

use crate::storage::library_db::{ChartDistributionSecond, LibraryDatabase};

pub const GENERATED_PREVIEW_VERSION: u32 = 2;
pub const GENERATED_PREVIEW_DURATION_MS: i64 = 18_000;

const GENERATED_PREVIEW_KEY_PREFIX: &str = "generated-preview";
const GENERATED_PREVIEW_DENSITY_WINDOW_SECONDS: usize = 8;
const GENERATED_PREVIEW_LEAD_SECONDS: usize = 2;
const GENERATED_PREVIEW_PREROLL_MS: i64 = 2_000;
const GENERATED_PREVIEW_FADE_IN_MS: i64 = 500;
const GENERATED_PREVIEW_FADE_OUT_MS: i64 = 1_000;
const GENERATED_PREVIEW_BGM_LOOKBACK_EVENTS: usize = 8;
const GENERATED_PREVIEW_BGM_EARLY_GRACE_MS: i64 = 2_000;
const GENERATED_PREVIEW_BGM_DURATION_PROBE_CANDIDATES: usize = 8;
const RENDER_CHUNK_FRAMES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedPreviewKey {
    pub chart_id: i64,
    pub start_ms: i64,
}

pub fn generated_preview_cache_key(chart_id: i64, start_ms: i64) -> String {
    format!("{GENERATED_PREVIEW_KEY_PREFIX}|{GENERATED_PREVIEW_VERSION}|{chart_id}|{start_ms}")
}

pub fn parse_generated_preview_cache_key(key: &str) -> Option<GeneratedPreviewKey> {
    let mut parts = key.split('|');
    let prefix = parts.next()?;
    if prefix != GENERATED_PREVIEW_KEY_PREFIX {
        return None;
    }
    let version = parts.next()?.parse::<u32>().ok()?;
    if version != GENERATED_PREVIEW_VERSION {
        return None;
    }
    let chart_id = parts.next()?.parse::<i64>().ok()?;
    let start_ms = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || chart_id <= 0 || start_ms < 0 {
        return None;
    }
    Some(GeneratedPreviewKey { chart_id, start_ms })
}

pub fn fallback_preview_start_ms(
    distribution: &[ChartDistributionSecond],
    length_ms: i64,
) -> Option<i64> {
    let length_seconds = seconds_from_ms(length_ms);
    if distribution.is_empty() && length_seconds == 0 {
        return None;
    }

    if distribution.is_empty() {
        return Some(fallback_start_second(length_seconds) as i64 * 1_000);
    }

    let window = GENERATED_PREVIEW_DENSITY_WINDOW_SECONDS.min(distribution.len()).max(1);
    let latest_distribution_start = distribution.len().saturating_sub(window);
    let first_search_start = (distribution.len() * 25 / 100).min(latest_distribution_start);
    let last_search_start = (distribution.len() * 80 / 100).min(latest_distribution_start);

    let mut best_start = first_search_start;
    let mut best_density = 0.0_f32;
    let target_center = distribution.len() as f32 * 0.55;

    for start in first_search_start..=last_search_start {
        let end = start + window;
        let density = distribution[start..end].iter().map(weighted_distribution_notes).sum::<f32>();
        let center = start as f32 + window as f32 * 0.5;
        let center_penalty = (center - target_center).abs() * 0.001;
        let score = density - center_penalty;
        let best_center = best_start as f32 + window as f32 * 0.5;
        let best_score = best_density - (best_center - target_center).abs() * 0.001;
        if score > best_score {
            best_start = start;
            best_density = density;
        }
    }

    let selected_start = if best_density > 0.0 {
        best_start
    } else {
        fallback_start_second(length_seconds.max(distribution.len()))
    };
    Some(selected_start.saturating_sub(GENERATED_PREVIEW_LEAD_SECONDS) as i64 * 1_000)
}

pub fn render_generated_preview_for_chart(
    library_db_path: &Path,
    chart_id: i64,
    start_ms: i64,
    sample_rate: u32,
) -> Result<DecodedSample> {
    let db = LibraryDatabase::open(library_db_path)
        .with_context(|| format!("open library db {}", library_db_path.display()))?;
    let chart_path = db
        .primary_chart_file_path(chart_id)?
        .with_context(|| format!("chart {chart_id} has no primary chart file"))?;
    let chart_path = Path::new(&chart_path);
    let import = import_bms_chart(chart_path, None, true)
        .with_context(|| format!("import chart for generated preview {}", chart_path.display()))?;
    let mut loader = FfmpegSampleLoader;
    render_generated_preview_sample(
        &import.chart,
        start_ms,
        GENERATED_PREVIEW_DURATION_MS,
        sample_rate,
        &mut loader,
    )
}

pub(crate) fn render_generated_preview_sample(
    chart: &PlayableChart,
    start_ms: i64,
    duration_ms: i64,
    sample_rate: u32,
    loader: &mut dyn SampleLoader,
) -> Result<DecodedSample> {
    if sample_rate == 0 {
        bail!("generated preview sample rate must be non-zero");
    }
    if duration_ms <= 0 {
        bail!("generated preview duration must be positive");
    }

    let start_us = start_ms.max(0).saturating_mul(1_000);
    let end_us = start_us.saturating_add(duration_ms.saturating_mul(1_000));
    let note_preroll_start_us = start_us.saturating_sub(GENERATED_PREVIEW_PREROLL_MS * 1_000);
    let mut sound_ids = HashSet::new();
    let bgm_event_indices =
        preview_bgm_event_indices(chart, start_us, note_preroll_start_us, end_us, loader);

    for index in &bgm_event_indices {
        sound_ids.insert(chart.bgm_events[*index].sound);
    }
    for lane_notes in &chart.lane_notes {
        for note in lane_notes {
            if note.time.0 < note_preroll_start_us || note.time.0 > end_us {
                continue;
            }
            if matches!(note.kind, NoteKind::Invisible | NoteKind::Mine) {
                continue;
            }
            if let Some(sound) = note.sound {
                sound_ids.insert(sound);
            }
        }
    }

    if sound_ids.is_empty() {
        bail!("generated preview has no sounds in the selected window");
    }

    let mut filtered_chart = chart.clone();
    filtered_chart.sounds.retain(|sound| sound_ids.contains(&sound.id));

    let mut engine = AudioEngine::new(sample_rate);
    let reports = load_chart_samples(&mut engine, &filtered_chart, loader);
    if reports.iter().all(|report| matches!(report.status, LoadedSampleStatus::Failed(_))) {
        let failures = reports
            .iter()
            .filter_map(|report| match &report.status {
                LoadedSampleStatus::Loaded => None,
                LoadedSampleStatus::Failed(error) => {
                    Some(format!("{}: {error}", report.path.display()))
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        bail!("generated preview failed to load chart samples: {failures}");
    }

    schedule_preview_sounds(&mut engine, chart, &bgm_event_indices, end_us, note_preroll_start_us);

    let frame_count = frames_from_ms(duration_ms, sample_rate);
    let start_frame = time_us_to_frame(start_us, sample_rate);
    let mut frames = vec![0.0_f32; frame_count.saturating_mul(2)];
    let mut rendered = 0_usize;
    while rendered < frame_count {
        let chunk_frames = RENDER_CHUNK_FRAMES.min(frame_count - rendered);
        let frame_offset = rendered * 2;
        let frame_end = frame_offset + chunk_frames * 2;
        engine.render_stereo(
            start_frame.saturating_add(rendered as u64),
            &mut frames[frame_offset..frame_end],
        );
        rendered += chunk_frames;
    }

    apply_preview_envelope_and_limit(&mut frames, sample_rate);
    let peak = peak_abs(&frames);
    if peak < 0.0001 {
        bail!("generated preview rendered silence");
    }

    Ok(DecodedSample { channels: 2, sample_rate, frames })
}

fn schedule_preview_sounds(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    bgm_event_indices: &[usize],
    end_us: i64,
    note_preroll_start_us: i64,
) {
    let sample_rate = engine.output_sample_rate();
    let bgm_events = bgm_event_indices.iter().map(|index| {
        let event = &chart.bgm_events[*index];
        let volume =
            chart_channel_volume_factor(chart_volume_at_time(&chart.bgm_volume_events, event.time));
        ScheduledSound {
            sound_id: event.sound,
            start_frame: time_us_to_frame(event.time.0, sample_rate),
            volume,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            restart_policy: RestartPolicy::StopSameSound,
            catch_up: true,
        }
    });
    engine.schedule_all(bgm_events);

    let key_events = chart.lane_notes.iter().flat_map(|lane_notes| {
        lane_notes.iter().filter_map(move |note| {
            if note.time.0 < note_preroll_start_us || note.time.0 > end_us {
                return None;
            }
            if matches!(note.kind, NoteKind::Invisible | NoteKind::Mine) {
                return None;
            }
            let sound_id = note.sound?;
            let volume = chart_channel_volume_factor(chart_volume_at_time(
                &chart.key_volume_events,
                note.time,
            ));
            Some(ScheduledSound {
                sound_id,
                start_frame: time_us_to_frame(note.time.0, sample_rate),
                volume,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                restart_policy: RestartPolicy::StopSameSound,
                catch_up: true,
            })
        })
    });
    engine.schedule_all(key_events);
}

fn preview_bgm_event_indices(
    chart: &PlayableChart,
    preview_start_us: i64,
    note_preroll_start_us: i64,
    end_us: i64,
    loader: &mut dyn SampleLoader,
) -> Vec<usize> {
    let mut indices = Vec::new();
    for (index, event) in chart.bgm_events.iter().enumerate() {
        if event.time.0 >= note_preroll_start_us && event.time.0 <= end_us {
            indices.push(index);
        }
    }

    let early_limit_us = chart
        .bgm_events
        .first()
        .map(|event| event.time.0.saturating_add(GENERATED_PREVIEW_BGM_EARLY_GRACE_MS * 1_000))
        .unwrap_or(0);
    for (index, event) in chart.bgm_events.iter().enumerate() {
        if event.time.0 >= note_preroll_start_us {
            break;
        }
        if event.time.0 <= early_limit_us {
            indices.push(index);
        }
    }

    let mut lookback_count = 0;
    for (index, event) in chart.bgm_events.iter().enumerate().rev() {
        if event.time.0 >= note_preroll_start_us {
            continue;
        }
        indices.push(index);
        lookback_count += 1;
        if lookback_count >= GENERATED_PREVIEW_BGM_LOOKBACK_EVENTS {
            break;
        }
    }

    // 直前の少数イベントだけでは、途中から鳴り続ける長尺BGMレーンを落とす。
    // 候補をファイルサイズで絞ってからコンテナのdurationだけを調べ、プレビュー開始時点
    // まで届く音だけを追加することで、全BGMのデコードは避ける。
    let sound_paths = chart
        .sounds
        .iter()
        .map(|sound| (sound.id, sound.path.as_path()))
        .collect::<HashMap<_, _>>();
    let mut seen_sound_ids = chart
        .bgm_events
        .iter()
        .filter(|event| event.time.0 >= note_preroll_start_us)
        .map(|event| event.sound)
        .collect::<HashSet<_>>();
    let mut duration_candidates = Vec::new();
    for (index, event) in chart.bgm_events.iter().enumerate().rev() {
        if event.time.0 >= note_preroll_start_us || !seen_sound_ids.insert(event.sound) {
            continue;
        }
        if indices.contains(&index) {
            continue;
        }
        let Some(path) = sound_paths.get(&event.sound) else {
            continue;
        };
        let Some(resolved_path) = sound_asset_candidates(path).into_iter().next() else {
            continue;
        };
        let file_bytes =
            std::fs::metadata(&resolved_path).ok().map(|metadata| metadata.len()).unwrap_or(0);
        duration_candidates.push((file_bytes, index, resolved_path));
    }
    duration_candidates.sort_unstable_by(
        |(left_bytes, left_index, _), (right_bytes, right_index, _)| {
            right_bytes.cmp(left_bytes).then_with(|| right_index.cmp(left_index))
        },
    );
    for (_, index, path) in
        duration_candidates.into_iter().take(GENERATED_PREVIEW_BGM_DURATION_PROBE_CANDIDATES)
    {
        let event = &chart.bgm_events[index];
        let Some(duration_ms) = loader.duration_ms_hint(&path) else {
            continue;
        };
        let ends_at_us = event.time.0.saturating_add(duration_ms.saturating_mul(1_000));
        if ends_at_us > preview_start_us {
            indices.push(index);
        }
    }

    indices.sort_unstable();
    indices.dedup();
    indices
}

fn weighted_distribution_notes(distribution: &ChartDistributionSecond) -> f32 {
    let tap_like = distribution.key_taps
        + distribution.scratch_taps
        + distribution.key_long_heads
        + distribution.scratch_long_heads;
    let long_bodies = distribution.key_long_bodies + distribution.scratch_long_bodies;
    tap_like as f32 + long_bodies as f32 * 0.25
}

fn fallback_start_second(length_seconds: usize) -> usize {
    if length_seconds == 0 {
        return 0;
    }
    let target = length_seconds * 45 / 100;
    let latest = length_seconds.saturating_sub(GENERATED_PREVIEW_DURATION_MS as usize / 1_000);
    target.min(latest)
}

fn seconds_from_ms(length_ms: i64) -> usize {
    if length_ms <= 0 { 0 } else { ((length_ms as u64).saturating_add(999) / 1_000) as usize }
}

fn frames_from_ms(duration_ms: i64, sample_rate: u32) -> usize {
    let frames =
        (duration_ms.max(1) as u128).saturating_mul(sample_rate as u128).saturating_add(999)
            / 1_000;
    frames.min(usize::MAX as u128) as usize
}

fn time_us_to_frame(time_us: i64, sample_rate: u32) -> u64 {
    if time_us <= 0 {
        return 0;
    }
    (time_us as u128)
        .saturating_mul(sample_rate as u128)
        .saturating_add(999_999)
        .saturating_div(1_000_000)
        .min(u64::MAX as u128) as u64
}

fn apply_preview_envelope_and_limit(frames: &mut [f32], sample_rate: u32) {
    if frames.is_empty() {
        return;
    }

    let frame_count = frames.len() / 2;
    let fade_in_frames = frames_from_ms(GENERATED_PREVIEW_FADE_IN_MS, sample_rate).min(frame_count);
    let fade_out_frames =
        frames_from_ms(GENERATED_PREVIEW_FADE_OUT_MS, sample_rate).min(frame_count);
    for frame_index in 0..fade_in_frames {
        let gain = fade_gain(frame_index, fade_in_frames, false);
        for channel in 0..2 {
            frames[frame_index * 2 + channel] *= gain;
        }
    }
    for frame_index in 0..fade_out_frames {
        let gain = fade_gain(frame_index, fade_out_frames, true);
        let output_frame = frame_count - fade_out_frames + frame_index;
        for channel in 0..2 {
            frames[output_frame * 2 + channel] *= gain;
        }
    }

    let peak = peak_abs(frames);
    if peak > 0.98 {
        let scale = 0.98 / peak;
        for sample in frames {
            *sample *= scale;
        }
    }
}

fn fade_gain(frame_index: usize, fade_frames: usize, invert: bool) -> f32 {
    if fade_frames <= 1 {
        return 1.0;
    }
    let progress = frame_index as f32 / (fade_frames - 1) as f32;
    if invert { 1.0 - progress } else { progress }
}

fn peak_abs(frames: &[f32]) -> f32 {
    frames.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use bmz_audio::loader::{SampleLoadError, SampleLoader};
    use bmz_audio::sample::DecodedSample;
    use bmz_chart::model::{
        BarLine, ChartMetadata, ChartVolumeEvent, JudgeRankEvent, NoteEvent, PlayableChart,
        ScrollEvent, SoundAssetRef, SoundEvent, SpeedEvent, TimingEvent,
    };
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;

    #[derive(Default)]
    struct TestLoader {
        samples: HashMap<PathBuf, DecodedSample>,
        duration_hints_ms: HashMap<PathBuf, i64>,
        loaded_paths: Vec<PathBuf>,
    }

    impl SampleLoader for TestLoader {
        fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
            self.loaded_paths.push(path.to_path_buf());
            self.samples.get(path).cloned().ok_or_else(|| SampleLoadError::Decode {
                path: path.to_path_buf(),
                message: "missing test sample".to_owned(),
            })
        }

        fn duration_ms_hint(&mut self, path: &Path) -> Option<i64> {
            self.duration_hints_ms.get(path).copied()
        }
    }

    #[test]
    fn fallback_preview_start_prefers_dense_middle_window() {
        let mut distribution = vec![ChartDistributionSecond::default(); 100];
        for item in distribution.iter_mut().take(20).skip(10) {
            item.key_taps = 3;
        }
        for item in distribution.iter_mut().take(58).skip(50) {
            item.key_taps = 10;
        }
        for item in distribution.iter_mut().take(94).skip(90) {
            item.key_taps = 30;
        }

        let start_ms = fallback_preview_start_ms(&distribution, 100_000).unwrap();

        assert!((48_000..=56_000).contains(&start_ms));
    }

    #[test]
    fn fallback_preview_weights_long_bodies_lower_than_taps() {
        let mut distribution = vec![ChartDistributionSecond::default(); 80];
        for item in distribution.iter_mut().take(28).skip(20) {
            item.key_long_bodies = 20;
        }
        for item in distribution.iter_mut().take(58).skip(50) {
            item.key_taps = 6;
        }

        let start_ms = fallback_preview_start_ms(&distribution, 80_000).unwrap();

        assert!((48_000..=56_000).contains(&start_ms));
    }

    #[test]
    fn generated_preview_key_round_trips() {
        let key = generated_preview_cache_key(42, 15_000);
        assert_eq!(
            parse_generated_preview_cache_key(&key),
            Some(GeneratedPreviewKey { chart_id: 42, start_ms: 15_000 })
        );
        assert_eq!(parse_generated_preview_cache_key("folder|preview.ogg"), None);
    }

    #[test]
    fn generated_preview_applies_requested_fade_in_and_out() {
        let mut frames = vec![0.5; 2_000 * 2];

        apply_preview_envelope_and_limit(&mut frames, 1_000);

        assert_eq!(frames[0], 0.0);
        assert!((frames[250 * 2] - 0.25).abs() < 0.001);
        assert_eq!(frames[499 * 2], 0.5);
        assert_eq!(frames[1_000 * 2], 0.5);
        assert!((frames[1_500 * 2] - 0.25).abs() < 0.001);
        assert_eq!(frames[(2_000 - 1) * 2], 0.0);
    }

    #[test]
    fn render_generated_preview_keeps_bgm_started_before_window() {
        let temp_dir =
            std::env::temp_dir().join(format!("bmz-generated-preview-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let sample_path = temp_dir.join("bgm.wav");
        fs::write(&sample_path, b"dummy").unwrap();

        let sample_rate = 1_000;
        let chart = test_chart_with_bgm(sample_path.clone());
        let mut loader = TestLoader::default();
        loader.samples.insert(
            sample_path,
            DecodedSample { channels: 1, sample_rate, frames: vec![1.0; 5_000] },
        );

        let sample =
            render_generated_preview_sample(&chart, 1_000, 1_000, sample_rate, &mut loader)
                .unwrap();

        assert_eq!(sample.channels, 2);
        assert_eq!(sample.sample_rate, sample_rate);
        let middle_left = sample.frames[500 * 2];
        assert!(middle_left > 0.45);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn render_generated_preview_skips_invisible_note_sounds() {
        let temp_dir = std::env::temp_dir()
            .join(format!("bmz-generated-preview-invisible-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let tap_path = temp_dir.join("tap.wav");
        let invisible_path = temp_dir.join("invisible.wav");
        fs::write(&tap_path, b"dummy").unwrap();
        fs::write(&invisible_path, b"dummy").unwrap();

        let sample_rate = 1_000;
        let mut chart = test_chart_with_bgm(tap_path.clone());
        chart.bgm_events.clear();
        chart.sounds = vec![
            SoundAssetRef { id: SoundId(1), path: tap_path.clone() },
            SoundAssetRef { id: SoundId(2), path: invisible_path.clone() },
        ];
        chart.lane_notes[Lane::Key1.index()].extend([
            NoteEvent {
                id: NoteId(1),
                lane: Lane::Key1,
                kind: NoteKind::Tap,
                tick: ChartTick(0),
                time: TimeUs(0),
                sound: Some(SoundId(1)),
                damage: None,
            },
            NoteEvent {
                id: NoteId(2),
                lane: Lane::Key1,
                kind: NoteKind::Invisible,
                tick: ChartTick(0),
                time: TimeUs(0),
                sound: Some(SoundId(2)),
                damage: None,
            },
        ]);
        let mut loader = TestLoader::default();
        loader.samples.insert(
            tap_path.clone(),
            DecodedSample { channels: 1, sample_rate, frames: vec![0.5; 1_000] },
        );
        loader.samples.insert(
            invisible_path.clone(),
            DecodedSample { channels: 1, sample_rate, frames: vec![1.0; 1_000] },
        );

        let sample =
            render_generated_preview_sample(&chart, 0, 1_000, sample_rate, &mut loader).unwrap();

        assert!((sample.frames[500 * 2] - 0.25).abs() < 0.001);
        assert!(loader.loaded_paths.contains(&tap_path));
        assert!(!loader.loaded_paths.contains(&invisible_path));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn render_generated_preview_limits_bgm_assets_before_window() {
        let temp_dir = std::env::temp_dir()
            .join(format!("bmz-generated-preview-lookback-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        let sample_rate = 1_000;
        let mut chart = test_chart_with_bgm(temp_dir.join("unused.wav"));
        chart.bgm_events.clear();
        chart.sounds.clear();

        let mut loader = TestLoader::default();
        let mut early_path = None;
        let mut long_path = None;
        let mut distant_path = None;
        let mut window_path = None;
        for index in 0..64 {
            let declared_path = temp_dir.join(format!("bgm-{index}.wav"));
            let path = if index == 10 {
                temp_dir.join(format!("bgm-{index}.ogg"))
            } else {
                declared_path.clone()
            };
            let file_bytes = if index == 10 { vec![0; 1_024] } else { b"dummy".to_vec() };
            fs::write(&path, file_bytes).unwrap();
            let sound_id = SoundId(index as u32 + 1);
            chart.sounds.push(SoundAssetRef { id: sound_id, path: declared_path });
            chart.bgm_events.push(SoundEvent {
                tick: ChartTick(index as u64 * 3_840),
                time: TimeUs(index as i64 * 1_000_000),
                sound: sound_id,
            });
            let frames = if matches!(index, 0 | 10) { vec![1.0; 70_000] } else { vec![0.5; 1_000] };
            loader.samples.insert(path.clone(), DecodedSample { channels: 1, sample_rate, frames });
            if index == 0 {
                early_path = Some(path.clone());
            } else if index == 10 {
                loader.duration_hints_ms.insert(path.clone(), 70_000);
                long_path = Some(path.clone());
            } else if index == 4 {
                distant_path = Some(path.clone());
            } else if index == 50 {
                window_path = Some(path.clone());
            }
        }

        let sample =
            render_generated_preview_sample(&chart, 50_000, 1_000, sample_rate, &mut loader)
                .unwrap();

        assert!(sample.frames[500 * 2] > 0.5);
        assert!(loader.loaded_paths.len() < 20, "loaded paths: {:?}", loader.loaded_paths);
        assert!(loader.loaded_paths.contains(&early_path.unwrap()));
        assert!(loader.loaded_paths.contains(&long_path.unwrap()));
        assert!(loader.loaded_paths.contains(&window_path.unwrap()));
        assert!(!loader.loaded_paths.contains(&distant_path.unwrap()));

        let _ = fs::remove_dir_all(temp_dir);
    }

    fn test_chart_with_bgm(sample_path: PathBuf) -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes: std::array::from_fn(|_| Vec::<NoteEvent>::new()),
            long_notes: Vec::new(),
            bgm_events: vec![SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(1) }],
            bga_events: Vec::new(),
            timing_events: Vec::<TimingEvent>::new(),
            scroll_events: Vec::<ScrollEvent>::new(),
            speed_events: Vec::<SpeedEvent>::new(),
            judge_rank_events: Vec::<JudgeRankEvent>::new(),
            bgm_volume_events: Vec::<ChartVolumeEvent>::new(),
            key_volume_events: Vec::<ChartVolumeEvent>::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::<BarLine>::new(),
            sounds: vec![SoundAssetRef { id: SoundId(1), path: sample_path }],
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(5_000_000),
        }
    }
}
