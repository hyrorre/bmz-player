use super::*;

impl WinitApp {
    pub(super) fn update_window_title_for_scene(&mut self, scene_kind: AppSceneKind) {
        // maintenance workerをscene遷移と同じframeで止める。titleが変わらないframeでも
        // deferred boot解消などでmaintenance可否が変わるため、early returnより前に同期する。
        self.sync_select_maintenance_gate();
        let scene_changed = self.integrations.last_scene_kind != Some(scene_kind);
        self.notify_obs_scene(scene_kind);
        if !scene_changed {
            return;
        }

        let previous = self.integrations.last_scene_kind;
        self.integrations.last_scene_kind = Some(scene_kind);
        if previous == Some(AppSceneKind::Select) && scene_kind != AppSceneKind::Select {
            self.stop_select_preview();
        }
        if should_shuffle_system_sound_sets_on_scene_enter(previous, scene_kind) {
            self.shuffle_system_sound_sets();
        }
        self.fire_scene_transition_sounds(scene_kind);
        if let Some(window) = &self.window {
            window.set_title(window_title_for_scene(scene_kind));
        }
        self.publish_discord_presence_for_scene(scene_kind);
        tracing::info!(scene = ?scene_kind, title = window_title_for_scene(scene_kind), "app scene active");
    }

    pub(super) fn sync_discord_presence_config(&mut self) {
        let desired = DiscordPresenceConfig::from_app_config(&self.boot.app_config.discord);
        if self.integrations.discord_presence_config == desired {
            return;
        }

        if let Some(handle) = self.integrations.discord_presence.take() {
            handle.shutdown();
        }
        self.integrations.discord_presence_config = desired.clone();
        if let Some(config) = desired {
            let handle = DiscordPresenceHandle::start(config);
            handle.update(self.discord_presence_for_scene(self.current_scene_kind()));
            self.integrations.discord_presence = Some(handle);
            tracing::info!("Discord Rich Presence enabled");
        } else {
            tracing::info!("Discord Rich Presence disabled");
        }
    }

    pub(super) fn publish_discord_presence_for_scene(&self, scene_kind: AppSceneKind) {
        if let Some(handle) = &self.integrations.discord_presence {
            handle.update(self.discord_presence_for_scene(scene_kind));
        }
    }

    pub(super) fn discord_presence_for_scene(&self, scene_kind: AppSceneKind) -> DiscordPresence {
        let started_at = now_unix_seconds();
        match scene_kind {
            AppSceneKind::Select => DiscordPresence::select(started_at),
            AppSceneKind::Decide => DiscordPresence::decide(started_at),
            AppSceneKind::Play => {
                if let Some(active_play) = &self.play.active_play {
                    let metadata = &active_play.running.session.chart.metadata;
                    let key_mode = discord_key_mode_label(metadata.key_mode);
                    let title = discord_join_metadata(&metadata.title, &metadata.subtitle, " ");
                    let artist =
                        discord_join_metadata(&metadata.artist, &metadata.subartist, " / ");
                    DiscordPresence::play(
                        started_at,
                        Some(&key_mode),
                        title.as_deref(),
                        artist.as_deref(),
                        self.discord_presence_show_song_details(),
                    )
                } else {
                    DiscordPresence::play(
                        started_at,
                        None,
                        None,
                        None,
                        self.discord_presence_show_song_details(),
                    )
                }
            }
            AppSceneKind::Result if self.result.finished_course.is_some() => {
                DiscordPresence::course_result(started_at)
            }
            AppSceneKind::Result => DiscordPresence::result(started_at),
        }
    }

    pub(super) fn discord_presence_show_song_details(&self) -> bool {
        self.integrations
            .discord_presence_config
            .as_ref()
            .map(DiscordPresenceConfig::show_song_details)
            .unwrap_or(self.boot.app_config.discord.show_song_details)
    }

    pub(super) fn sync_obs_controller(&mut self) {
        if self.integrations.applied_obs_config == self.boot.app_config.obs {
            return;
        }
        self.integrations.applied_obs_config = self.boot.app_config.obs.clone();
        self.integrations.obs_controller =
            crate::obs::ObsController::spawn(self.integrations.applied_obs_config.clone());
        self.integrations.last_obs_event_key = None;
        self.notify_obs_scene(self.current_scene_kind());
        tracing::info!(
            enabled = self.integrations.applied_obs_config.enabled,
            "OBS WebSocket config applied"
        );
    }

    pub(super) fn sync_play_overlay_controller(&mut self) {
        let desired = crate::play_overlay::PlayOverlayServerConfig::from(
            &self.boot.profile_config.play_overlay,
        );
        if self.integrations.applied_play_overlay_config == desired {
            return;
        }
        self.integrations.applied_play_overlay_config = desired;
        self.integrations.play_overlay_controller.apply_config(desired);
    }

    pub(super) fn notify_obs_scene(&mut self, scene_kind: AppSceneKind) {
        let key = self.obs_event_key_for_scene(scene_kind);
        if self.integrations.last_obs_event_key == Some(key) {
            return;
        }
        self.integrations.last_obs_event_key = Some(key);
        if let Some(obs) = &self.integrations.obs_controller {
            obs.scene(key);
        }
    }

    pub(super) fn notify_obs_play_ended(&self) {
        if let Some(obs) = &self.integrations.obs_controller {
            obs.play_ended();
        }
    }

    pub(super) fn notify_obs_retry_play(&self) {
        if let Some(obs) = &self.integrations.obs_controller {
            obs.retry_play();
        }
    }

    pub(super) fn notify_obs_save_recording(&self, reason: crate::obs::ObsRecordingSaveReason) {
        if let Some(obs) = &self.integrations.obs_controller {
            obs.save_last_recording(reason);
        }
    }

    pub(super) fn obs_event_key_for_scene(
        &self,
        scene_kind: AppSceneKind,
    ) -> crate::obs::ObsEventKey {
        match scene_kind {
            AppSceneKind::Select => crate::obs::ObsEventKey::MusicSelect,
            AppSceneKind::Decide => crate::obs::ObsEventKey::Decide,
            AppSceneKind::Play => crate::obs::ObsEventKey::Play,
            AppSceneKind::Result if self.result.finished_course.is_some() => {
                crate::obs::ObsEventKey::CourseResult
            }
            AppSceneKind::Result => crate::obs::ObsEventKey::Result,
        }
    }
    /// シーン遷移時のシステム SE / BGM を発火する。
    /// Play 入口では Decide 音を曲開始まで残し、それ以外では進行中の BGM を止める。
    pub(super) fn fire_scene_transition_sounds(&self, scene_kind: AppSceneKind) {
        use crate::system_sound::SoundType;
        for sound_type in system_bgm_stop_targets_on_scene_enter(scene_kind) {
            self.stop_system_sound(*sound_type);
        }
        match scene_kind {
            AppSceneKind::Select
                if should_play_select_bgm_on_enter(self.select.select_assets.preview_playing()) =>
            {
                self.play_system_sound(SoundType::Select);
            }
            AppSceneKind::Select => {}
            AppSceneKind::Decide => self.play_system_sound(SoundType::Decide),
            AppSceneKind::Play => {}
            AppSceneKind::Result => {
                let Some(finished) = self.result.finished_play.as_ref() else {
                    return;
                };
                let clear_type = result_entry_clear_type_for_sound(finished);
                self.play_system_sound(result_entry_sound_for_clear(clear_type));
            }
        }
    }

    /// beatoraja の `SystemSoundManager.shuffle()` と同様に、起動時にスキャンした候補から
    /// BGM / SE セットを再抽選する。旧セットの BGM は、同じ SoundId へ新しいサンプルを
    /// 登録する前に停止する。
    fn shuffle_system_sound_sets(&mut self) {
        let Some(system_audio) = self.audio.system_audio.as_ref() else {
            return;
        };
        if let Some(manager) = &self.audio.system_sound {
            manager.stop_all_bgm();
        }
        self.audio.system_sound =
            Some(system_sound_manager_from_catalog(&self.audio.system_sound_catalog, system_audio));
    }

    /// `profile.audio_mix.system_bgm_volume` / `system_se_volume` に
    /// `master_volume` を乗算してシステム音を鳴らす。
    /// ボリュームは AudioEngine 側で 0.0..=1.0 にクランプされる。
    pub(super) fn play_system_sound(&self, sound_type: crate::system_sound::SoundType) {
        if let Some(manager) = &self.audio.system_sound {
            manager.play_with_master_gain(
                sound_type,
                system_sound_volume_from_mix(&self.boot.profile_config.audio_mix, sound_type),
                1.0,
            );
            self.start_audio_output_stream();
        }
    }

    pub(super) fn result_skin_audio_volumes(&self) -> (f32, f32) {
        let mix = &self.boot.profile_config.audio_mix;
        let master = crate::config::play::volume_unit_to_f32(mix.master_volume);
        let bgm = master * crate::config::play::volume_unit_to_f32(mix.system_bgm_volume);
        let se = master * crate::config::play::volume_unit_to_f32(mix.system_se_volume);
        (bgm.clamp(0.0, 1.0), se.clamp(0.0, 1.0))
    }

    pub(super) fn play_course_result_entry_sound(&self, clear_type: bmz_core::clear::ClearType) {
        use crate::system_sound::SoundType;
        for sound in [
            SoundType::ResultClear,
            SoundType::ResultFail,
            SoundType::ResultClose,
            SoundType::CourseClear,
            SoundType::CourseFail,
            SoundType::CourseClose,
        ] {
            self.stop_system_sound(sound);
        }
        let preferred = course_result_entry_sound_for_clear(clear_type);
        let sound = if self.system_sound_has(preferred) {
            preferred
        } else {
            result_entry_sound_for_clear(clear_type)
        };
        self.play_system_sound(sound);
    }

    pub(super) fn system_sound_has(&self, sound_type: crate::system_sound::SoundType) -> bool {
        self.audio.system_sound.as_ref().is_some_and(|manager| manager.has_sound(sound_type))
    }

    pub(super) fn start_audio_output_stream(&self) {
        let Some(runtime) = &self.audio.audio_runtime else {
            return;
        };
        if let Err(error) = runtime.play() {
            tracing::warn!(%error, "failed to start shared audio output stream");
        }
    }

    /// 譜面側にキー音がない Mine を踏んだフレームで既定の地雷 SE を鳴らす。
    /// 複数同時ヒットでも1回にまとめる (`hits == 0` のときは no-op)。
    pub(super) fn play_landmine_se(&self, hits: usize) {
        if hits == 0 {
            return;
        }
        self.play_system_sound(crate::system_sound::SoundType::Landmine);
    }

    pub(super) fn stop_system_sound(&self, sound_type: crate::system_sound::SoundType) {
        if let Some(manager) = &self.audio.system_sound {
            manager.stop(sound_type);
        }
    }
}
