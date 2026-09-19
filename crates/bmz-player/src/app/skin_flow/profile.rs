use super::*;

impl WinitApp {
    pub(super) fn sync_realtime_profile_settings(&mut self) {
        self.sync_active_play_realtime_profile_settings();
        let mut needs_system_sound_analysis = false;
        if let Some(manager) = &self.audio.system_sound {
            let mix = self.boot.profile_config.audio_mix.clone();
            manager.set_bgm_normalization_enabled(mix.normalize_system_bgm_volume);
            needs_system_sound_analysis =
                mix.normalize_system_bgm_volume && !manager.normalization_analysis_enabled();
            let preview_factor = select_preview_fade_factor(
                self.select.select_assets.preview_fade(),
                Instant::now(),
            );
            manager.refresh_volumes(|sound_type| {
                let volume = system_sound_volume_from_mix(&mix, sound_type);
                if sound_type == crate::system_sound::SoundType::Select {
                    volume * (1.0 - preview_factor).clamp(0.0, 1.0)
                } else {
                    volume
                }
            });
        }
        if needs_system_sound_analysis && self.audio.pending_system_sound.is_none() {
            self.start_system_sound_load();
        }
        self.apply_select_preview_audio_mix();
    }

    pub(super) fn sync_active_play_lane_settings_from_profile(
        &mut self,
        before: &LaneViewConfig,
        before_lane_effect: LaneEffectConfig,
    ) {
        let speed_locked = active_course_speed_locked(self.play.active_course.as_ref());
        let profile_lane = self.boot.profile_config.lane.clone();
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };
        let before = before.clone();
        let lane_effect = self.boot.profile_config.play.lane_effect;
        let practice_mode = active_play.running.practice_mode;
        if active_play.running.gameplay.edit(move |session| {
            apply_profile_lane_settings_to_session(
                session,
                &before,
                before_lane_effect,
                &profile_lane,
                lane_effect,
                speed_locked,
                practice_mode,
            );
        }) {
            update_pre_ready_play_snapshot_options_for_runtime(
                self.play.play_ready_sound_started_at,
                &mut self.play.last_play_snapshot,
                &active_play.running.gameplay,
                &active_play.running.applied_arrange,
            );
            tracing::info!(
                hispeed = active_play.running.session.hispeed,
                hispeed_mode = ?active_play.running.session.hispeed_mode,
                target_green_number = active_play.running.session.target_green_number,
                lane_cover = active_play.running.session.lane_cover,
                lift = active_play.running.session.lift,
                "applied egui lane settings to active play"
            );
        }
    }

    pub(super) fn sync_active_play_realtime_profile_settings(&mut self) {
        if let Some(active_play) = &mut self.play.active_play {
            let profile = self.boot.profile_config.clone();
            active_play.running.gameplay.edit(move |session| {
                let key_mode = session.play_config_key_mode;
                let chart_normalization_gain = session.audio_mix.chart_normalization_gain;
                session.audio_mix = crate::config::play::audio_mix_from_profile_with_chart_gain(
                    &profile,
                    chart_normalization_gain,
                );
                session.offsets =
                    crate::config::play::play_offsets_from_profile_for_mode(&profile, key_mode);
                session.input_offset_auto_adjust_enabled = profile.judge.visual_offset_auto_adjust;
                session.guide_se_enabled = profile.play.guide_se;
                let auto_adjust_available = session.replay_player.is_none()
                    && !session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_full());
                if session.input_offset_auto_adjust_enabled && auto_adjust_available {
                    session.input_offset_auto_adjust.get_or_insert_with(Default::default);
                } else {
                    session.input_offset_auto_adjust = None;
                }
            });
        }
    }

    pub(super) fn sync_profile_visual_offset_from_active_play(&mut self) {
        let Some((key_mode, visual_offset_us, auto_adjust_active)) =
            self.play.active_play.as_ref().map(|active| {
                (
                    active.running.session.play_config_key_mode,
                    active.running.session.offsets.visual_offset_us,
                    active.running.session.input_offset_auto_adjust.is_some(),
                )
            })
        else {
            return;
        };
        self.boot.profile_config.activate_play_mode(key_mode);
        sync_active_play_visual_offset_to_profile(
            &mut self.boot.profile_config,
            visual_offset_us,
            auto_adjust_active,
        );
        self.boot.profile_config.sync_active_play_mode();
    }

    pub(super) fn skin_defs_for_path(&mut self, path: &str, play: bool) -> SceneSkinDefs {
        let key = (path.trim().to_string(), play);
        if let Some(defs) = self.skin.skin_defs_cache.get(&key) {
            return defs.clone();
        }
        let defs = skin_defs_from_path(&self.boot.app_paths, &key.0, play);
        self.skin.skin_defs_cache.insert(key, defs.clone());
        defs
    }

    pub(super) fn reset_skin_config_from_disk(&mut self) {
        match load_profile_config(&self.boot.profile_paths.profile_toml) {
            Ok(profile) => {
                replace_skin_config_from_loaded_profile(&mut self.boot.profile_config, profile);
                self.apply_profile_skin_offsets_to_active_play();
                self.reload_skins(SkinReloadRequest {
                    select: true,
                    decide: true,
                    result: true,
                    course_result: true,
                    play4: true,
                    play5: true,
                    play6: true,
                    play7: true,
                    play8: true,
                    play9: true,
                    play10: true,
                    play14: true,
                    offsets: true,
                });
                tracing::info!("skin config reset from profile.toml");
            }
            Err(error) => {
                tracing::error!(
                    path = %self.boot.profile_paths.profile_toml.display(),
                    %error,
                    "failed to reset skin config from profile.toml"
                );
            }
        }
    }

    pub(super) fn apply_profile_skin_offsets_to_active_play(&mut self) {
        let Some(key_mode) = self
            .play
            .active_play
            .as_ref()
            .map(|active_play| active_play.running.session.play_config_key_mode)
        else {
            return;
        };
        let offsets = play_skin_selection_for_session(
            &self.boot.profile_config.skin,
            key_mode,
            self.select.session_mode,
        )
        .offsets
        .iter()
        .map(|offset| PlaySkinOffset {
            id: offset.id,
            x: offset.x,
            y: offset.y,
            w: offset.w,
            h: offset.h,
            r: offset.r,
            a: offset.a,
        })
        .collect();
        if let Some(active_play) = &mut self.play.active_play {
            active_play.running.gameplay.edit(move |session| session.skin_offsets = offsets);
        }
    }
}
