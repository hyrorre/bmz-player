//! [`SystemSoundManager`] は [`crate::system_sound`] が決定したサウンドセットを
//! デコードして `SharedAudioEngine` に登録し、各 [`SoundType`] を SE / BGM として
//! 再生・停止する beatoraja の `SystemSoundManager` 相当 facade。
//!
//! - 構築時に 22 種すべてを `FfmpegSampleLoader` でデコードし、サンプル個別の失敗は
//!   warn ログだけで継続する(致命化しない)。
//! - SoundId は chart のキー音(BMS `#WAVxx` は base-36 で最大 1296 個)と衝突しないよう
//!   [`SYSTEM_SOUND_BASE`] (= 100_000) からの 22 連番を予約する。`SampleBank` は
//!   `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、`u32::MAX` 付近の
//!   巨大 ID を使うと resize が数十 GB の allocation を試みて OOM kill される。
//! - 再生は [`bmz_audio::engine::AudioEngine::play_now`] を経由し、`is_bgm()` の音は
//!   そのままループ再生になる。

use std::collections::HashMap;

use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_core::ids::SoundId;

use crate::system_sound::{SoundSetSelection, SoundType};

/// chart 側のキー音 SoundId と衝突しないよう確保する予約レンジの先頭。
/// `SampleBank` は `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、
/// 大きすぎる値を使うと巨大な resize が走り OOM kill される。
/// BMS の `#WAVxx` は base-36 で最大 1296 個なので、100_000 オフセットなら衝突しない。
const SYSTEM_SOUND_BASE: u32 = 100_000;

pub struct SystemSoundManager {
    engine: SharedAudioEngine,
    id_map: HashMap<SoundType, SoundId>,
}

impl SystemSoundManager {
    /// `selection` から各 [`SoundType`] のパスを解決し、デコードして engine へ登録する。
    /// 解決失敗は info、デコード失敗は warn をサウンド単位で出してスキップする。
    pub fn new(engine: SharedAudioEngine, selection: &SoundSetSelection) -> Self {
        let mut id_map = HashMap::new();
        let mut loader = FfmpegSampleLoader;
        let mut decoded: Vec<(SoundId, bmz_audio::sample::DecodedSample)> = Vec::new();

        for (i, sound_type) in SoundType::ALL.iter().enumerate() {
            let id = SoundId(SYSTEM_SOUND_BASE + i as u32);
            let Some(path) = selection.resolve(*sound_type) else {
                tracing::info!(
                    sound_type = ?sound_type,
                    file_name = sound_type.file_name(),
                    "system sound file not found in selected set or default dir; skipping"
                );
                continue;
            };
            match loader.load(&path) {
                Ok(sample) => {
                    decoded.push((id, sample));
                    id_map.insert(*sound_type, id);
                }
                Err(error) => {
                    tracing::warn!(
                        sound_type = ?sound_type,
                        path = %path.display(),
                        %error,
                        "failed to decode system sound; skipping"
                    );
                }
            }
        }

        // ロックは1回に集約してロード結果をまとめて挿入する。
        if !decoded.is_empty() {
            match engine.lock() {
                Ok(mut guard) => {
                    for (id, sample) in decoded {
                        guard.insert_sample(id, sample);
                    }
                }
                Err(poisoned) => {
                    let mut guard = poisoned.into_inner();
                    for (id, sample) in decoded {
                        guard.insert_sample(id, sample);
                    }
                }
            }
        }

        Self { engine, id_map }
    }

    /// 引数で指定した SoundType を再生する。BGM はループ、SE は 1 ショット。
    /// 対応サンプルが登録されていない場合は何もしない。
    pub fn play(&self, sound_type: SoundType, master_volume: f32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        let loop_playback = sound_type.loops();
        if let Ok(mut engine) = self.engine.lock() {
            if sound_type.is_bgm() {
                engine.stop_sound(id);
            }
            engine.play_now(id, master_volume, loop_playback);
        }
    }

    /// 登録済み sound の再生待ち/再生中音量を、SoundType ごとの最新設定で更新する。
    pub fn refresh_volumes(&self, mut volume_for: impl FnMut(SoundType) -> f32) {
        let Ok(mut engine) = self.engine.lock() else {
            return;
        };
        for (&sound_type, &id) in &self.id_map {
            engine.set_sound_volume(id, volume_for(sound_type));
        }
    }

    /// 指定 SoundType の再生待ち/再生中音量を直接更新する。
    pub fn set_volume(&self, sound_type: SoundType, volume: f32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_sound_volume(id, volume);
        }
    }

    /// システム音 engine 全体のマスターゲインを更新する。
    /// リザルト退出時の `ResultClose` など、複数のシステム音をまとめて
    /// フェードアウトさせる用途で使う。
    pub fn set_master_gain(&self, gain: f32) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_master_gain(gain);
        }
    }

    /// 指定 SoundType を停止する。鳴っていなくても害は無い。
    pub fn stop(&self, sound_type: SoundType) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        if let Ok(mut engine) = self.engine.lock() {
            engine.stop_sound(id);
        }
    }

    /// 登録済みかつ `is_bgm()` の SoundType をすべて停止する。
    pub fn stop_all_bgm(&self) {
        let Ok(mut engine) = self.engine.lock() else {
            return;
        };
        for sound_type in SoundType::ALL.iter().filter(|t| t.is_bgm()) {
            if let Some(&id) = self.id_map.get(sound_type) {
                engine.stop_sound(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bmz_audio::engine::AudioEngine;

    use super::*;

    #[test]
    fn new_succeeds_with_empty_selection_and_registers_no_samples() {
        // どのファイルも resolve できない Selection を渡してもパニックせず空 manager を返すこと。
        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let selection = SoundSetSelection::default();

        let manager = SystemSoundManager::new(Arc::clone(&engine), &selection);

        assert!(manager.id_map.is_empty());
        // 未登録の SoundType の play / stop は no-op で問題ないこと。
        manager.play(SoundType::Scratch, 1.0);
        manager.stop(SoundType::Select);
        manager.stop_all_bgm();
    }

    #[test]
    fn play_bgm_stops_existing_voice_before_restart() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                SoundId(SYSTEM_SOUND_BASE),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5; 48_000] },
            );
        }

        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };
        manager.play(SoundType::Select, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 8];
            guard.render_stereo(0, &mut output);
            assert_eq!(guard.mixer.voices.len(), 1);
        }
        manager.play(SoundType::Select, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 8];
            guard.render_stereo(8, &mut output);
            assert_eq!(guard.mixer.voices.len(), 1, "duplicate BGM play should not stack voices");
        }
    }

    #[test]
    fn play_se_keeps_existing_se_voice() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let clear_id = SoundId(SYSTEM_SOUND_BASE);
        let close_id = SoundId(SYSTEM_SOUND_BASE + 1);
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClear, clear_id);
        id_map.insert(SoundType::ResultClose, close_id);
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                clear_id,
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 4] },
            );
            guard.insert_sample(
                close_id,
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25; 4] },
            );
        }

        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };
        manager.play(SoundType::ResultClear, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 2];
            guard.render_stereo(0, &mut output);
            assert_eq!(guard.mixer.voices.len(), 1);
        }

        manager.play(SoundType::ResultClose, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 2];
            guard.render_stereo(1, &mut output);
            assert_eq!(output, vec![1.25, 1.25]);
            assert_eq!(guard.mixer.voices.len(), 2, "SE voices should overlap");
        }
    }

    #[test]
    fn play_decide_does_not_loop() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Decide, SoundId(SYSTEM_SOUND_BASE));
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                SoundId(SYSTEM_SOUND_BASE),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
            );
        }

        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };
        manager.play(SoundType::Decide, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 4];
            guard.render_stereo(0, &mut output);
            assert_eq!(output, vec![0.5, 0.5, 0.25, 0.25]);
            assert!(guard.mixer.voices.is_empty());
        }
    }

    #[test]
    fn refresh_volumes_updates_active_bgm_voice() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                SoundId(SYSTEM_SOUND_BASE),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
            );
        }
        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };
        manager.play(SoundType::Select, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 2];
            guard.render_stereo(0, &mut output);
        }

        manager.refresh_volumes(|sound_type| if sound_type.is_bgm() { 0.25 } else { 1.0 });
        let mut output = vec![0.0; 2];
        engine.lock().unwrap().render_stereo(1, &mut output);

        assert_eq!(output, vec![0.25, 0.25]);
    }

    #[test]
    fn set_volume_updates_single_active_bgm_voice() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::Select, SoundId(SYSTEM_SOUND_BASE));
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                SoundId(SYSTEM_SOUND_BASE),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
            );
        }
        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };
        manager.play(SoundType::Select, 1.0);
        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 2];
            guard.render_stereo(0, &mut output);
        }

        manager.set_volume(SoundType::Select, 0.4);
        let mut output = vec![0.0; 2];
        engine.lock().unwrap().render_stereo(1, &mut output);

        assert_eq!(output, vec![0.4, 0.4]);
    }

    #[test]
    fn set_master_gain_scales_all_system_sound_output() {
        use bmz_audio::sample::DecodedSample;

        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut id_map = HashMap::new();
        id_map.insert(SoundType::ResultClose, SoundId(SYSTEM_SOUND_BASE));
        {
            let mut guard = engine.lock().unwrap();
            guard.insert_sample(
                SoundId(SYSTEM_SOUND_BASE),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
            );
        }
        let manager = SystemSoundManager { engine: Arc::clone(&engine), id_map };

        manager.set_master_gain(0.25);
        manager.play(SoundType::ResultClose, 1.0);
        let mut output = vec![0.0; 2];
        engine.lock().unwrap().render_stereo(0, &mut output);

        assert_eq!(output, vec![0.25, 0.25]);
    }

    #[test]
    fn system_sound_ids_are_above_typical_chart_ids_but_safe_for_vec_sample_bank() {
        // BMS の `#WAVxx` は最大 1296 個なので 100_000 オフセットなら chart と衝突しない。
        // 一方で `SampleBank` (`Vec<Option<DecodedSample>>`) の resize が現実的サイズで済む
        // (= u32::MAX のような巨大 index を使うと数十 GB の allocation で OOM kill される) こと。
        const { assert!(SYSTEM_SOUND_BASE >= 10_000) };
        const { assert!(SYSTEM_SOUND_BASE as usize + SoundType::ALL.len() < 10_000_000) };
    }
}
