//! [`SystemSoundManager`] は [`crate::system_sound`] が決定したサウンドセットを
//! デコードして `SharedAudioEngine` に登録し、各 [`SoundType`] を SE / BGM として
//! 再生・停止する beatoraja の `SystemSoundManager` 相当 facade。
//!
//! - 構築時に 22 種すべてを `FfmpegSampleLoader` でデコードし、サンプル個別の失敗は
//!   warn ログだけで継続する(致命化しない)。
//! - SoundId は chart の音声サンプルと衝突しないよう `u32::MAX` の上位 22 個を予約。
//! - 再生は [`bmz_audio::engine::AudioEngine::play_now`] を経由し、`is_bgm()` の音は
//!   そのままループ再生になる。

use std::collections::HashMap;

use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_core::ids::SoundId;

use crate::system_sound::{SoundSetSelection, SoundType};

/// chart 側のキー音 SoundId と衝突しないよう、`u32::MAX` から 22 個を予約。
const SYSTEM_SOUND_BASE: u32 = u32::MAX - (SoundType::ALL.len() as u32 - 1);

pub struct SystemSoundManager {
    engine: SharedAudioEngine,
    id_map: HashMap<SoundType, SoundId>,
}

impl SystemSoundManager {
    /// `selection` から各 [`SoundType`] のパスを解決し、デコードして engine へ登録する。
    /// 解決失敗 / デコード失敗はサウンド単位で warn ログを出してスキップする。
    pub fn new(engine: SharedAudioEngine, selection: &SoundSetSelection) -> Self {
        let mut id_map = HashMap::new();
        let mut loader = FfmpegSampleLoader;
        let mut decoded: Vec<(SoundId, bmz_audio::sample::DecodedSample)> = Vec::new();

        for (i, sound_type) in SoundType::ALL.iter().enumerate() {
            let id = SoundId(SYSTEM_SOUND_BASE + i as u32);
            let Some(path) = selection.resolve(*sound_type) else {
                tracing::warn!(
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
        let loop_playback = sound_type.is_bgm();
        if let Ok(mut engine) = self.engine.lock() {
            engine.play_now(id, master_volume, loop_playback);
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
    fn system_sound_ids_do_not_collide_with_typical_chart_ids() {
        // chart の SoundId は通常小さい値から割り当てられる前提で、
        // システム音は u32::MAX 付近を使うことを assertion で固定する。
        const { assert!(SYSTEM_SOUND_BASE >= u32::MAX - 1024) };
    }
}
