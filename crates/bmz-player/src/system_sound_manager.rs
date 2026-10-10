//! [`SystemSoundManager`] は [`crate::system_sound`] が決定したサウンドセットを
//! デコードして system audio command handle に登録し、各 [`SoundType`] を SE / BGM として
//! 再生・停止する beatoraja の `SystemSoundManager` 相当 facade。
//!
//! - 各音種を `FfmpegSampleLoader` でデコードし、個別の失敗時は次の補完候補を試す。
//! - SoundId は chart のキー音(BMS `#WAVxx` は base-36 で最大 1296 個)と衝突しないよう
//!   [`SYSTEM_SOUND_BASE`] (= 100_000) からの連番を予約する。`SampleBank` は
//!   `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、`u32::MAX` 付近の
//!   巨大 ID を使うと resize が数十 GB の allocation を試みて OOM kill される。
//! - 再生種別とループ指定を分離し、RESULT BGMの`.loop`指定を準備結果に保持する。

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, UNIX_EPOCH};

use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_audio::loudness::{
    LoudnessAnalysis, analyze_decoded_loudness, system_bgm_normalization_gain_for_analysis,
};
use bmz_audio::sample::DecodedSample;
use bmz_core::ids::SoundId;
use serde::{Deserialize, Serialize};

use crate::system_sound::{ResolvedSystemSound, SoundSetSelection, SoundType};

/// chart 側のキー音 SoundId と衝突しないよう確保する予約レンジの先頭。
/// `SampleBank` は `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、
/// 大きすぎる値を使うと巨大な resize が走り OOM kill される。
/// BMS の `#WAVxx` は base-36 で最大 1296 個なので、100_000 オフセットなら衝突しない。
const SYSTEM_SOUND_BASE: u32 = 100_000;
const VOLUME_EPSILON: f32 = 0.000_1;
const MAX_SCRATCH_VOICES: usize = 3;
const SYSTEM_BGM_LOUDNESS_CACHE_FILE: &str = "system-bgm-loudness-v1.json";
const SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION: u32 = 1;
const SYSTEM_BGM_LOUDNESS_ANALYSIS_VERSION: u32 = 2;
const MAX_SYSTEM_BGM_LOUDNESS_CACHE_ENTRIES: usize = 256;
static SYSTEM_BGM_LOUDNESS_CACHE_IO: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemSoundPrepareStats {
    pub decoded_count: usize,
    pub cache_hit_count: usize,
    pub analysis_count: usize,
    pub decode_ms: u64,
    pub analysis_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug)]
pub struct PreparedSystemSoundSet {
    pub(crate) normalization_paths: HashMap<SoundType, PathBuf>,
    normalization_keys: HashMap<SoundType, LoudnessCacheKey>,
    source_fingerprint: Option<SystemSoundSourceFingerprint>,
    samples: Vec<(SoundType, SoundId, DecodedSample)>,
    looping_sounds: HashSet<SoundType>,
    bgm_normalization_gains: HashMap<SoundType, f32>,
    pub normalization_analysis_enabled: bool,
    pub(crate) reused_existing_sound_set: bool,
    pub stats: SystemSoundPrepareStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemSoundSourceFingerprint {
    selection: SoundSetSelection,
    output_sample_rate: u32,
    candidates: Vec<Vec<SystemSoundCandidateFingerprint>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SystemSoundCandidateFingerprint {
    path: String,
    file_len: u64,
    modified_ns: u64,
    loop_playback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LoudnessCacheKey {
    path: String,
    file_len: u64,
    modified_ns: u64,
    analysis_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoudnessCacheEntry {
    key: LoudnessCacheKey,
    loudness_lufs: f32,
    short_term_lufs: f32,
    peak_abs: f32,
}

impl LoudnessCacheEntry {
    fn analysis(&self) -> Option<LoudnessAnalysis> {
        let analysis = LoudnessAnalysis {
            loudness_lufs: self.loudness_lufs,
            short_term_lufs: self.short_term_lufs,
            peak_abs: self.peak_abs,
        };
        (analysis.loudness_lufs.is_finite()
            && analysis.short_term_lufs.is_finite()
            && analysis.peak_abs.is_finite())
        .then_some(analysis)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LoudnessCacheFile {
    version: u32,
    entries: Vec<LoudnessCacheEntry>,
}

pub struct SystemSoundManager {
    normalization_paths: HashMap<SoundType, PathBuf>,
    normalization_keys: HashMap<SoundType, LoudnessCacheKey>,
    source_fingerprint: Option<SystemSoundSourceFingerprint>,
    engine: AudioEngineHandle,
    id_map: HashMap<SoundType, SoundId>,
    looping_sounds: HashSet<SoundType>,
    last_volumes: RefCell<HashMap<SoundType, f32>>,
    master_gain: Cell<f32>,
    bgm_normalization_gains: HashMap<SoundType, f32>,
    normalize_bgm_volume: Cell<bool>,
    normalization_analysis_enabled: bool,
    gameplay_se_volume: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

#[derive(Clone)]
pub struct GameplaySoundOutput {
    engine: AudioEngineHandle,
    ids: HashMap<SoundType, SoundId>,
    volume: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl GameplaySoundOutput {
    pub fn bind_play(&mut self, cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.engine = self.engine.for_play(cancelled);
    }
    pub fn play(&self, kind: SoundType) {
        if let Some(&id) = self.ids.get(&kind) {
            let volume = f32::from_bits(self.volume.load(std::sync::atomic::Ordering::Relaxed));
            self.engine.set_master_gain(1.0);
            self.engine.play_now(id, volume, false);
        }
    }
}

impl SystemSoundManager {
    pub fn gameplay_output(&self, volume: f32) -> GameplaySoundOutput {
        self.gameplay_se_volume.store(volume.to_bits(), std::sync::atomic::Ordering::Relaxed);
        GameplaySoundOutput {
            engine: self.engine.clone(),
            ids: self.id_map.clone(),
            volume: self.gameplay_se_volume.clone(),
        }
    }
    /// `selection` から各 [`SoundType`] のパスを解決し、デコードして engine へ登録する。
    /// 解決失敗は info、デコード失敗は warn をサウンド単位で出してスキップする。
    pub fn new(
        engine: AudioEngineHandle,
        selection: &SoundSetSelection,
        normalize_bgm_volume: bool,
        cache_dir: Option<&Path>,
    ) -> Self {
        let output_sample_rate = engine.output_sample_rate();
        let prepared =
            Self::prepare(selection, normalize_bgm_volume, output_sample_rate, cache_dir);
        Self::from_prepared(engine, prepared, normalize_bgm_volume)
    }

    /// ファイルI/O、decode、loudness解析、出力レート化を行うworker向け処理。
    /// AudioEngineへの登録は [`Self::from_prepared`] でapp threadから行う。
    pub fn prepare(
        selection: &SoundSetSelection,
        normalize_bgm_volume: bool,
        output_sample_rate: u32,
        cache_dir: Option<&Path>,
    ) -> PreparedSystemSoundSet {
        Self::prepare_with_source_reuse(
            selection,
            normalize_bgm_volume,
            output_sample_rate,
            cache_dir,
            None,
        )
    }

    /// Reuses the applied sound set when a Select return randomly chooses the same unchanged
    /// files. Fingerprinting and any required decoding stay on the worker thread.
    pub(crate) fn prepare_with_source_reuse(
        selection: &SoundSetSelection,
        normalize_bgm_volume: bool,
        output_sample_rate: u32,
        cache_dir: Option<&Path>,
        previous_fingerprint: Option<&SystemSoundSourceFingerprint>,
    ) -> PreparedSystemSoundSet {
        let total_started_at = Instant::now();
        let candidates_by_type = selection.candidates_for_all();
        let source_fingerprint =
            system_sound_source_fingerprint(selection, output_sample_rate, &candidates_by_type);
        if source_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| previous_fingerprint == Some(fingerprint))
        {
            return PreparedSystemSoundSet {
                normalization_paths: HashMap::new(),
                normalization_keys: HashMap::new(),
                source_fingerprint,
                samples: Vec::new(),
                looping_sounds: HashSet::new(),
                bgm_normalization_gains: HashMap::new(),
                normalization_analysis_enabled: normalize_bgm_volume,
                reused_existing_sound_set: true,
                stats: SystemSoundPrepareStats {
                    total_ms: elapsed_ms_u64(total_started_at),
                    ..Default::default()
                },
            };
        }
        let mut bgm_normalization_gains = HashMap::new();
        let mut normalization_paths = HashMap::new();
        let mut normalization_keys = HashMap::new();
        let mut loader = FfmpegSampleLoader::default();
        let mut samples = Vec::new();
        let mut looping_sounds = HashSet::new();
        let mut stats = SystemSoundPrepareStats::default();
        let cache_path = cache_dir.map(|dir| dir.join(SYSTEM_BGM_LOUDNESS_CACHE_FILE));
        let mut cache = cache_path.as_deref().map(load_loudness_cache).unwrap_or_default();
        let mut cache_updates = Vec::new();

        for (i, sound_type) in SoundType::ALL.iter().enumerate() {
            let id = SoundId(SYSTEM_SOUND_BASE + i as u32);
            let candidates = &candidates_by_type[i];
            if candidates.is_empty() {
                tracing::info!(
                    sound_type = ?sound_type,
                    file_name = sound_type.file_name(),
                    "system sound file not found in selected set or default dir; skipping"
                );
                continue;
            }
            for candidate in candidates {
                let path = candidate.path.clone();
                let decode_started_at = Instant::now();
                match loader.load(&path) {
                    Ok(sample) => {
                        let decode_ms = elapsed_ms_u64(decode_started_at);
                        stats.decode_ms = stats.decode_ms.saturating_add(decode_ms);
                        stats.decoded_count = stats.decoded_count.saturating_add(1);
                        if sound_type.is_bgm() {
                            let key = loudness_cache_key(&path);
                            normalization_paths.insert(*sound_type, path.clone());
                            if let Some(key) = key.as_ref() {
                                normalization_keys.insert(*sound_type, key.clone());
                            }
                            if normalize_bgm_volume {
                                let (analysis, cache_hit) = bgm_loudness_with_cache(
                                    &sample,
                                    key,
                                    &mut cache,
                                    &mut cache_updates,
                                    &mut stats,
                                );
                                if let Some(analysis) = analysis {
                                    let gain = system_bgm_normalization_gain_for_analysis(analysis);
                                    tracing::debug!(
                                        sound_type = ?sound_type,
                                        path = %path.display(),
                                        loudness_lufs = analysis.loudness_lufs,
                                        short_term_lufs = analysis.short_term_lufs,
                                        sample_peak = analysis.peak_abs,
                                        normalization_gain = gain,
                                        cache_hit,
                                        decode_ms,
                                        "prepared system BGM loudness"
                                    );
                                    bgm_normalization_gains.insert(*sound_type, gain);
                                }
                            }
                        }
                        let sample = if sample.sample_rate == output_sample_rate {
                            sample
                        } else {
                            sample.resampled_to(output_sample_rate)
                        };
                        samples.push((*sound_type, id, sample));
                        if candidate.loop_playback {
                            looping_sounds.insert(*sound_type);
                        }
                        break;
                    }
                    Err(error) => {
                        let decode_ms = elapsed_ms_u64(decode_started_at);
                        stats.decode_ms = stats.decode_ms.saturating_add(decode_ms);
                        tracing::warn!(
                            sound_type = ?sound_type,
                            path = %path.display(),
                            decode_ms,
                            %error,
                            "failed to decode system sound; trying next candidate"
                        );
                    }
                }
            }
        }

        if !cache_updates.is_empty()
            && let Some(path) = cache_path.as_deref()
        {
            save_loudness_cache(path, &cache_updates);
        }
        stats.total_ms = elapsed_ms_u64(total_started_at);
        PreparedSystemSoundSet {
            normalization_paths,
            normalization_keys,
            source_fingerprint,
            samples,
            looping_sounds,
            bgm_normalization_gains,
            normalization_analysis_enabled: normalize_bgm_volume,
            reused_existing_sound_set: false,
            stats,
        }
    }

    pub fn from_prepared(
        engine: AudioEngineHandle,
        prepared: PreparedSystemSoundSet,
        normalize_bgm_volume: bool,
    ) -> Self {
        let mut id_map = HashMap::new();
        let commands = prepared
            .samples
            .into_iter()
            .map(|(sound_type, id, sample)| {
                id_map.insert(sound_type, id);
                AudioEngineCommand::InsertPreparedSample { id, sample }
            })
            .collect::<Vec<_>>();
        if !commands.is_empty() && !engine.push_commands(commands) {
            tracing::warn!("failed to enqueue decoded system sounds");
        }

        let mut manager = Self::with_id_map_and_normalization_gains(
            engine,
            id_map,
            prepared.bgm_normalization_gains,
            normalize_bgm_volume,
            prepared.normalization_analysis_enabled,
        );
        manager.normalization_paths = prepared.normalization_paths;
        manager.normalization_keys = prepared.normalization_keys;
        manager.source_fingerprint = prepared.source_fingerprint;
        manager.looping_sounds = prepared.looping_sounds;
        manager
    }

    #[cfg(test)]
    pub(crate) fn with_id_map(
        engine: AudioEngineHandle,
        id_map: HashMap<SoundType, SoundId>,
    ) -> Self {
        Self::with_id_map_and_normalization_gains(engine, id_map, HashMap::new(), false, false)
    }

    fn with_id_map_and_normalization_gains(
        engine: AudioEngineHandle,
        id_map: HashMap<SoundType, SoundId>,
        bgm_normalization_gains: HashMap<SoundType, f32>,
        normalize_bgm_volume: bool,
        normalization_analysis_enabled: bool,
    ) -> Self {
        Self {
            normalization_paths: HashMap::new(),
            normalization_keys: HashMap::new(),
            source_fingerprint: None,
            engine,
            looping_sounds: id_map.keys().copied().filter(SoundType::loops).collect(),
            id_map,
            last_volumes: RefCell::new(HashMap::new()),
            master_gain: Cell::new(1.0),
            bgm_normalization_gains,
            normalize_bgm_volume: Cell::new(normalize_bgm_volume),
            normalization_analysis_enabled,
            gameplay_se_volume: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(
                1.0f32.to_bits(),
            )),
        }
    }

    pub fn set_bgm_normalization_enabled(&self, enabled: bool) {
        self.normalize_bgm_volume.set(enabled);
    }

    pub fn normalization_analysis_enabled(&self) -> bool {
        self.normalization_analysis_enabled
    }

    pub fn normalization_paths(&self) -> HashMap<SoundType, PathBuf> {
        self.normalization_paths.clone()
    }

    pub fn normalization_source_matches(&self, prepared: &PreparedSystemSoundSet) -> bool {
        self.normalization_paths == prepared.normalization_paths
            && self.normalization_keys == prepared.normalization_keys
    }

    pub(crate) fn source_fingerprint(&self) -> Option<&SystemSoundSourceFingerprint> {
        self.source_fingerprint.as_ref()
    }

    /// 適用済みサンプルのBGMパスだけをデコードして解析するworker向け処理。
    pub fn prepare_normalization_for_paths(
        paths: &HashMap<SoundType, PathBuf>,
        cache_dir: Option<&Path>,
    ) -> PreparedSystemSoundSet {
        let started_at = Instant::now();
        let cache_path = cache_dir.map(|dir| dir.join(SYSTEM_BGM_LOUDNESS_CACHE_FILE));
        let mut cache = cache_path.as_deref().map(load_loudness_cache).unwrap_or_default();
        let mut cache_updates = Vec::new();
        let mut gains = HashMap::new();
        let mut normalization_keys = HashMap::new();
        let mut stats = SystemSoundPrepareStats::default();
        let mut loader = FfmpegSampleLoader::default();
        for (sound_type, path) in paths {
            if !sound_type.is_bgm() {
                continue;
            }
            let decode_started_at = Instant::now();
            let sample = match loader.load(path) {
                Ok(sample) => sample,
                Err(error) => {
                    tracing::warn!(sound_type = ?sound_type, path = %path.display(), %error, "failed to decode selected system BGM for normalization");
                    continue;
                }
            };
            stats.decode_ms = stats.decode_ms.saturating_add(elapsed_ms_u64(decode_started_at));
            stats.decoded_count += 1;
            let key = loudness_cache_key(path);
            if let Some(key) = key.as_ref() {
                normalization_keys.insert(*sound_type, key.clone());
            }
            let (analysis, _) =
                bgm_loudness_with_cache(&sample, key, &mut cache, &mut cache_updates, &mut stats);
            if let Some(analysis) = analysis {
                gains.insert(*sound_type, system_bgm_normalization_gain_for_analysis(analysis));
            }
        }
        if !cache_updates.is_empty()
            && let Some(path) = cache_path.as_deref()
        {
            save_loudness_cache(path, &cache_updates);
        }
        stats.total_ms = elapsed_ms_u64(started_at);
        PreparedSystemSoundSet {
            normalization_paths: paths.clone(),
            normalization_keys,
            source_fingerprint: None,
            samples: Vec::new(),
            looping_sounds: HashSet::new(),
            bgm_normalization_gains: gains,
            normalization_analysis_enabled: true,
            reused_existing_sound_set: false,
            stats,
        }
    }

    /// 現在の登録サンプルと再生状態を維持し、選択済みセットの解析ゲインだけを反映する。
    pub fn apply_normalization_analysis(&mut self, prepared: PreparedSystemSoundSet) {
        self.bgm_normalization_gains = prepared.bgm_normalization_gains;
        self.normalization_analysis_enabled = prepared.normalization_analysis_enabled;
    }

    /// 指定音種を、準備時に解決したループ設定で再生する。SEは単発。
    /// 対応サンプルが登録されていない場合は何もしない。
    pub fn play(&self, sound_type: SoundType, master_volume: f32) {
        self.play_with_master_gain(sound_type, master_volume, self.master_gain.get());
    }

    /// マスターゲイン復帰と再生を 1 回の AudioEngine lock にまとめる。
    pub fn play_with_master_gain(&self, sound_type: SoundType, master_volume: f32, gain: f32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        let master_volume = self.effective_volume(sound_type, master_volume);
        let gain = normalize_volume(gain);
        let loop_playback = self.looping_sounds.contains(&sound_type);
        let mut commands = vec![AudioEngineCommand::SetMasterGain { gain }];
        if sound_type.is_bgm() {
            commands.push(AudioEngineCommand::StopSound { id });
        }
        commands.push(if sound_type == SoundType::Scratch {
            AudioEngineCommand::PlayNowWithVoiceLimit {
                sound_id: id,
                volume: master_volume,
                loop_playback,
                max_voices: MAX_SCRATCH_VOICES,
            }
        } else {
            AudioEngineCommand::PlayNow { sound_id: id, volume: master_volume, loop_playback }
        });
        if self.engine.push_commands(commands) {
            self.master_gain.set(gain);
            self.last_volumes.borrow_mut().insert(sound_type, master_volume);
        }
    }

    pub fn play_with_master_gain_and_fade_out(
        &self,
        sound_type: SoundType,
        master_volume: f32,
        gain: f32,
        fade_out_frames: u32,
    ) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        let master_volume = self.effective_volume(sound_type, master_volume);
        let gain = normalize_volume(gain);
        let commands = vec![
            AudioEngineCommand::SetMasterGain { gain },
            AudioEngineCommand::PlayNowWithFadeInAndFadeOut {
                sound_id: id,
                volume: master_volume,
                loop_playback: self.looping_sounds.contains(&sound_type),
                fade_in_frames: 0,
                fade_out_frames,
            },
        ];
        if self.engine.push_commands(commands) {
            self.master_gain.set(gain);
            self.last_volumes.borrow_mut().insert(sound_type, master_volume);
        }
    }

    pub fn has_sound(&self, sound_type: SoundType) -> bool {
        self.id_map.contains_key(&sound_type)
    }

    /// 登録済み sound の再生待ち/再生中音量を、SoundType ごとの最新設定で更新する。
    pub fn refresh_volumes(&self, mut volume_for: impl FnMut(SoundType) -> f32) {
        self.gameplay_se_volume
            .store(volume_for(SoundType::Landmine).to_bits(), std::sync::atomic::Ordering::Relaxed);
        let mut updates = Vec::new();
        {
            let last_volumes = self.last_volumes.borrow();
            for (&sound_type, &id) in &self.id_map {
                let volume = self.effective_volume(sound_type, volume_for(sound_type));
                if last_volumes.get(&sound_type).is_none_or(|&last| !volume_matches(last, volume)) {
                    updates.push((sound_type, id, volume));
                }
            }
        }
        if updates.is_empty() {
            return;
        }

        let commands = updates
            .iter()
            .map(|&(_, id, volume)| AudioEngineCommand::SetSoundVolume { id, volume })
            .collect::<Vec<_>>();
        if !self.engine.push_commands(commands) {
            return;
        }
        let mut last_volumes = self.last_volumes.borrow_mut();
        for (sound_type, _, volume) in updates {
            last_volumes.insert(sound_type, volume);
        }
    }

    /// 指定 SoundType の再生待ち/再生中音量を直接更新する。
    pub fn set_volume(&self, sound_type: SoundType, volume: f32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        let volume = self.effective_volume(sound_type, volume);
        if self
            .last_volumes
            .borrow()
            .get(&sound_type)
            .is_some_and(|&last| volume_matches(last, volume))
        {
            return;
        }
        if self.engine.set_sound_volume(id, volume) {
            self.last_volumes.borrow_mut().insert(sound_type, volume);
        }
    }

    /// システム音 engine 全体のマスターゲインを更新する。
    /// リザルト退出時の `ResultClose` など、複数のシステム音をまとめて
    /// フェードアウトさせる用途で使う。
    pub fn set_master_gain(&self, gain: f32) {
        let gain = normalize_volume(gain);
        if volume_matches(self.master_gain.get(), gain) {
            return;
        }
        if self.engine.set_master_gain(gain) {
            self.master_gain.set(gain);
        }
    }

    /// 指定 SoundType を停止する。鳴っていなくても害は無い。
    pub fn stop(&self, sound_type: SoundType) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        self.engine.stop_sound(id);
    }

    pub fn stop_with_fade_out(&self, sound_type: SoundType, fade_out_frames: u32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        self.engine.stop_sound_with_fade_out(id, fade_out_frames);
    }

    /// 登録済みかつ `is_bgm()` の SoundType をすべて停止する。
    pub fn stop_all_bgm(&self) {
        let commands = SoundType::ALL
            .iter()
            .filter(|t| t.is_bgm())
            .filter_map(|sound_type| self.id_map.get(sound_type).copied())
            .map(|id| AudioEngineCommand::StopSound { id })
            .collect::<Vec<_>>();
        self.engine.push_commands(commands);
    }

    fn effective_volume(&self, sound_type: SoundType, volume: f32) -> f32 {
        let normalization_gain = if self.normalize_bgm_volume.get() && sound_type.is_bgm() {
            self.bgm_normalization_gains.get(&sound_type).copied().unwrap_or(1.0)
        } else {
            1.0
        };
        normalize_volume(volume * normalization_gain)
    }
}

fn normalize_volume(volume: f32) -> f32 {
    if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 }
}

fn volume_matches(left: f32, right: f32) -> bool {
    (left - right).abs() <= VOLUME_EPSILON
}

fn loudness_cache_key(path: &Path) -> Option<LoudnessCacheKey> {
    let metadata = path.metadata().ok()?;
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(duration_ns_u64)
        .unwrap_or_default();
    Some(LoudnessCacheKey {
        path: cache_path_text(path),
        file_len: metadata.len(),
        modified_ns,
        analysis_version: SYSTEM_BGM_LOUDNESS_ANALYSIS_VERSION,
    })
}

/// キャッシュ済みの解析結果を再利用し、無ければdecode済みsampleを解析する。
/// 新しい解析結果は `cache` と保存用の `cache_updates` に追加する。戻り値の `bool` はキャッシュヒット。
fn bgm_loudness_with_cache(
    sample: &DecodedSample,
    key: Option<LoudnessCacheKey>,
    cache: &mut LoudnessCacheFile,
    cache_updates: &mut Vec<LoudnessCacheEntry>,
    stats: &mut SystemSoundPrepareStats,
) -> (Option<LoudnessAnalysis>, bool) {
    let cached = key.as_ref().and_then(|key| {
        cache.entries.iter().find(|entry| entry.key == *key).and_then(LoudnessCacheEntry::analysis)
    });
    if let Some(analysis) = cached {
        stats.cache_hit_count = stats.cache_hit_count.saturating_add(1);
        return (Some(analysis), true);
    }
    let analysis_started_at = Instant::now();
    let analysis = analyze_decoded_loudness(sample);
    stats.analysis_ms = stats.analysis_ms.saturating_add(elapsed_ms_u64(analysis_started_at));
    stats.analysis_count = stats.analysis_count.saturating_add(1);
    if let (Some(key), Some(analysis)) = (key, analysis) {
        cache.entries.retain(|entry| entry.key.path != key.path);
        let entry = LoudnessCacheEntry {
            key,
            loudness_lufs: analysis.loudness_lufs,
            short_term_lufs: analysis.short_term_lufs,
            peak_abs: analysis.peak_abs,
        };
        cache.entries.push(entry.clone());
        cache_updates.push(entry);
    }
    (analysis, false)
}

fn system_sound_source_fingerprint(
    selection: &SoundSetSelection,
    output_sample_rate: u32,
    candidates: &[Vec<ResolvedSystemSound>],
) -> Option<SystemSoundSourceFingerprint> {
    let candidates = candidates
        .iter()
        .map(|candidates| {
            candidates
                .iter()
                .map(|candidate| {
                    let key = loudness_cache_key(&candidate.path)?;
                    Some(SystemSoundCandidateFingerprint {
                        path: key.path,
                        file_len: key.file_len,
                        modified_ns: key.modified_ns,
                        loop_playback: candidate.loop_playback,
                    })
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(SystemSoundSourceFingerprint {
        selection: selection.clone(),
        output_sample_rate,
        candidates,
    })
}

fn cache_path_text(path: &Path) -> String {
    path.canonicalize().unwrap_or_else(|_| PathBuf::from(path)).to_string_lossy().into_owned()
}

fn load_loudness_cache(path: &Path) -> LoudnessCacheFile {
    let _cache_guard = system_bgm_loudness_cache_io_guard();
    read_loudness_cache(path)
}

fn read_loudness_cache(path: &Path) -> LoudnessCacheFile {
    let Ok(text) = std::fs::read_to_string(path) else {
        return LoudnessCacheFile {
            version: SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION,
            entries: Vec::new(),
        };
    };
    match serde_json::from_str::<LoudnessCacheFile>(&text) {
        Ok(cache) if cache.version == SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION => cache,
        Ok(_) => LoudnessCacheFile {
            version: SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION,
            entries: Vec::new(),
        },
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "ignored invalid system BGM loudness cache");
            LoudnessCacheFile {
                version: SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION,
                entries: Vec::new(),
            }
        }
    }
}

fn save_loudness_cache(path: &Path, updates: &[LoudnessCacheEntry]) {
    let _cache_guard = system_bgm_loudness_cache_io_guard();
    let mut cache = read_loudness_cache(path);
    cache.version = SYSTEM_BGM_LOUDNESS_CACHE_FORMAT_VERSION;
    for update in updates {
        cache.entries.retain(|entry| entry.key.path != update.key.path);
        cache.entries.push(update.clone());
    }
    if cache.entries.len() > MAX_SYSTEM_BGM_LOUDNESS_CACHE_ENTRIES {
        cache
            .entries
            .drain(..cache.entries.len().saturating_sub(MAX_SYSTEM_BGM_LOUDNESS_CACHE_ENTRIES));
    }
    let result = serde_json::to_vec(&cache)
        .map_err(anyhow::Error::from)
        .and_then(|bytes| std::fs::write(path, bytes).map_err(anyhow::Error::from));
    if let Err(error) = result {
        tracing::warn!(%error, path = %path.display(), "failed to save system BGM loudness cache");
    }
}

fn system_bgm_loudness_cache_io_guard() -> std::sync::MutexGuard<'static, ()> {
    SYSTEM_BGM_LOUDNESS_CACHE_IO
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn duration_ns_u64(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn elapsed_ms_u64(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use bmz_audio::command::CommandedAudioEngine;
    use bmz_audio::engine::AudioEngine;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn new_succeeds_with_empty_selection_and_registers_no_samples() {
        // どのファイルも resolve できない Selection を渡してもパニックせず空 manager を返すこと。
        let (engine, _processor) = test_engine();
        let selection = SoundSetSelection::default();

        let manager = SystemSoundManager::new(engine, &selection, false, None);

        assert!(manager.id_map.is_empty());
        // 未登録の SoundType の play / stop は no-op で問題ないこと。
        manager.play(SoundType::Scratch, 1.0);
        manager.stop(SoundType::Select);
        manager.stop_all_bgm();
    }

    #[test]
    fn prepare_skips_disabled_analysis_and_reuses_valid_cache() {
        let root = test_temp_dir("loudness-cache");
        std::fs::create_dir_all(&root).unwrap();
        let select = root.join("select.wav");
        write_test_wav(&select, 48_000);
        let selection = SoundSetSelection {
            bgm_dir: Some(root.clone()),
            bgm_variant_dir: None,
            se_dir: None,
            default_dir: None,
        };

        let disabled = SystemSoundManager::prepare(&selection, false, 48_000, Some(&root));
        assert_eq!(disabled.stats.analysis_count, 0);
        assert_eq!(disabled.stats.cache_hit_count, 0);
        assert!(!disabled.normalization_analysis_enabled);
        assert!(!root.join(SYSTEM_BGM_LOUDNESS_CACHE_FILE).exists());

        let first = SystemSoundManager::prepare(&selection, true, 48_000, Some(&root));
        assert_eq!(first.stats.analysis_count, 1);
        assert_eq!(first.stats.cache_hit_count, 0);
        assert!(root.join(SYSTEM_BGM_LOUDNESS_CACHE_FILE).is_file());

        let cached = SystemSoundManager::prepare(&selection, true, 48_000, Some(&root));
        assert_eq!(cached.stats.analysis_count, 0);
        assert_eq!(cached.stats.cache_hit_count, 1);

        write_test_wav(&select, 48_001);
        let changed = SystemSoundManager::prepare(&selection, true, 48_000, Some(&root));
        assert_eq!(changed.stats.analysis_count, 1);
        assert_eq!(changed.stats.cache_hit_count, 0);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loudness_cache_save_merges_parallel_worker_updates() {
        let root = test_temp_dir("parallel-loudness-cache");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(SYSTEM_BGM_LOUDNESS_CACHE_FILE);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let workers = ["sound-a.wav", "sound-b.wav"].map(|sound_path| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                // Both workers keep the same initial snapshot while analyzing different sounds.
                let snapshot = load_loudness_cache(&path);
                assert!(snapshot.entries.is_empty());
                barrier.wait();

                let update = LoudnessCacheEntry {
                    key: LoudnessCacheKey {
                        path: sound_path.to_owned(),
                        file_len: 128,
                        modified_ns: 256,
                        analysis_version: SYSTEM_BGM_LOUDNESS_ANALYSIS_VERSION,
                    },
                    loudness_lufs: -12.0,
                    short_term_lufs: -11.0,
                    peak_abs: 0.5,
                };
                save_loudness_cache(&path, &[update]);
            })
        });

        for worker in workers {
            worker.join().expect("cache worker should finish");
        }

        let cache = load_loudness_cache(&path);
        assert_eq!(cache.entries.len(), 2);
        assert!(cache.entries.iter().any(|entry| entry.key.path == "sound-a.wav"));
        assert!(cache.entries.iter().any(|entry| entry.key.path == "sound-b.wav"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_reuses_unchanged_selected_sound_sources() {
        let root = test_temp_dir("reuse-system-sound-set");
        std::fs::create_dir_all(&root).unwrap();
        write_test_wav(&root.join("select.wav"), 48_000);
        let result_bgm = root.join("clear.wav");
        write_test_wav(&result_bgm, 48_000);
        let selection = SoundSetSelection { bgm_dir: Some(root.clone()), ..Default::default() };

        let first = SystemSoundManager::prepare(&selection, false, 48_000, None);
        let fingerprint = first.source_fingerprint.as_ref().unwrap();
        let reused = SystemSoundManager::prepare_with_source_reuse(
            &selection,
            false,
            48_000,
            None,
            Some(fingerprint),
        );
        assert!(reused.reused_existing_sound_set);
        assert_eq!(reused.stats.decoded_count, 0);
        assert!(reused.samples.is_empty());

        let changed_rate = SystemSoundManager::prepare_with_source_reuse(
            &selection,
            false,
            44_100,
            None,
            Some(fingerprint),
        );
        assert!(!changed_rate.reused_existing_sound_set);

        write_test_wav(&result_bgm, 48_001);
        let changed = SystemSoundManager::prepare_with_source_reuse(
            &selection,
            false,
            48_000,
            None,
            Some(fingerprint),
        );
        assert!(!changed.reused_existing_sound_set);
        assert_eq!(changed.stats.decoded_count, 2);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "manual system sound loading measurement"]
    fn measure_long_result_bgm_decode_and_reselection_cost() {
        const DURATION_SECONDS: u32 = 60;
        let root = test_temp_dir("long-result-bgm-measurement");
        std::fs::create_dir_all(&root).unwrap();
        for sound_type in SoundType::RESULT_BGMS {
            write_stereo_test_wav(&root.join(sound_type.file_name()), DURATION_SECONDS);
        }
        let selection = SoundSetSelection { bgm_dir: Some(root.clone()), ..Default::default() };

        let first = SystemSoundManager::prepare(&selection, false, 48_000, None);
        let second = SystemSoundManager::prepare(&selection, false, 48_000, None);
        let reused = SystemSoundManager::prepare_with_source_reuse(
            &selection,
            false,
            48_000,
            None,
            first.source_fingerprint.as_ref(),
        );
        let pcm_bytes = |prepared: &PreparedSystemSoundSet| {
            prepared
                .samples
                .iter()
                .map(|(_, _, sample)| sample.frames.len() * std::mem::size_of::<f32>())
                .sum::<usize>()
        };
        let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
        println!(
            "{}-second stereo RESULT set: first decode={} ms, reselect decode={} ms, unchanged-source reuse decode={} ms, retained PCM={} MiB, old/new overlap without reuse={} MiB, reuse keeps={} MiB",
            DURATION_SECONDS,
            first.stats.decode_ms,
            second.stats.decode_ms,
            reused.stats.decode_ms,
            mib(pcm_bytes(&first)),
            mib(pcm_bytes(&first) + pcm_bytes(&second)),
            mib(pcm_bytes(&first) + pcm_bytes(&reused)),
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_prepare_can_analyze_only_loaded_bgms_and_reject_changed_sources() {
        let root = test_temp_dir("normalization-only-loaded-bgms");
        let bgm = root.join("bgm");
        let se = root.join("se");
        std::fs::create_dir_all(&bgm).unwrap();
        std::fs::create_dir_all(&se).unwrap();
        let select_path = bgm.join("select.wav");
        let result_bgm_path = bgm.join("clear.wav");
        write_test_wav(&select_path, 48_000);
        write_test_wav(&result_bgm_path, 48_000);
        write_test_wav(&se.join("clear.wav"), 48_000);
        let selection = SoundSetSelection {
            bgm_dir: Some(bgm),
            bgm_variant_dir: None,
            se_dir: Some(se),
            default_dir: None,
        };

        let disabled = SystemSoundManager::prepare(&selection, false, 48_000, None);
        assert!(!disabled.normalization_analysis_enabled);
        assert_eq!(disabled.stats.analysis_count, 0);
        assert_eq!(disabled.normalization_paths.get(&SoundType::Select), Some(&select_path));
        assert_eq!(
            disabled.normalization_paths.get(&SoundType::ResultBgmClear),
            Some(&result_bgm_path)
        );
        assert!(!disabled.normalization_paths.contains_key(&SoundType::ResultClear));

        let (engine, _processor) = test_engine();
        let mut manager = SystemSoundManager::from_prepared(engine, disabled, false);
        let normalization = SystemSoundManager::prepare_normalization_for_paths(
            &manager.normalization_paths(),
            Some(&root),
        );
        assert_eq!(normalization.stats.decoded_count, 2);
        assert_eq!(normalization.stats.analysis_count, 2);
        assert!(!normalization.bgm_normalization_gains.contains_key(&SoundType::ResultClear));
        assert!(manager.normalization_source_matches(&normalization));

        manager.apply_normalization_analysis(normalization);
        assert!(manager.normalization_analysis_enabled());
        assert!(manager.bgm_normalization_gains.contains_key(&SoundType::Select));
        assert!(manager.bgm_normalization_gains.contains_key(&SoundType::ResultBgmClear));
        assert!(!manager.bgm_normalization_gains.contains_key(&SoundType::ResultClear));

        write_test_wav(&select_path, 48_001);
        let replaced = SystemSoundManager::prepare_normalization_for_paths(
            &manager.normalization_paths(),
            Some(&root),
        );
        assert!(!manager.normalization_source_matches(&replaced));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_resamples_system_sounds_to_output_rate() {
        let root = test_temp_dir("output-rate");
        std::fs::create_dir_all(&root).unwrap();
        let select = root.join("select.wav");
        write_test_wav_at_rate(&select, 2, 24_000);
        let selection = SoundSetSelection {
            bgm_dir: Some(root.clone()),
            bgm_variant_dir: None,
            se_dir: None,
            default_dir: None,
        };

        let prepared = SystemSoundManager::prepare(&selection, false, 48_000, None);
        let sample = prepared
            .samples
            .iter()
            .find_map(|(sound_type, _, sample)| {
                (*sound_type == SoundType::Select).then_some(sample)
            })
            .expect("select sample should be prepared");

        assert_eq!(sample.sample_rate, 48_000);
        assert_eq!(sample.frames.len(), 4);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_falls_back_from_invalid_overrides_and_keeps_se_one_shot() {
        let root = test_temp_dir("override-fallback");
        let bgm = root.join("bgm");
        let se = root.join("se");
        let default = root.join("default");
        for dir in [&bgm, &se, &default] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(bgm.join("scratch.wav"), b"broken").unwrap();
        write_test_wav(&se.join("scratch.wav"), 3);
        write_test_wav(&default.join("scratch.wav"), 5);
        std::fs::write(bgm.join("clear.wav"), b"broken").unwrap();
        std::fs::write(se.join("clear.wav"), b"broken").unwrap();
        write_test_wav(&default.join("clear.wav"), 7);
        write_test_wav(&bgm.join("resultclose.wav"), 9);
        write_test_wav(&se.join("resultclose.wav"), 11);
        write_test_wav(&bgm.join("scratch.loop.wav"), 13);
        std::fs::write(bgm.join("aaa.loop.wav"), b"broken").unwrap();
        write_test_wav(&bgm.join("aaa.wav"), 2);
        write_test_wav(&se.join("aaa.wav"), 3);
        write_test_wav(&se.join("aa.loop.wav"), 4);
        std::fs::write(se.join("a.wav"), b"broken").unwrap();
        write_test_wav(&default.join("a.wav"), 6);
        std::fs::write(bgm.join("fail.wav"), b"broken").unwrap();
        let selection = SoundSetSelection {
            bgm_dir: Some(bgm),
            bgm_variant_dir: None,
            se_dir: Some(se),
            default_dir: Some(default),
        };
        let prepared = SystemSoundManager::prepare(&selection, true, 48_000, None);
        for (sound, expected_frames) in [
            (SoundType::Scratch, 3),
            (SoundType::ResultClear, 7),
            (SoundType::ResultClose, 9),
            (SoundType::ResultBgmAAA, 2),
            (SoundType::ResultAAA, 3),
            (SoundType::ResultA, 6),
        ] {
            let sample = prepared.samples.iter().find(|(kind, _, _)| *kind == sound).unwrap();
            assert_eq!(sample.2.frames.len(), expected_frames);
            assert!(!prepared.looping_sounds.contains(&sound));
        }
        assert!(!prepared.samples.iter().any(|(sound, _, _)| *sound == SoundType::ResultBgmFail));
        assert!(!prepared.samples.iter().any(|(sound, _, _)| *sound == SoundType::ResultBgmClear));
        assert!(!prepared.samples.iter().any(|(sound, _, _)| *sound == SoundType::ResultAA));
        assert_eq!(prepared.stats.analysis_count, 1, "only the Result BGM should be analyzed");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_uses_variant_then_parent_and_falls_back_after_decode_failure() {
        let root = test_temp_dir("variant-prepare-fallback");
        let parent = root.join("set");
        let child = parent.join("chosen");
        let se = root.join("se");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir_all(&se).unwrap();
        std::fs::write(child.join("scratch.wav"), b"broken").unwrap();
        write_test_wav(&parent.join("scratch.wav"), 3);
        write_test_wav(&child.join("clear.wav"), 2);
        write_test_wav(&parent.join("clear.loop.wav"), 4);
        write_test_wav(&parent.join("aaa.wav"), 5);
        write_test_wav(&se.join("clear.wav"), 6);
        let selection = SoundSetSelection {
            bgm_dir: Some(parent.clone()),
            bgm_variant_dir: Some(child.clone()),
            se_dir: Some(se),
            default_dir: None,
        };

        let prepared = SystemSoundManager::prepare(&selection, true, 48_000, None);

        let frames_for = |kind| {
            prepared
                .samples
                .iter()
                .find_map(|(sound_type, _, sample)| {
                    (*sound_type == kind).then_some(sample.frames.len())
                })
                .unwrap()
        };
        assert_eq!(frames_for(SoundType::Scratch), 3, "broken child should fall back to parent");
        assert_eq!(frames_for(SoundType::ResultBgmClear), 2, "child plain file beats parent loop");
        assert_eq!(frames_for(SoundType::ResultBgmAAA), 5, "rank BGM can come from parent");
        assert!(!prepared.looping_sounds.contains(&SoundType::ResultBgmClear));
        assert_eq!(
            prepared.normalization_paths.get(&SoundType::ResultBgmClear),
            Some(&child.join("clear.wav"))
        );
        assert_eq!(
            prepared.normalization_paths.get(&SoundType::ResultBgmAAA),
            Some(&parent.join("aaa.wav"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepared_result_bgm_loops_only_with_suffix_and_stops_without_stopping_se() {
        let root = test_temp_dir("result-loop");
        std::fs::create_dir_all(&root).unwrap();
        write_test_wav(&root.join("clear.loop.wav"), 2);
        write_test_wav(&root.join("clear.wav"), 4);
        write_test_wav(&root.join("aaa.wav"), 2);
        let se = root.join("se");
        std::fs::create_dir_all(&se).unwrap();
        write_test_wav(&se.join("clear.wav"), 4);
        write_test_wav(&se.join("aaa.wav"), 2);
        let selection = SoundSetSelection {
            bgm_dir: Some(root.clone()),
            se_dir: Some(se),
            ..Default::default()
        };
        let prepared = SystemSoundManager::prepare(&selection, false, 48_000, None);
        assert!(prepared.looping_sounds.contains(&SoundType::ResultBgmClear));
        let (engine, mut processor) = test_engine();
        let manager = SystemSoundManager::from_prepared(engine, prepared, false);

        manager.play(SoundType::ResultBgmClear, 1.0);
        let first = render(&mut processor, 0, 2);
        assert!(first.iter().any(|sample| *sample != 0.0));
        assert_eq!(render(&mut processor, 2, 2), first);
        manager.stop_all_bgm();
        assert_eq!(render(&mut processor, 4, 2), vec![0.0; 4]);

        manager.play(SoundType::ResultBgmAAA, 1.0);
        assert_eq!(render(&mut processor, 6, 2), first);
        assert_eq!(render(&mut processor, 8, 2), vec![0.0; 4]);

        manager.play(SoundType::ResultBgmClear, 1.0);
        assert_eq!(render(&mut processor, 10, 2), first);
        manager.stop_with_fade_out(SoundType::ResultBgmClear, 2);
        let fade = render(&mut processor, 12, 4);
        assert!(fade[0].abs() > 0.0);
        assert_eq!(&fade[4..], &[0.0; 4]);

        manager.play(SoundType::ResultClear, 1.0);
        manager.stop_all_bgm();
        assert_eq!(render(&mut processor, 16, 2), first);
        manager.stop(SoundType::ResultClear);
        manager.play(SoundType::ResultAAA, 1.0);
        manager.stop_all_bgm();
        assert_eq!(render(&mut processor, 18, 2), first);
        assert_eq!(render(&mut processor, 20, 2), vec![0.0; 4]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn result_bgm_normalization_uses_bgm_gain_and_applies_live_volume_changes() {
        let (engine, mut processor) = test_engine();
        let id = SoundId(SYSTEM_SOUND_BASE);
        insert_sample(
            &engine,
            &mut processor,
            id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
        );
        let manager = SystemSoundManager::with_id_map_and_normalization_gains(
            engine,
            HashMap::from([(SoundType::ResultBgmClear, id)]),
            HashMap::from([(SoundType::ResultBgmClear, 0.5)]),
            true,
            true,
        );
        manager.play(SoundType::ResultBgmClear, 1.0);
        assert_eq!(render(&mut processor, 0, 1), vec![0.5; 2]);
        manager.refresh_volumes(|sound| if sound.is_bgm() { 0.4 } else { 1.0 });
        assert_eq!(render(&mut processor, 1, 1), vec![0.2; 2]);
        manager.set_bgm_normalization_enabled(false);
        manager.refresh_volumes(|_| 0.4);
        assert_eq!(render(&mut processor, 2, 1), vec![0.4; 2]);
    }

    #[test]
    fn play_bgm_stops_existing_voice_before_restart() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5; 48_000] },
        );

        let manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::Select, 1.0);
        assert_eq!(render(&mut processor, 0, 4), vec![0.5; 8]);
        manager.play(SoundType::Select, 1.0);
        assert_eq!(
            render(&mut processor, 8, 4),
            vec![0.5; 8],
            "duplicate BGM play should not stack voices"
        );
    }

    #[test]
    fn play_se_keeps_existing_se_voice() {
        let (engine, mut processor) = test_engine();
        let clear_id = SoundId(SYSTEM_SOUND_BASE);
        let close_id = SoundId(SYSTEM_SOUND_BASE + 1);
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClear, clear_id);
        id_map.insert(SoundType::ResultClose, close_id);
        insert_sample(
            &engine,
            &mut processor,
            clear_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
        );
        insert_sample(
            &engine,
            &mut processor,
            close_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 4] },
        );

        let manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::ResultClear, 1.0);
        assert_eq!(render(&mut processor, 0, 1), vec![1.0, 1.0]);

        manager.play(SoundType::ResultClose, 1.0);
        assert_eq!(render(&mut processor, 1, 1), vec![1.25, 1.25]);
    }

    #[test]
    fn play_scratch_limits_overlapping_voices_to_three() {
        let (engine, mut processor) = test_engine();
        let scratch_id = SoundId(SYSTEM_SOUND_BASE);
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Scratch, scratch_id);
        insert_sample(
            &engine,
            &mut processor,
            scratch_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
        );

        let manager = SystemSoundManager::with_id_map(engine, id_map);
        for _ in 0..5 {
            manager.play(SoundType::Scratch, 1.0);
        }

        assert_eq!(render(&mut processor, 0, 1), vec![3.0, 3.0]);
    }

    #[test]
    fn play_decide_does_not_loop() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Decide, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
        );

        let manager = SystemSoundManager::with_id_map(engine.clone(), id_map);
        manager.play(SoundType::Decide, 1.0);
        assert_eq!(render(&mut processor, 0, 2), vec![0.5, 0.5, 0.25, 0.25]);
        assert!(engine.is_idle());
    }

    #[test]
    fn refresh_volumes_updates_active_bgm_voice() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        let manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::Select, 1.0);
        render(&mut processor, 0, 1);

        manager.refresh_volumes(|sound_type| if sound_type.is_bgm() { 0.25 } else { 1.0 });
        assert_eq!(render(&mut processor, 1, 1), vec![0.25, 0.25]);
    }

    #[test]
    fn set_volume_updates_single_active_bgm_voice() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        let manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::Select, 1.0);
        render(&mut processor, 0, 1);

        manager.set_volume(SoundType::Select, 0.4);
        assert_eq!(render(&mut processor, 1, 1), vec![0.4, 0.4]);
    }

    #[test]
    fn system_bgm_normalization_composes_with_crossfade_and_runtime_toggle() {
        let (engine, mut processor) = test_engine();
        let select_id = SoundId(SYSTEM_SOUND_BASE);
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, select_id);
        insert_sample(
            &engine,
            &mut processor,
            select_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 3] },
        );
        let manager = SystemSoundManager::with_id_map_and_normalization_gains(
            engine,
            id_map,
            HashMap::from([(SoundType::Select, 0.5)]),
            false,
            true,
        );
        manager.set_bgm_normalization_enabled(true);
        manager.play(SoundType::Select, 1.0);
        assert_eq!(render(&mut processor, 0, 1), vec![0.5, 0.5]);

        manager.set_volume(SoundType::Select, 0.4);
        assert_eq!(render(&mut processor, 1, 1), vec![0.2, 0.2]);

        manager.set_bgm_normalization_enabled(false);
        manager.refresh_volumes(|_| 1.0);
        assert_eq!(render(&mut processor, 2, 1), vec![1.0, 1.0]);
    }

    #[test]
    fn apply_normalization_analysis_updates_active_voice_without_replacing_sample() {
        let (engine, mut processor) = test_engine();
        let select_id = SoundId(SYSTEM_SOUND_BASE);
        let id_map = HashMap::from([(SoundType::Select, select_id)]);
        insert_sample(
            &engine,
            &mut processor,
            select_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 0.5, 0.25] },
        );
        let mut manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::Select, 1.0);
        assert_eq!(render(&mut processor, 0, 1), vec![1.0, 1.0]);

        manager.apply_normalization_analysis(PreparedSystemSoundSet {
            normalization_paths: HashMap::new(),
            normalization_keys: HashMap::new(),
            source_fingerprint: None,
            samples: Vec::new(),
            looping_sounds: HashSet::new(),
            bgm_normalization_gains: HashMap::from([(SoundType::Select, 0.25)]),
            normalization_analysis_enabled: true,
            reused_existing_sound_set: false,
            stats: SystemSoundPrepareStats::default(),
        });
        manager.set_bgm_normalization_enabled(true);
        manager.refresh_volumes(|_| 1.0);

        // 既存voiceが同じ再生位置から続き、登録済みsampleを差し替えずgainだけ反映する。
        assert_eq!(render(&mut processor, 1, 1), vec![0.125, 0.125]);
    }

    #[test]
    fn system_bgm_normalization_never_changes_system_se() {
        let (engine, mut processor) = test_engine();
        let clear_id = SoundId(SYSTEM_SOUND_BASE);
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClear, clear_id);
        insert_sample(
            &engine,
            &mut processor,
            clear_id,
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] },
        );
        let manager = SystemSoundManager::with_id_map_and_normalization_gains(
            engine,
            id_map,
            HashMap::from([(SoundType::ResultClear, 0.25)]),
            false,
            true,
        );
        manager.set_bgm_normalization_enabled(true);

        manager.play(SoundType::ResultClear, 1.0);

        assert_eq!(render(&mut processor, 0, 1), vec![1.0, 1.0]);
    }

    #[test]
    fn set_master_gain_scales_all_system_sound_output() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClose, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        let manager = SystemSoundManager::with_id_map(engine, id_map);

        manager.set_master_gain(0.25);
        manager.play(SoundType::ResultClose, 1.0);
        assert_eq!(render(&mut processor, 0, 1), vec![0.25, 0.25]);
    }

    #[test]
    fn stop_with_fade_out_ramps_active_system_sound() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClear, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
        );
        let manager = SystemSoundManager::with_id_map(engine, id_map);
        manager.play(SoundType::ResultClear, 1.0);
        render(&mut processor, 0, 1);

        manager.stop_with_fade_out(SoundType::ResultClear, 2);
        assert_eq!(render(&mut processor, 1, 3), vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn play_with_fade_out_ramps_new_system_sound() {
        let (engine, mut processor) = test_engine();
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClose, SoundId(SYSTEM_SOUND_BASE));
        insert_sample(
            &engine,
            &mut processor,
            SoundId(SYSTEM_SOUND_BASE),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
        );
        let manager = SystemSoundManager::with_id_map(engine, id_map);

        manager.play_with_master_gain_and_fade_out(SoundType::ResultClose, 1.0, 1.0, 2);

        assert_eq!(render(&mut processor, 0, 3), vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn system_sound_ids_are_above_typical_chart_ids_but_safe_for_vec_sample_bank() {
        // BMS の `#WAVxx` は最大 1296 個なので 100_000 オフセットなら chart と衝突しない。
        // 一方で `SampleBank` (`Vec<Option<DecodedSample>>`) の resize が現実的サイズで済む
        // (= u32::MAX のような巨大 index を使うと数十 GB の allocation で OOM kill される) こと。
        const { assert!(SYSTEM_SOUND_BASE >= 10_000) };
        const { assert!(SYSTEM_SOUND_BASE as usize + SoundType::ALL.len() < 10_000_000) };
    }

    fn test_engine() -> (AudioEngineHandle, CommandedAudioEngine) {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let processor = engine.processor();
        (engine, processor)
    }

    fn insert_sample(
        engine: &AudioEngineHandle,
        processor: &mut CommandedAudioEngine,
        id: SoundId,
        sample: DecodedSample,
    ) {
        assert!(engine.insert_sample(id, sample));
        processor.apply_pending_commands_for_tests();
    }

    fn test_temp_dir(label: &str) -> PathBuf {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-system-sound-{label}-{}-{now}", std::process::id()))
    }

    fn write_test_wav(path: &Path, frames: u32) {
        write_test_wav_at_rate(path, frames, 48_000);
    }

    fn write_test_wav_at_rate(path: &Path, frames: u32, sample_rate: u32) {
        let data_len = frames.saturating_mul(2);
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32.saturating_add(data_len)).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.saturating_mul(2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let sample = if frame.is_multiple_of(2) { 10_000i16 } else { -10_000i16 };
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn write_stereo_test_wav(path: &Path, duration_seconds: u32) {
        use std::io::Write;

        const SAMPLE_RATE: u32 = 48_000;
        let frames = SAMPLE_RATE * duration_seconds;
        let data_len = frames * 4;
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36u32 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap();
        file.write_all(&2u16.to_le_bytes()).unwrap();
        file.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
        file.write_all(&(SAMPLE_RATE * 4).to_le_bytes()).unwrap();
        file.write_all(&4u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();

        let mut one_second = Vec::with_capacity((SAMPLE_RATE * 4) as usize);
        for frame in 0..SAMPLE_RATE {
            let sample = if frame.is_multiple_of(2) { 10_000i16 } else { -10_000i16 };
            one_second.extend_from_slice(&sample.to_le_bytes());
            one_second.extend_from_slice(&(-sample).to_le_bytes());
        }
        for _ in 0..duration_seconds {
            file.write_all(&one_second).unwrap();
        }
    }

    fn render(processor: &mut CommandedAudioEngine, start_frame: u64, frames: usize) -> Vec<f32> {
        let mut output = vec![0.0; frames * 2];
        assert!(processor.render_stereo(start_frame, &mut output));
        output
    }
}
