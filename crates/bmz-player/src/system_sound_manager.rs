//! [`SystemSoundManager`] は [`crate::system_sound`] が決定したサウンドセットを
//! デコードして system audio command handle に登録し、各 [`SoundType`] を SE / BGM として
//! 再生・停止する beatoraja の `SystemSoundManager` 相当 facade。
//!
//! - 構築時に 22 種すべてを `FfmpegSampleLoader` でデコードし、サンプル個別の失敗は
//!   warn ログだけで継続する(致命化しない)。
//! - SoundId は chart のキー音(BMS `#WAVxx` は base-36 で最大 1296 個)と衝突しないよう
//!   [`SYSTEM_SOUND_BASE`] (= 100_000) からの 22 連番を予約する。`SampleBank` は
//!   `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、`u32::MAX` 付近の
//!   巨大 ID を使うと resize が数十 GB の allocation を試みて OOM kill される。
//! - 再生は [`bmz_audio::engine::AudioEngine::play_now`] 相当の command を経由し、`is_bgm()` の音は
//!   そのままループ再生になる。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_core::ids::SoundId;

use crate::system_sound::{SoundSetSelection, SoundType};

/// chart 側のキー音 SoundId と衝突しないよう確保する予約レンジの先頭。
/// `SampleBank` は `Vec<Option<DecodedSample>>` で `SoundId.0` を index に取るため、
/// 大きすぎる値を使うと巨大な resize が走り OOM kill される。
/// BMS の `#WAVxx` は base-36 で最大 1296 個なので、100_000 オフセットなら衝突しない。
const SYSTEM_SOUND_BASE: u32 = 100_000;
const VOLUME_EPSILON: f32 = 0.000_1;

pub struct SystemSoundManager {
    engine: AudioEngineHandle,
    id_map: HashMap<SoundType, SoundId>,
    last_volumes: RefCell<HashMap<SoundType, f32>>,
    master_gain: Cell<f32>,
}

impl SystemSoundManager {
    /// `selection` から各 [`SoundType`] のパスを解決し、デコードして engine へ登録する。
    /// 解決失敗は info、デコード失敗は warn をサウンド単位で出してスキップする。
    pub fn new(engine: AudioEngineHandle, selection: &SoundSetSelection) -> Self {
        let mut id_map = HashMap::new();
        let mut loader = FfmpegSampleLoader;
        let mut commands = Vec::new();

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
                    commands.push(AudioEngineCommand::InsertSample { id, sample });
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

        if !commands.is_empty() && !engine.push_commands(commands) {
            tracing::warn!("failed to enqueue decoded system sounds");
        }

        Self::with_id_map(engine, id_map)
    }

    fn with_id_map(engine: AudioEngineHandle, id_map: HashMap<SoundType, SoundId>) -> Self {
        Self {
            engine,
            id_map,
            last_volumes: RefCell::new(HashMap::new()),
            master_gain: Cell::new(1.0),
        }
    }

    /// 引数で指定した SoundType を再生する。BGM はループ、SE は 1 ショット。
    /// 対応サンプルが登録されていない場合は何もしない。
    pub fn play(&self, sound_type: SoundType, master_volume: f32) {
        self.play_with_master_gain(sound_type, master_volume, self.master_gain.get());
    }

    /// マスターゲイン復帰と再生を 1 回の AudioEngine lock にまとめる。
    pub fn play_with_master_gain(&self, sound_type: SoundType, master_volume: f32, gain: f32) {
        let Some(&id) = self.id_map.get(&sound_type) else {
            return;
        };
        let master_volume = normalize_volume(master_volume);
        let gain = normalize_volume(gain);
        let loop_playback = sound_type.loops();
        let mut commands = vec![AudioEngineCommand::SetMasterGain { gain }];
        if sound_type.is_bgm() {
            commands.push(AudioEngineCommand::StopSound { id });
        }
        commands.push(AudioEngineCommand::PlayNow {
            sound_id: id,
            volume: master_volume,
            loop_playback,
        });
        if self.engine.push_commands(commands) {
            self.master_gain.set(gain);
            self.last_volumes.borrow_mut().insert(sound_type, master_volume);
        }
    }

    /// 登録済み sound の再生待ち/再生中音量を、SoundType ごとの最新設定で更新する。
    pub fn refresh_volumes(&self, mut volume_for: impl FnMut(SoundType) -> f32) {
        let mut updates = Vec::new();
        {
            let last_volumes = self.last_volumes.borrow();
            for (&sound_type, &id) in &self.id_map {
                let volume = normalize_volume(volume_for(sound_type));
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
        let volume = normalize_volume(volume);
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
}

fn normalize_volume(volume: f32) -> f32 {
    if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 }
}

fn volume_matches(left: f32, right: f32) -> bool {
    (left - right).abs() <= VOLUME_EPSILON
}

#[cfg(test)]
mod tests {
    use bmz_audio::command::CommandedAudioEngine;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::sample::DecodedSample;

    use super::*;

    #[test]
    fn new_succeeds_with_empty_selection_and_registers_no_samples() {
        // どのファイルも resolve できない Selection を渡してもパニックせず空 manager を返すこと。
        let (engine, _processor) = test_engine();
        let selection = SoundSetSelection::default();

        let manager = SystemSoundManager::new(engine, &selection);

        assert!(manager.id_map.is_empty());
        // 未登録の SoundType の play / stop は no-op で問題ないこと。
        manager.play(SoundType::Scratch, 1.0);
        manager.stop(SoundType::Select);
        manager.stop_all_bgm();
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

    fn render(processor: &mut CommandedAudioEngine, start_frame: u64, frames: usize) -> Vec<f32> {
        let mut output = vec![0.0; frames * 2];
        assert!(processor.render_stereo(start_frame, &mut output));
        output
    }
}
