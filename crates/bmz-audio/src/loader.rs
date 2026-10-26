use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bmz_chart::model::{PlayableChart, SoundAssetRef, SoundSlice};
use bmz_chart::sound_asset::{first_sound_asset_candidate, sound_asset_candidates};
use bmz_chart::volume::volwav_factor;
use thiserror::Error;

use crate::engine::AudioEngine;
use crate::sample::DecodedSample;

#[derive(Debug, Error)]
pub enum SampleLoadError {
    #[error("failed to read sample file: {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to decode sample file: {path}: {message}")]
    Decode { path: PathBuf, message: String },
}

pub trait SampleLoader {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError>;

    /// Prepared PCM may be shared across loads. Custom loaders retain the same
    /// sanitization, VOLWAV and resampling rules through this default method.
    fn load_for_engine(
        &mut self,
        path: &Path,
        volwav: f32,
        output_rate: u32,
    ) -> Result<Arc<DecodedSample>, SampleLoadError> {
        self.load(path).map(|sample| prepare_sample(sample, path, volwav, output_rate))
    }

    /// 音声全体をデコードせずに取得できる場合の再生時間ヒント (ms)。
    /// 時間不明なローダーは `None` を返す。
    fn duration_ms_hint(&mut self, _path: &Path) -> Option<i64> {
        None
    }
}

#[derive(Debug, Default)]
pub struct WavSampleLoader;

impl SampleLoader for WavSampleLoader {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
        let bytes = bmz_song_resource::read_file(path)
            .map_err(|source| SampleLoadError::Io { path: path.to_path_buf(), source })?;
        let mut sample = decode_wav(path, &bytes)?;
        sample.sanitize_pcm(path);
        Ok(sample)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedSampleReport {
    pub path: PathBuf,
    pub status: LoadedSampleStatus,
}

#[derive(Debug, Clone)]
pub enum LoadedSampleStatus {
    Loaded,
    Failed(String),
}

#[derive(Debug, Clone)]
enum CachedDecode {
    Loaded(Arc<DecodedSample>),
    Failed(String),
}

pub fn load_chart_samples(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    loader: &mut dyn SampleLoader,
) -> Vec<LoadedSampleReport> {
    load_chart_samples_with_progress(engine, chart, loader, |_, _| {})
}

pub fn load_chart_samples_with_progress(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    loader: &mut dyn SampleLoader,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<LoadedSampleReport> {
    on_progress(0, chart.sounds.len());
    load_chart_samples_in_order(
        engine,
        chart,
        loader,
        HashMap::new(),
        HashMap::new(),
        0,
        on_progress,
    )
}

/// Decodes sound files on several threads before [`load_chart_samples_with_progress`]'s
/// ordered pass. Each worker builds its own loader; a worker can also adjust its thread,
/// e.g. lower its priority while the Select screen plays audio.
pub struct ParallelSampleDecode<'a> {
    pub threads: usize,
    pub make_loader: &'a (dyn Fn() -> Box<dyn SampleLoader + Send> + Sync),
}

/// [`load_chart_samples_with_progress`] with the first candidate of every declared sound
/// decoded in parallel. The ordered pass then installs the same regions and decodes later
/// candidates only where the first one failed, so the result matches the sequential load.
pub fn load_chart_samples_parallel_with_progress(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    decode: ParallelSampleDecode<'_>,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<LoadedSampleReport> {
    let total = chart.sounds.len();
    on_progress(0, total);
    let volwav = volwav_factor(chart.metadata.volwav_percent);
    let output_rate = engine.output_sample_rate();
    let mut seen = HashSet::new();
    let paths: Vec<&PathBuf> =
        chart.sounds.iter().map(|asset| &asset.path).filter(|path| seen.insert(*path)).collect();
    let mut candidates_by_declared_path = HashMap::<PathBuf, Vec<PathBuf>>::new();
    let mut decoded_by_path = HashMap::<PathBuf, CachedDecode>::new();
    let mut progress = 0;
    let threads = decode.threads.min(paths.len());
    if threads > 1 {
        // Candidate stat calls are significant even on a PCM cache hit. Resolve
        // them on the same workers; deduplicate shared first candidates before
        // decoding so fallback aliases still decode only once.
        let shared = std::sync::Mutex::new(HashMap::<
            PathBuf,
            Arc<std::sync::OnceLock<CachedDecode>>,
        >::new());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            for _ in 0..threads {
                let sender = sender.clone();
                let (next, paths, shared) = (&next, &paths, &shared);
                scope.spawn(move || {
                    let mut loader = (decode.make_loader)();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(path) = paths.get(index) else {
                            break;
                        };
                        let first = first_sound_asset_candidate(path);
                        let decoded = first.map(|first| {
                            let cell =
                                shared.lock().unwrap().entry(first.clone()).or_default().clone();
                            let decoded = cell
                                .get_or_init(|| {
                                    decode_for_engine(&mut *loader, &first, volwav, output_rate)
                                })
                                .clone();
                            (first, decoded)
                        });
                        let candidates = match &decoded {
                            Some((first, CachedDecode::Loaded(_))) => vec![first.clone()],
                            Some((_, CachedDecode::Failed(_))) => sound_asset_candidates(path),
                            None => Vec::new(),
                        };
                        if sender.send(((*path).clone(), candidates, decoded)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);
            // Report decode progress in sound units; the ordered pass continues from here.
            for (done, (path, candidates, decoded)) in receiver.iter().enumerate() {
                candidates_by_declared_path.insert(path, candidates);
                if let Some((path, decoded)) = decoded {
                    decoded_by_path.insert(path, decoded);
                }
                let reached = (done + 1) * total / paths.len();
                if reached > progress && reached < total {
                    progress = reached;
                    on_progress(progress, total);
                }
            }
        });
    }
    let mut fallback = (decode.make_loader)();
    load_chart_samples_in_order(
        engine,
        chart,
        &mut *fallback,
        candidates_by_declared_path,
        decoded_by_path,
        progress,
        on_progress,
    )
}

fn load_chart_samples_in_order(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    loader: &mut dyn SampleLoader,
    mut candidates_by_declared_path: HashMap<PathBuf, Vec<PathBuf>>,
    mut decoded_by_path: HashMap<PathBuf, CachedDecode>,
    reported: usize,
    mut on_progress: impl FnMut(usize, usize),
) -> Vec<LoadedSampleReport> {
    let volwav = volwav_factor(chart.metadata.volwav_percent);
    let total = chart.sounds.len();
    chart
        .sounds
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            let report = load_asset(
                engine,
                asset,
                loader,
                volwav,
                &mut candidates_by_declared_path,
                &mut decoded_by_path,
            );
            if index + 1 > reported {
                on_progress(index + 1, total);
            }
            report
        })
        .collect()
}

/// Decodes one file into the engine's form: sanitized PCM with VOLWAV applied once and
/// resampled to the output rate.
fn decode_for_engine(
    loader: &mut dyn SampleLoader,
    path: &Path,
    volwav: f32,
    output_rate: u32,
) -> CachedDecode {
    match loader.load_for_engine(path, volwav, output_rate) {
        Ok(sample) => CachedDecode::Loaded(sample),
        Err(error) => CachedDecode::Failed(error.to_string()),
    }
}

pub(crate) fn prepare_sample(
    mut sample: DecodedSample,
    path: &Path,
    volwav: f32,
    output_rate: u32,
) -> Arc<DecodedSample> {
    sample.sanitize_pcm(path);
    sample.apply_gain(volwav);
    Arc::new(if sample.sample_rate == output_rate {
        sample
    } else {
        sample.resampled_to(output_rate)
    })
}

fn load_asset(
    engine: &mut AudioEngine,
    asset: &SoundAssetRef,
    loader: &mut dyn SampleLoader,
    volwav: f32,
    candidates_by_declared_path: &mut HashMap<PathBuf, Vec<PathBuf>>,
    decoded_by_path: &mut HashMap<PathBuf, CachedDecode>,
) -> LoadedSampleReport {
    // 同じ宣言pathの各 SoundId は同じ候補列を使う。特に `stronger` の多数
    // region では、毎回 filesystem 上の候補を stat しない。
    let candidates = candidates_by_declared_path
        .entry(asset.path.clone())
        .or_insert_with(|| sound_asset_candidates(&asset.path));
    if candidates.is_empty() {
        let error = SampleLoadError::Io {
            path: asset.path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "sample file not found"),
        };
        return LoadedSampleReport {
            path: asset.path.clone(),
            status: LoadedSampleStatus::Failed(error.to_string()),
        };
    }

    let mut last_error = None;
    let mut last_path = asset.path.clone();
    for path in candidates.iter() {
        last_path = path.clone();
        let source = match decoded_by_path.entry(path.clone()) {
            Entry::Occupied(entry) => match entry.into_mut() {
                CachedDecode::Loaded(sample) => sample.clone(),
                CachedDecode::Failed(error) => {
                    last_error = Some(error.clone());
                    continue;
                }
            },
            // 同一pathの VOLWAV 適用・出力レート化は最初の1回だけ行う。
            Entry::Vacant(entry) => {
                match entry.insert(decode_for_engine(
                    loader,
                    path,
                    volwav,
                    engine.output_sample_rate(),
                )) {
                    CachedDecode::Loaded(sample) => sample.clone(),
                    CachedDecode::Failed(error) => {
                        last_error = Some(error.clone());
                        continue;
                    }
                }
            }
        };

        let (start_frame, end_frame) = asset
            .slice
            .map_or((0, source.frame_count()), |slice| slice_frame_range(&source, slice));
        engine.insert_shared_sample_region(asset.id, source, start_frame, end_frame);
        return LoadedSampleReport { path: path.clone(), status: LoadedSampleStatus::Loaded };
    }

    LoadedSampleReport {
        path: last_path,
        status: LoadedSampleStatus::Failed(
            last_error.unwrap_or_else(|| "sample file not found".to_string()),
        ),
    }
}

fn slice_frame_range(sample: &DecodedSample, slice: SoundSlice) -> (usize, usize) {
    let channels = sample.channels as usize;
    if channels == 0 || sample.sample_rate == 0 {
        return (0, sample.frame_count());
    }
    let frame_count = sample.frame_count();
    let start_frame = ((slice.start_us as u128 * sample.sample_rate as u128) / 1_000_000)
        .min(frame_count as u128) as usize;
    let end_frame = slice.duration_us.map_or(frame_count, |duration_us| {
        let duration_frames = (duration_us as u128 * sample.sample_rate as u128) / 1_000_000;
        (start_frame as u128 + duration_frames).min(frame_count as u128) as usize
    });
    (start_frame, end_frame)
}

fn decode_wav(path: &Path, bytes: &[u8]) -> Result<DecodedSample, SampleLoadError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(decode_error(path, "not a RIFF/WAVE file"));
    }

    let mut offset = 12;
    let mut format: Option<WavFormat> = None;
    let mut data: Option<&[u8]> = None;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let len = le_u32(&bytes[offset + 4..offset + 8]) as usize;
        offset += 8;
        if offset + len > bytes.len() {
            return Err(decode_error(path, "chunk extends past end of file"));
        }

        let chunk = &bytes[offset..offset + len];
        match id {
            b"fmt " => format = Some(parse_wav_format(path, chunk)?),
            b"data" => data = Some(chunk),
            _ => {}
        }
        offset += len + (len % 2);
    }

    let format = format.ok_or_else(|| decode_error(path, "missing fmt chunk"))?;
    let data = data.ok_or_else(|| decode_error(path, "missing data chunk"))?;
    let frames = decode_wav_frames(path, format, data)?;
    Ok(DecodedSample { channels: format.channels, sample_rate: format.sample_rate, frames })
}

#[derive(Debug, Clone, Copy)]
struct WavFormat {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

fn parse_wav_format(path: &Path, chunk: &[u8]) -> Result<WavFormat, SampleLoadError> {
    if chunk.len() < 16 {
        return Err(decode_error(path, "fmt chunk is too short"));
    }

    let format = WavFormat {
        audio_format: le_u16(&chunk[0..2]),
        channels: le_u16(&chunk[2..4]),
        sample_rate: le_u32(&chunk[4..8]),
        bits_per_sample: le_u16(&chunk[14..16]),
    };

    if format.channels == 0 {
        return Err(decode_error(path, "channel count is zero"));
    }

    Ok(format)
}

fn decode_wav_frames(
    path: &Path,
    format: WavFormat,
    data: &[u8],
) -> Result<Vec<f32>, SampleLoadError> {
    match (format.audio_format, format.bits_per_sample) {
        (1, 8) => Ok(data.iter().map(|sample| (*sample as f32 - 128.0) / 128.0).collect()),
        (1, 16) => {
            if !data.len().is_multiple_of(2) {
                return Err(decode_error(path, "16-bit PCM data length is odd"));
            }
            Ok(data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
                .collect())
        }
        (3, 32) => {
            if !data.len().is_multiple_of(4) {
                return Err(decode_error(path, "32-bit float data length is not divisible by 4"));
            }
            Ok(data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
                .collect())
        }
        _ => Err(decode_error(
            path,
            format!(
                "unsupported WAV format {} with {} bits per sample",
                format.audio_format, format.bits_per_sample
            ),
        )),
    }
}

fn decode_error(path: &Path, message: impl Into<String>) -> SampleLoadError {
    SampleLoadError::Decode { path: path.to_path_buf(), message: message.into() }
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef};
    use bmz_core::ids::SoundId;
    use bmz_core::time::TimeUs;

    use super::*;

    #[derive(Default)]
    struct TestLoader {
        samples: HashMap<PathBuf, DecodedSample>,
        failures: HashMap<PathBuf, String>,
        attempts: Vec<PathBuf>,
    }

    impl SampleLoader for TestLoader {
        fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
            self.attempts.push(path.to_path_buf());
            if let Some(message) = self.failures.get(path) {
                return Err(SampleLoadError::Decode {
                    path: path.to_path_buf(),
                    message: message.clone(),
                });
            }
            self.samples.get(path).cloned().ok_or_else(|| SampleLoadError::Decode {
                path: path.to_path_buf(),
                message: "missing test sample".to_string(),
            })
        }
    }

    #[test]
    fn load_chart_samples_inserts_loaded_samples_and_reports_failures() {
        let mut engine = AudioEngine::default();
        let temp = temp_dir("load-samples");
        let dir = temp.path().to_path_buf();
        let ok_path = dir.join("ok.wav");
        let missing_path = dir.join("missing.wav");
        std::fs::write(&ok_path, b"dummy").unwrap();
        let chart = chart_with_sounds(vec![
            SoundAssetRef { id: SoundId(1), path: ok_path.clone(), slice: None },
            SoundAssetRef { id: SoundId(2), path: missing_path.clone(), slice: None },
        ]);
        let mut loader = TestLoader::default();
        loader
            .samples
            .insert(ok_path, DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] });

        let report = load_chart_samples(&mut engine, &chart, &mut loader);

        assert_eq!(report.len(), 2);
        assert!(matches!(report[0].status, LoadedSampleStatus::Loaded));
        assert!(matches!(report[1].status, LoadedSampleStatus::Failed(_)));
        assert!(engine.samples.get(SoundId(1)).is_some());
        assert!(engine.samples.get(SoundId(2)).is_none());
    }

    #[test]
    fn parallel_load_matches_sequential_with_fallbacks_and_failures() {
        let temp = temp_dir("parallel");
        let dir = temp.path().to_path_buf();
        let mut sounds = Vec::new();
        let mut samples = HashMap::new();
        let mut failures = HashMap::new();
        for index in 0..40 {
            let path = dir.join(format!("s{index}.wav"));
            std::fs::write(&path, b"dummy").unwrap();
            samples.insert(
                path.clone(),
                DecodedSample {
                    channels: 1,
                    sample_rate: 24_000,
                    frames: vec![index as f32 / 40.0; 8],
                },
            );
            sounds.push(SoundAssetRef { id: SoundId(index), path, slice: None });
        }
        // The declared WAV fails to decode and the OGG of the same stem stands in.
        let broken = dir.join("broken.wav");
        let fallback = dir.join("broken.ogg");
        std::fs::write(&broken, b"dummy").unwrap();
        std::fs::write(&fallback, b"dummy").unwrap();
        failures.insert(broken.clone(), "corrupt".to_string());
        samples.insert(
            fallback.clone(),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 4] },
        );
        sounds.push(SoundAssetRef { id: SoundId(100), path: broken, slice: None });
        sounds.push(SoundAssetRef { id: SoundId(101), path: dir.join("missing.wav"), slice: None });
        // A region sharing a source with an earlier sound.
        sounds.push(SoundAssetRef {
            id: SoundId(102),
            path: dir.join("s3.wav"),
            slice: Some(SoundSlice { start_us: 0, duration_us: Some(10) }),
        });
        let mut chart = chart_with_sounds(sounds);
        chart.metadata.volwav_percent = 50;
        let make = || {
            Box::new(TestLoader {
                samples: samples.clone(),
                failures: failures.clone(),
                attempts: Vec::new(),
            }) as Box<dyn SampleLoader + Send>
        };

        let mut sequential = AudioEngine::new(48_000);
        let sequential_reports = load_chart_samples(&mut sequential, &chart, &mut *make());
        let mut parallel = AudioEngine::new(48_000);
        let mut progress = Vec::new();
        let parallel_reports = load_chart_samples_parallel_with_progress(
            &mut parallel,
            &chart,
            ParallelSampleDecode { threads: 4, make_loader: &make },
            |loaded, total| progress.push((loaded, total)),
        );

        let summary = |reports: &[LoadedSampleReport]| {
            reports
                .iter()
                .map(|report| {
                    (report.path.clone(), matches!(report.status, LoadedSampleStatus::Loaded))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(summary(&parallel_reports), summary(&sequential_reports));
        assert_eq!(parallel_reports[40].path, fallback);
        for sound in &chart.sounds {
            let left = sequential.samples.get(sound.id).map(|sample| {
                (0..sample.frame_count())
                    .map(|frame| sample.sample_stereo(frame))
                    .collect::<Vec<_>>()
            });
            let right = parallel.samples.get(sound.id).map(|sample| {
                (0..sample.frame_count())
                    .map(|frame| sample.sample_stereo(frame))
                    .collect::<Vec<_>>()
            });
            assert_eq!(left, right, "{:?}", sound.id);
        }
        let total = chart.sounds.len();
        assert_eq!(progress.first(), Some(&(0, total)));
        assert_eq!(progress.last(), Some(&(total, total)));
        assert!(progress.windows(2).all(|pair| pair[0].0 < pair[1].0));

        let cache = Arc::new(std::sync::Mutex::new(crate::sample_cache::SampleCache::new(1 << 20)));
        let make_cached = || {
            Box::new(crate::sample_cache::CachedSampleLoader::new(
                TestLoader {
                    samples: samples.clone(),
                    failures: failures.clone(),
                    attempts: Vec::new(),
                },
                cache.clone(),
            )) as Box<dyn SampleLoader + Send>
        };
        let mut previous = None::<AudioEngine>;
        for _ in 0..2 {
            let mut engine = AudioEngine::new(48_000);
            let reports = load_chart_samples_parallel_with_progress(
                &mut engine,
                &chart,
                ParallelSampleDecode { threads: 4, make_loader: &make_cached },
                |_, _| {},
            );
            assert_eq!(summary(&reports), summary(&sequential_reports));
            for sound in &chart.sounds {
                let expected = sequential.samples.get(sound.id);
                let actual = engine.samples.get(sound.id);
                assert_eq!(expected.map(|s| s.frame_count()), actual.map(|s| s.frame_count()));
                if let (Some(expected), Some(actual)) = (expected, actual) {
                    for frame in 0..expected.frame_count() {
                        assert_eq!(expected.sample_stereo(frame), actual.sample_stereo(frame));
                    }
                    if let Some(previous) = &previous {
                        assert!(previous.samples.get(sound.id).unwrap().shares_source_with(actual));
                    }
                }
            }
            previous = Some(engine);
        }
        assert!(cache.lock().unwrap().stats().hits >= 41);
    }

    #[test]
    fn parallel_candidates_with_different_declarations_decode_one_shared_file() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountingLoader(Arc<AtomicUsize>);
        impl SampleLoader for CountingLoader {
            fn load(&mut self, _: &Path) -> Result<DecodedSample, SampleLoadError> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 8] })
            }
        }
        let temp = temp_dir("parallel-alias");
        std::fs::write(temp.path().join("shared.ogg"), b"placeholder").unwrap();
        let chart = chart_with_sounds(
            ["wav", "ogg", "mp3"]
                .into_iter()
                .enumerate()
                .map(|(id, ext)| SoundAssetRef {
                    id: SoundId(id as u32),
                    path: temp.path().join(format!("shared.{ext}")),
                    slice: None,
                })
                .collect(),
        );
        for threads in [0, 1, 2, 8] {
            let count = Arc::new(AtomicUsize::new(0));
            let make = || Box::new(CountingLoader(count.clone())) as Box<dyn SampleLoader + Send>;
            let mut audio = AudioEngine::new(48_000);
            let report = load_chart_samples_parallel_with_progress(
                &mut audio,
                &chart,
                ParallelSampleDecode { threads, make_loader: &make },
                |_, _| {},
            );
            assert!(
                report.iter().all(|report| matches!(report.status, LoadedSampleStatus::Loaded))
            );
            assert_eq!(count.load(Ordering::Relaxed), 1);
            assert_eq!(audio.samples.source_count(), 1);
            assert!(
                audio
                    .samples
                    .get(SoundId(0))
                    .unwrap()
                    .shares_source_with(audio.samples.get(SoundId(2)).unwrap())
            );
        }
    }

    #[test]
    fn load_chart_samples_reports_progress_after_each_asset() {
        let mut engine = AudioEngine::default();
        let chart = chart_with_sounds(vec![
            SoundAssetRef { id: SoundId(1), path: PathBuf::from("missing-1.wav"), slice: None },
            SoundAssetRef { id: SoundId(2), path: PathBuf::from("missing-2.wav"), slice: None },
        ]);
        let mut loader = TestLoader::default();
        let mut progress = Vec::new();

        load_chart_samples_with_progress(&mut engine, &chart, &mut loader, |loaded, total| {
            progress.push((loaded, total));
        });

        assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    }

    #[test]
    fn load_chart_samples_applies_volwav_gain() {
        let mut engine = AudioEngine::default();
        let temp = temp_dir("volwav");
        let dir = temp.path().to_path_buf();
        let ok_path = dir.join("ok.wav");
        std::fs::write(&ok_path, b"dummy").unwrap();
        let mut chart = chart_with_sounds(vec![SoundAssetRef {
            id: SoundId(1),
            path: ok_path.clone(),
            slice: None,
        }]);
        chart.metadata.volwav_percent = 50;
        let mut loader = TestLoader::default();
        loader.samples.insert(
            ok_path,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, -1.0] },
        );

        load_chart_samples(&mut engine, &chart, &mut loader);

        let sample = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_stereo(0), (0.5, 0.5));
        assert_eq!(sample.sample_stereo(1), (-0.5, -0.5));
    }

    #[test]
    fn load_chart_samples_caches_source_and_crops_each_slice() {
        let mut engine = AudioEngine::default();
        let temp = temp_dir("sound-slices");
        let dir = temp.path().to_path_buf();
        let path = dir.join("full.wav");
        std::fs::write(&path, b"dummy").unwrap();
        let chart = chart_with_sounds(vec![
            SoundAssetRef {
                id: SoundId(1),
                path: path.clone(),
                slice: Some(SoundSlice { start_us: 200_000, duration_us: Some(300_000) }),
            },
            SoundAssetRef {
                id: SoundId(2),
                path: path.clone(),
                slice: Some(SoundSlice { start_us: 500_000, duration_us: None }),
            },
        ]);
        let mut loader = TestLoader::default();
        loader.samples.insert(
            path.clone(),
            DecodedSample {
                channels: 1,
                sample_rate: 10,
                frames: (0..10).map(|value| value as f32 / 10.0).collect(),
            },
        );

        load_chart_samples(&mut engine, &chart, &mut loader);

        assert_eq!(loader.attempts, vec![path]);
        let first = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(first.frame_count(), 14_400);
        assert!((first.sample_stereo(0).0 - 0.2).abs() < 0.001);
        assert!(first.sample_stereo(first.frame_count() - 1).0 > 0.499);
        assert!(first.sample_stereo(first.frame_count() - 1).0 < 0.5);

        let second = engine.samples.get(SoundId(2)).unwrap();
        assert_eq!(second.frame_count(), 24_000);
        assert!((second.sample_stereo(0).0 - 0.5).abs() < 0.001);
        assert!((second.sample_stereo(second.frame_count() - 1).0 - 0.9).abs() < 0.001);
        assert!(first.shares_source_with(second));
    }

    #[test]
    fn load_chart_samples_caches_candidates_and_decode_failures_by_actual_path() {
        let mut engine = AudioEngine::default();
        let temp = temp_dir("candidate-cache");
        let dir = temp.path().to_path_buf();
        let requested = dir.join("full.wav");
        let fallback = dir.join("full.ogg");
        std::fs::write(&requested, b"invalid").unwrap();
        std::fs::write(&fallback, b"valid").unwrap();
        let chart = chart_with_sounds(vec![
            SoundAssetRef { id: SoundId(1), path: requested.clone(), slice: None },
            SoundAssetRef { id: SoundId(2), path: requested.clone(), slice: None },
        ]);
        let mut loader = TestLoader::default();
        loader.failures.insert(requested.clone(), "decode failed".to_string());
        loader.samples.insert(
            fallback.clone(),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );

        let report = load_chart_samples(&mut engine, &chart, &mut loader);

        // 2件目は1件目で解決した候補列と decode 結果を使う。これは多数regionで
        // 同じpathを宣言する譜面でも stat/decodeを繰り返さないことを守る。
        assert!(matches!(report[0].status, LoadedSampleStatus::Loaded));
        assert!(matches!(report[1].status, LoadedSampleStatus::Loaded));
        assert_eq!(loader.attempts, vec![requested, fallback]);
        assert!(
            engine
                .samples
                .get(SoundId(1))
                .unwrap()
                .shares_source_with(engine.samples.get(SoundId(2)).unwrap())
        );
    }

    #[test]
    fn loads_two_thousand_regions_from_one_shared_source() {
        let mut engine = AudioEngine::new(1_000);
        let temp = temp_dir("many-shared-regions");
        let dir = temp.path().to_path_buf();
        let path = dir.join("long-source.wav");
        std::fs::write(&path, b"dummy").unwrap();
        let chart = chart_with_sounds(
            (0..2_000)
                .map(|index| SoundAssetRef {
                    id: SoundId(index),
                    path: path.clone(),
                    slice: Some(SoundSlice {
                        start_us: u64::from(index) * 1_000,
                        duration_us: Some(1_000),
                    }),
                })
                .collect(),
        );
        let mut loader = TestLoader::default();
        loader.samples.insert(
            path.clone(),
            DecodedSample {
                channels: 1,
                sample_rate: 1_000,
                frames: (0..2_000).map(|value| value as f32 / 2_000.0).collect(),
            },
        );

        let reports = load_chart_samples(&mut engine, &chart, &mut loader);

        assert_eq!(reports.len(), 2_000);
        assert_eq!(loader.attempts, vec![path]);
        assert_eq!(engine.samples.source_count(), 1);
        assert_eq!(engine.samples.region_count(), 2_000);
        let first = engine.samples.get(SoundId(0)).unwrap();
        let last = engine.samples.get(SoundId(1_999)).unwrap();
        assert!(first.shares_source_with(last));
        assert_eq!(first.frame_count(), 1);
        assert_eq!(last.frame_count(), 1);
        assert_eq!(first.sample_stereo(0), (0.0, 0.0));
        assert_eq!(last.sample_stereo(0), (0.9995, 0.9995));
    }

    #[test]
    fn wav_loader_decodes_pcm16_mono() {
        let path = write_temp_wav(&[
            wav_header(1, 1, 44_100, 16, 4).as_slice(),
            &[0x00, 0x00, 0xff, 0x7f],
        ]);
        let mut loader = WavSampleLoader;

        let sample = loader.load(&path).unwrap();

        assert_eq!(sample.channels, 1);
        assert_eq!(sample.sample_rate, 44_100);
        assert_eq!(sample.frames.len(), 2);
        assert_eq!(sample.frames[0], 0.0);
        assert!((sample.frames[1] - 0.9999695).abs() < 0.00001);
    }

    #[test]
    fn wav_loader_decodes_float32_stereo() {
        let mut data = Vec::new();
        data.extend_from_slice(&0.25_f32.to_le_bytes());
        data.extend_from_slice(&(-0.5_f32).to_le_bytes());
        let path =
            write_temp_wav(&[wav_header(3, 2, 48_000, 32, data.len() as u32).as_slice(), &data]);
        let mut loader = WavSampleLoader;

        let sample = loader.load(&path).unwrap();

        assert_eq!(sample.channels, 2);
        assert_eq!(sample.sample_stereo(0), (0.25, -0.5));
    }

    #[test]
    fn sound_asset_candidates_follow_beatoraja_audio_order() {
        let temp = temp_dir("audio-order");
        let dir = temp.path().to_path_buf();

        // #WAV では foo.wav を指定するが、実体は複数候補。
        let requested = dir.join("foo.wav");
        let flac = dir.join("foo.flac");
        let ogg = dir.join("foo.ogg");
        let mp3 = dir.join("foo.mp3");
        std::fs::write(&ogg, b"dummy").unwrap();
        std::fs::write(&flac, b"dummy").unwrap();
        std::fs::write(&mp3, b"dummy").unwrap();

        assert_eq!(sound_asset_candidates(&requested), vec![flac, ogg, mp3]);
    }

    #[test]
    fn float_wav_loaders_repair_pcm_before_resampling() {
        let values = [1.36f32, 6405.997, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.2];
        let data = values.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>();
        let path =
            write_temp_wav(&[wav_header(3, 1, 24_000, 32, data.len() as u32).as_slice(), &data]);
        for mut loader in [
            Box::new(WavSampleLoader) as Box<dyn SampleLoader>,
            Box::new(crate::ffmpeg_loader::FfmpegSampleLoader::default()),
        ] {
            let sample = loader.load(&path).unwrap();
            assert_eq!(sample.frames, [1.36, 0.0, 0.0, 0.0, 0.0, -1.2]);
            let resampled = sample.resampled_to(48_000);
            assert!(resampled.frames.iter().all(|v| v.is_finite() && v.abs() <= 1.36));
        }
    }

    #[test]
    fn sound_asset_candidates_keep_declared_file_first() {
        let temp = temp_dir("declared-first");
        let dir = temp.path().to_path_buf();

        let requested = dir.join("foo.ogg");
        let wav = dir.join("foo.wav");
        std::fs::write(&requested, b"dummy").unwrap();
        std::fs::write(&wav, b"dummy").unwrap();

        assert_eq!(sound_asset_candidates(&requested), vec![requested, wav]);
    }

    #[test]
    fn load_chart_samples_retries_decoding_candidates_in_order() {
        let mut engine = AudioEngine::default();
        let temp = temp_dir("decode-retry");
        let dir = temp.path().to_path_buf();
        let requested = dir.join("foo.wav");
        let fallback = dir.join("foo.flac");
        std::fs::write(&requested, b"invalid").unwrap();
        std::fs::write(&fallback, b"valid").unwrap();
        let chart = chart_with_sounds(vec![SoundAssetRef {
            id: SoundId(1),
            path: requested.clone(),
            slice: None,
        }]);
        let mut loader = TestLoader::default();
        loader.failures.insert(requested.clone(), "decode failed".to_string());
        loader.samples.insert(
            fallback.clone(),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );

        let report = load_chart_samples(&mut engine, &chart, &mut loader);

        assert_eq!(loader.attempts, vec![requested, fallback.clone()]);
        assert_eq!(report[0].path, fallback);
        assert!(matches!(report[0].status, LoadedSampleStatus::Loaded));
        assert!(engine.samples.get(SoundId(1)).is_some());
    }

    fn chart_with_sounds(sounds: Vec<SoundAssetRef>) -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"samples"),
            metadata: ChartMetadata {
                title: "samples".to_string(),
                initial_bpm: 120.0,
                ..Default::default()
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
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds,
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }

    fn temp_dir(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new().prefix(&format!("bmz-audio-{label}-")).tempdir().unwrap()
    }

    fn wav_header(
        audio_format: u16,
        channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
        data_len: u32,
    ) -> Vec<u8> {
        let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
        let block_align = channels * bits_per_sample / 8;
        let riff_len = 36 + data_len;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&riff_len.to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&audio_format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits_per_sample.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out
    }

    /// Each call gets its own file; the name used to be derived from the byte length, so
    /// two tests writing different WAVs of equal size overwrote each other. The handle is
    /// closed (Windows decoders may reopen the file) and the file is removed on drop.
    fn write_temp_wav(parts: &[&[u8]]) -> tempfile::TempPath {
        let mut file =
            tempfile::Builder::new().prefix("bmz-audio-wav-").suffix(".wav").tempfile().unwrap();
        for part in parts {
            std::io::Write::write_all(&mut file, part).unwrap();
        }
        file.into_temp_path()
    }
}
