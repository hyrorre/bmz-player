use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_core::ids::SoundId;
use bmz_render::renderer::SkinRuntimeScene;
use bmz_render::skin::{SkinAudioActionDef, SkinAudioActionKind, SkinCustomEventDef, SkinDocument};

use crate::skin_loader::DecodedSkinAudio;

/// system sound / chart preview の予約範囲より後ろに置く、sceneごとの小さい固定レンジ。
const SKIN_AUDIO_SOUND_BASE: u32 = 100_100;
/// 1 sceneあたりのID数。前半は宣言的な音声、後半はLuaが実行時に鳴らすfileに使う。
const SKIN_AUDIO_SCENE_SOUNDS: u32 = 128;
const SKIN_AUDIO_DECLARED_SOUNDS: u32 = 64;

/// Lua skin の音声命令を、共有 system audio engine 上で実行するsceneごとのランタイム。
///
/// Resultは宣言的な`sceneAudio` / `customEvents`も持つ。Luaの`act` / `customEvents`が
/// 実行時に鳴らすfileは初回にbackgroundでdecodeし、登録されるまで命令を順に保留する。
pub struct SkinAudioRuntime {
    engine: AudioEngineHandle,
    sound_ids: HashMap<String, SoundId>,
    /// 解決済みpathごとのfile。宣言的な音声とLuaの`audio_*`で共有する。
    files: HashMap<PathBuf, Arc<SkinAudioFile>>,
    next_runtime_id: u32,
    runtime_id_end: u32,
    runtime_limit_warned: bool,
    /// Luaの`audio_loop`で鳴らし、まだ止めていない音。
    looping: HashSet<SoundId>,
    scene_audio: Vec<SkinAudioActionDef>,
    custom_events: Vec<SkinCustomEventDef>,
    fired_once_events: HashSet<i32>,
}

struct SkinAudioFile {
    id: SoundId,
    state: Mutex<SkinAudioFileState>,
}

enum SkinAudioFileState {
    /// decode中。命令はsampleの登録と同時に送る。
    Loading(Vec<AudioEngineCommand>),
    Ready,
    Failed,
    /// 所有するskinは破棄済み。遅いworkerのsample登録も受け付けない。
    Cancelled,
}

impl SkinAudioRuntime {
    /// Result skinの宣言的な音声とdecode済みassetを登録する。
    pub fn install(
        engine: AudioEngineHandle,
        document: &SkinDocument,
        audio_assets: Vec<DecodedSkinAudio>,
    ) -> Self {
        let mut runtime = Self::empty(engine, SkinRuntimeScene::Result);
        let base = scene_sound_base(SkinRuntimeScene::Result);
        let mut commands = Vec::with_capacity(audio_assets.len());
        for (index, audio) in
            audio_assets.into_iter().take(SKIN_AUDIO_DECLARED_SOUNDS as usize).enumerate()
        {
            let id = SoundId(base + index as u32);
            runtime.sound_ids.insert(audio.path, id);
            if let Some(resolved) = audio.resolved {
                runtime.files.insert(resolved, Arc::new(SkinAudioFile::ready(id)));
            }
            commands.push(AudioEngineCommand::InsertSample { id, sample: audio.sample });
        }
        if !commands.is_empty() && !runtime.engine.push_commands(commands) {
            tracing::warn!("failed to enqueue decoded skin audio assets");
        }
        runtime.scene_audio = document.scene_audio.clone();
        runtime.custom_events = document.custom_events.clone();
        runtime
    }

    /// 宣言的な音声を持たないsceneで、Luaの`audio_*`だけを扱う。
    pub fn for_lua(engine: AudioEngineHandle, scene: SkinRuntimeScene) -> Self {
        Self::empty(engine, scene)
    }

    fn empty(engine: AudioEngineHandle, scene: SkinRuntimeScene) -> Self {
        let base = scene_sound_base(scene);
        Self {
            engine,
            sound_ids: HashMap::new(),
            files: HashMap::new(),
            next_runtime_id: base + SKIN_AUDIO_DECLARED_SOUNDS,
            runtime_id_end: base + SKIN_AUDIO_SCENE_SOUNDS,
            runtime_limit_warned: false,
            looping: HashSet::new(),
            scene_audio: Vec::new(),
            custom_events: Vec::new(),
            fired_once_events: HashSet::new(),
        }
    }

    pub fn reset(&mut self) {
        self.stop_all();
        self.looping.clear();
        self.fired_once_events.clear();
    }

    pub fn start_scene(&self, bgm_volume: f32, se_volume: f32) -> bool {
        self.run_actions(&self.scene_audio, bgm_volume, se_volume)
    }

    pub fn trigger_timer(&mut self, timer: i32, bgm_volume: f32, se_volume: f32) -> bool {
        let matching = self
            .custom_events
            .iter()
            .filter(|event| {
                event.timer == timer && (!event.once || !self.fired_once_events.contains(&event.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let actions = matching
            .iter()
            .flat_map(|event| event.audio_actions.iter().cloned())
            .collect::<Vec<_>>();
        let dispatched = self.run_actions(&actions, bgm_volume, se_volume);
        if dispatched {
            for event in matching.into_iter().filter(|event| event.once) {
                self.fired_once_events.insert(event.id);
            }
        }
        dispatched
    }

    /// Luaの`audio_play` / `audio_loop` / `audio_stop`。`action.path`は解決済みの
    /// 絶対pathで、未知のfileは初回にbackgroundでdecodeする。
    pub fn run_lua_action(
        &mut self,
        action: &SkinAudioActionDef,
        bgm_volume: f32,
        se_volume: f32,
    ) -> bool {
        let path = PathBuf::from(&action.path);
        let file = match self.files.get(&path) {
            Some(file) => Arc::clone(file),
            // 鳴らしていないfileは止める必要がない。
            None if action.action == SkinAudioActionKind::Stop => return false,
            None => match self.load_runtime_file(path) {
                Some(file) => file,
                None => return false,
            },
        };
        match action.action {
            SkinAudioActionKind::Loop => {
                self.looping.insert(file.id);
            }
            SkinAudioActionKind::Stop => {
                self.looping.remove(&file.id);
            }
            SkinAudioActionKind::Play => {}
        }
        file.dispatch(&self.engine, action_commands(action, file.id, bgm_volume, se_volume))
    }

    /// sceneを離れるとき、Luaで始めたloopと保留中の命令を止める。単発の音は鳴らし切る。
    pub fn stop_loops(&mut self) -> bool {
        for file in self.files.values() {
            file.cancel_pending();
        }
        let commands =
            self.looping.drain().map(|id| AudioEngineCommand::StopSound { id }).collect::<Vec<_>>();
        !commands.is_empty() && self.engine.push_commands(commands)
    }

    pub fn stop_all(&self) -> bool {
        for file in self.files.values() {
            file.cancel_pending();
        }
        let ids = self
            .sound_ids
            .values()
            .copied()
            .chain(self.files.values().map(|file| file.id))
            .collect::<HashSet<_>>();
        let commands =
            ids.into_iter().map(|id| AudioEngineCommand::StopSound { id }).collect::<Vec<_>>();
        !commands.is_empty() && self.engine.push_commands(commands)
    }

    fn run_actions(&self, actions: &[SkinAudioActionDef], bgm_volume: f32, se_volume: f32) -> bool {
        let commands = actions
            .iter()
            .filter_map(|action| {
                let id = *self.sound_ids.get(&action.path)?;
                Some(action_commands(action, id, bgm_volume, se_volume))
            })
            .flatten()
            .collect::<Vec<_>>();
        !commands.is_empty() && self.engine.push_commands(commands)
    }

    fn load_runtime_file(&mut self, path: PathBuf) -> Option<Arc<SkinAudioFile>> {
        if self.next_runtime_id >= self.runtime_id_end {
            if !std::mem::replace(&mut self.runtime_limit_warned, true) {
                tracing::warn!(
                    path = %path.display(),
                    limit = SKIN_AUDIO_SCENE_SOUNDS - SKIN_AUDIO_DECLARED_SOUNDS,
                    "Lua skin plays too many audio files; skipping the rest"
                );
            }
            return None;
        }
        let id = SoundId(self.next_runtime_id);
        self.next_runtime_id += 1;
        let file = Arc::new(SkinAudioFile {
            id,
            state: Mutex::new(SkinAudioFileState::Loading(Vec::new())),
        });
        self.files.insert(path.clone(), Arc::clone(&file));
        let engine = self.engine.clone();
        let worker = Arc::clone(&file);
        let spawned = std::thread::Builder::new()
            .name("bmz-skin-audio".to_string())
            .spawn(move || worker.decode(&engine, &path));
        if let Err(error) = spawned {
            tracing::warn!(%error, "failed to start skin audio decode");
            file.set_state(SkinAudioFileState::Failed);
        }
        Some(file)
    }
}

impl SkinAudioFile {
    fn ready(id: SoundId) -> Self {
        Self { id, state: Mutex::new(SkinAudioFileState::Ready) }
    }

    fn set_state(&self, next: SkinAudioFileState) {
        if let Ok(mut state) = self.state.lock() {
            *state = next;
        }
    }

    /// 登録済みならすぐ送り、decode中なら保留する。
    fn dispatch(&self, engine: &AudioEngineHandle, commands: Vec<AudioEngineCommand>) -> bool {
        let Ok(mut state) = self.state.lock() else { return false };
        match &mut *state {
            SkinAudioFileState::Ready => engine.push_commands(commands),
            SkinAudioFileState::Loading(pending) => {
                pending.extend(commands);
                true
            }
            SkinAudioFileState::Failed | SkinAudioFileState::Cancelled => false,
        }
    }

    fn cancel_pending(&self) {
        if let Ok(mut state) = self.state.lock()
            && let SkinAudioFileState::Loading(pending) = &mut *state
        {
            pending.clear();
        }
    }

    fn cancel(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = SkinAudioFileState::Cancelled;
        }
    }

    /// 登録と保留中の命令を同じlock内で送り、後から届く命令との順序を保つ。
    fn decode(&self, engine: &AudioEngineHandle, path: &Path) {
        let sample = FfmpegSampleLoader::default().load(path);
        let Ok(mut state) = self.state.lock() else { return };
        // Drop and sample submission share this lock. Once the owner is gone,
        // an old worker cannot overwrite an ID already reused by the next skin.
        if matches!(*state, SkinAudioFileState::Cancelled) {
            return;
        }
        match sample {
            Ok(sample) => {
                let pending = match std::mem::replace(&mut *state, SkinAudioFileState::Ready) {
                    SkinAudioFileState::Loading(pending) => pending,
                    _ => Vec::new(),
                };
                let mut commands = Vec::with_capacity(pending.len() + 1);
                commands.push(AudioEngineCommand::InsertSample { id: self.id, sample });
                commands.extend(pending);
                if !engine.push_commands(commands) {
                    tracing::warn!(path = %path.display(), "failed to enqueue skin audio");
                }
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "failed to decode skin audio");
                *state = SkinAudioFileState::Failed;
            }
        }
    }
}

impl Drop for SkinAudioRuntime {
    fn drop(&mut self) {
        for file in self.files.values() {
            file.cancel();
        }
        self.stop_all();
    }
}

fn scene_sound_base(scene: SkinRuntimeScene) -> u32 {
    let index = match scene {
        SkinRuntimeScene::Result => 0,
        SkinRuntimeScene::Select => 1,
        SkinRuntimeScene::Decide => 2,
        SkinRuntimeScene::Play => 3,
    };
    SKIN_AUDIO_SOUND_BASE + index * SKIN_AUDIO_SCENE_SOUNDS
}

/// 単発の音はSE、loopはBGMの音量で鳴らす。
fn action_commands(
    action: &SkinAudioActionDef,
    id: SoundId,
    bgm_volume: f32,
    se_volume: f32,
) -> Vec<AudioEngineCommand> {
    match action.action {
        SkinAudioActionKind::Play => vec![AudioEngineCommand::PlayNow {
            sound_id: id,
            volume: normalized_volume(action.volume * se_volume),
            loop_playback: false,
        }],
        SkinAudioActionKind::Loop => vec![
            AudioEngineCommand::StopSound { id },
            AudioEngineCommand::PlayNow {
                sound_id: id,
                volume: normalized_volume(action.volume * bgm_volume),
                loop_playback: true,
            },
        ],
        SkinAudioActionKind::Stop => vec![AudioEngineCommand::StopSound { id }],
    }
}

fn normalized_volume(volume: f32) -> f32 {
    if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use bmz_audio::command::AudioEngineHandle;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::sample::DecodedSample;

    use super::*;

    fn sample(value: f32, frames: usize) -> DecodedSample {
        DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![value; frames] }
    }

    fn render(processor: &mut bmz_audio::command::CommandedAudioEngine, frames: usize) -> Vec<f32> {
        let mut output = vec![0.0; frames * 2];
        assert!(processor.render_stereo(0, &mut output));
        output
    }

    /// 48 kHz mono 16-bit PCM of a constant sample.
    fn write_wav(path: &Path, frames: u32, value: i16) {
        let data_len = frames * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for _ in 0..frames {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn wait_until_loaded(runtime: &SkinAudioRuntime, path: &Path) {
        let started = Instant::now();
        while matches!(*runtime.files[path].state.lock().unwrap(), SkinAudioFileState::Loading(_)) {
            assert!(started.elapsed() < Duration::from_secs(10), "skin audio decode timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn action(action: SkinAudioActionKind, path: &Path, volume: f32) -> SkinAudioActionDef {
        SkinAudioActionDef { action, path: path.to_string_lossy().into_owned(), volume }
    }

    #[test]
    fn scene_loop_and_once_timer_event_execute_declaratively() {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let mut processor = engine.processor();
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "sceneAudio": [
                    { "action": "loop", "path": "bgm.ogg", "volume": 0.5 }
                ],
                "customEvents": [
                    {
                        "id": 1001,
                        "timer": 2,
                        "once": true,
                        "audioActions": [
                            { "action": "stop", "path": "bgm.ogg" },
                            { "action": "play", "path": "close.ogg", "volume": 0.25 }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();
        let assets = vec![
            DecodedSkinAudio {
                path: "bgm.ogg".to_string(),
                resolved: None,
                sample: sample(1.0, 16),
            },
            DecodedSkinAudio {
                path: "close.ogg".to_string(),
                resolved: None,
                sample: sample(0.5, 1),
            },
        ];
        let mut runtime = SkinAudioRuntime::install(engine, &document, assets);

        assert!(runtime.start_scene(0.5, 1.0));
        assert_eq!(render(&mut processor, 1), vec![0.25, 0.25]);

        assert!(runtime.trigger_timer(2, 0.5, 0.8));
        assert_eq!(render(&mut processor, 1), vec![0.1, 0.1]);
        assert!(!runtime.trigger_timer(2, 0.5, 0.8));
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);
    }

    #[test]
    fn lua_actions_decode_new_files_in_the_background_and_keep_their_order() {
        let root = crate::test_dir::TestDir::new("bmz-skin-lua-audio");
        std::fs::create_dir_all(&*root).unwrap();
        let path = root.join("open.wav");
        write_wav(&path, 4_800, 16_384);
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let mut processor = engine.processor();
        let mut runtime = SkinAudioRuntime::for_lua(engine, SkinRuntimeScene::Select);

        // Nothing to stop before the file was played.
        assert!(!runtime.run_lua_action(&action(SkinAudioActionKind::Stop, &path, 1.0), 1.0, 1.0));
        assert!(runtime.files.is_empty());
        // A play and a stop sent while decoding run in order once it is ready.
        assert!(runtime.run_lua_action(&action(SkinAudioActionKind::Play, &path, 1.0), 1.0, 1.0));
        assert!(runtime.run_lua_action(&action(SkinAudioActionKind::Stop, &path, 1.0), 1.0, 1.0));
        wait_until_loaded(&runtime, &path);
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);
        assert_eq!(runtime.files[&path].id, SoundId(100_100 + 128 + 64));

        // Loops use the BGM volume and stop with the scene; one-shot sounds use the SE volume.
        assert!(runtime.run_lua_action(&action(SkinAudioActionKind::Loop, &path, 1.0), 0.5, 1.0));
        assert_eq!(render(&mut processor, 1), vec![0.25, 0.25]);
        assert!(runtime.stop_loops());
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);
        assert!(runtime.run_lua_action(&action(SkinAudioActionKind::Play, &path, 2.0), 1.0, 0.25));
        assert_eq!(render(&mut processor, 1), vec![0.25, 0.25]);
        assert!(!runtime.stop_loops(), "one-shot sounds are not loops");

        // A file that fails to decode is skipped afterwards.
        let broken = root.join("broken.wav");
        std::fs::write(&broken, b"not audio").unwrap();
        assert!(runtime.run_lua_action(&action(SkinAudioActionKind::Play, &broken, 1.0), 1.0, 1.0));
        wait_until_loaded(&runtime, &broken);
        assert!(!runtime.run_lua_action(
            &action(SkinAudioActionKind::Play, &broken, 1.0),
            1.0,
            1.0
        ));
    }

    #[test]
    fn lua_actions_reuse_declared_result_assets_by_resolved_path() {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let mut processor = engine.processor();
        let document: SkinDocument =
            serde_json::from_str(r#"{ "sceneAudio": [{ "action": "loop", "path": "bgm.ogg" }] }"#)
                .unwrap();
        let resolved = PathBuf::from("skins/result/bgm.ogg");
        let assets = vec![DecodedSkinAudio {
            path: "bgm.ogg".to_string(),
            resolved: Some(resolved.clone()),
            sample: sample(0.5, 16),
        }];
        let mut runtime = SkinAudioRuntime::install(engine, &document, assets);
        assert!(runtime.start_scene(1.0, 1.0));
        assert_eq!(render(&mut processor, 1), vec![0.5, 0.5]);
        // A Lua act stops the scene loop it did not start.
        assert!(runtime.run_lua_action(
            &action(SkinAudioActionKind::Stop, &resolved, 1.0),
            1.0,
            1.0
        ));
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);
        assert_eq!(runtime.files.len(), 1, "the declared sample is not decoded again");
    }

    #[test]
    fn unloaded_skin_decode_cannot_overwrite_a_reused_sound_id() {
        let root = crate::test_dir::TestDir::new("bmz-skin-stale-audio");
        std::fs::create_dir_all(&*root).unwrap();
        let old_path = root.join("old.wav");
        let new_path = root.join("new.wav");
        write_wav(&old_path, 4_800, 16_384);
        write_wav(&new_path, 4_800, 24_576);
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let mut processor = engine.processor();
        let mut old = SkinAudioRuntime::for_lua(engine.clone(), SkinRuntimeScene::Select);
        // Hold the old worker just before decode completes, without racing a thread.
        let worker = Arc::new(SkinAudioFile {
            id: SoundId(old.next_runtime_id),
            state: Mutex::new(SkinAudioFileState::Loading(Vec::new())),
        });
        old.files.insert(old_path.clone(), Arc::clone(&worker));
        drop(old);

        let mut current = SkinAudioRuntime::for_lua(engine, SkinRuntimeScene::Select);
        assert!(current.run_lua_action(
            &action(SkinAudioActionKind::Loop, &new_path, 1.0),
            1.0,
            1.0
        ));
        wait_until_loaded(&current, &new_path);
        assert_eq!(current.files[&new_path].id, worker.id, "the new skin reuses this scene's IDs");
        assert_eq!(render(&mut processor, 1), vec![0.75, 0.75]);
        assert!(current.stop_loops());
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);

        worker.decode(&current.engine, &old_path);
        assert!(current.run_lua_action(
            &action(SkinAudioActionKind::Play, &new_path, 1.0),
            1.0,
            1.0
        ));
        assert_eq!(render(&mut processor, 1), vec![0.75, 0.75]);
        assert!(!worker.dispatch(
            &current.engine,
            vec![AudioEngineCommand::PlayNow {
                sound_id: worker.id,
                volume: 1.0,
                loop_playback: false,
            }]
        ));
    }
}
