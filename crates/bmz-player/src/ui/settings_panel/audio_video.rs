use super::*;

pub(super) struct AudioVideoSectionContext<'a, 'state> {
    pub(super) window: &'a Window,
    pub(super) config: &'a mut AppConfig,
    pub(super) profile: &'a mut ProfileConfig,
    pub(super) show_fps: &'a mut bool,
    pub(super) text: Localizer,
    pub(super) state: &'a mut SettingsPanelState<'state>,
    pub(super) apply_audio: &'a mut bool,
    pub(super) save_profile: &'a mut bool,
    pub(super) obs_enabled_changed: &'a mut bool,
}

pub(super) fn build_audio_video_settings_sections(
    ui: &mut egui::Ui,
    context: AudioVideoSectionContext<'_, '_>,
) {
    let AudioVideoSectionContext {
        window,
        config,
        profile,
        show_fps,
        text,
        state,
        apply_audio,
        save_profile,
        obs_enabled_changed,
    } = context;
    egui::CollapsingHeader::new(tr!(text, "settings-audio-title")).id_salt("settings_audio").show(
        ui,
        |ui| {
            let available_audio_backends = crate::audio::available_audio_backends();
            if !available_audio_backends.contains(&config.audio.backend) {
                config.audio.backend = AudioBackend::Auto;
            }
            egui::ComboBox::new("audio_backend", tr!(text, "settings-backend"))
                .selected_text(audio_backend_label(&config.audio.backend, text))
                .show_ui(ui, |ui| {
                    for backend in &available_audio_backends {
                        ui.selectable_value(
                            &mut config.audio.backend,
                            backend.clone(),
                            audio_backend_label(backend, text),
                        );
                    }
                });
            if config.audio.backend == AudioBackend::Wasapi {
                egui::ComboBox::new("audio_output_mode", tr!(text, "settings-audio-output-mode"))
                    .selected_text(audio_output_mode_label(&config.audio.output_mode, text))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config.audio.output_mode,
                            AudioOutputMode::Shared,
                            tr!(text, "settings-audio-output-mode-shared"),
                        );
                        ui.selectable_value(
                            &mut config.audio.output_mode,
                            AudioOutputMode::SharedLowLatency,
                            tr!(text, "settings-audio-output-mode-low-latency"),
                        );
                    });
                if config.audio.output_mode == AudioOutputMode::SharedLowLatency {
                    ui.label(tr!(text, "settings-audio-low-latency-help"));
                }
            }
            let sample_rate_text = if config.audio.sample_rate_mode == AudioSampleRateMode::Auto {
                tr!(text, "settings-audio-auto-driver-default")
            } else {
                audio_sample_rate_label(config.audio.sample_rate)
            };
            egui::ComboBox::new("audio_sample_rate", tr!(text, "settings-audio-sample-rate"))
                .selected_text(sample_rate_text)
                .show_ui(ui, |ui| {
                    let is_auto = config.audio.sample_rate_mode == AudioSampleRateMode::Auto;
                    if ui
                        .selectable_label(is_auto, tr!(text, "settings-audio-auto-driver-default"))
                        .clicked()
                    {
                        config.audio.sample_rate_mode = AudioSampleRateMode::Auto;
                    }
                    for hz in [44_100u32, 48_000, 96_000, 192_000, 384_000] {
                        let selected = config.audio.sample_rate_mode == AudioSampleRateMode::Fixed
                            && config.audio.sample_rate == hz;
                        if ui.selectable_label(selected, audio_sample_rate_label(hz)).clicked() {
                            config.audio.sample_rate_mode = AudioSampleRateMode::Fixed;
                            config.audio.sample_rate = hz;
                        }
                    }
                });
            egui::ComboBox::new("audio_buffer_mode", tr!(text, "settings-audio-buffer-mode"))
                .selected_text(audio_buffer_size_mode_label(&config.audio.buffer_size_mode, text))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.audio.buffer_size_mode,
                        AudioBufferSizeMode::Auto,
                        tr!(text, "common-auto"),
                    );
                    ui.selectable_value(
                        &mut config.audio.buffer_size_mode,
                        AudioBufferSizeMode::Fixed,
                        tr!(text, "common-fixed"),
                    );
                });
            if config.audio.buffer_size_mode == AudioBufferSizeMode::Fixed {
                ui.add(
                    egui::Slider::new(&mut config.audio.buffer_size, 32..=4096)
                        .text(tr!(text, "settings-audio-buffer-frames")),
                );
                ui.horizontal(|ui| {
                    ui.label(tr!(text, "settings-audio-presets"));
                    for frames in [32u32, 48, 64, 96, 128, 256] {
                        if ui.button(frames.to_string()).clicked() {
                            config.audio.buffer_size = frames;
                            config.audio.buffer_size_mode = AudioBufferSizeMode::Fixed;
                        }
                    }
                });
            }
            // ASIO 以外は安価なのでバックエンド変更時に自動列挙する。
            // ASIO はドライバ初期化を伴い得るため、更新ボタンでのみ列挙する。
            let backend = config.audio.backend.clone();
            if backend != AudioBackend::Asio
                && state.audio_device_picker.backend.as_ref() != Some(&backend)
            {
                state.audio_device_picker.names = crate::audio::list_output_devices(&backend);
                state.audio_device_picker.backend = Some(backend);
            }

            ui.horizontal(|ui| {
                if ui.button(tr!(text, "settings-audio-refresh-devices")).clicked() {
                    state.audio_device_picker.names =
                        crate::audio::list_output_devices(&config.audio.backend);
                    state.audio_device_picker.backend = Some(config.audio.backend.clone());
                }
                ui.label(tr!(
                    text,
                    "common-count",
                    "count" => state.audio_device_picker.names.len()
                ));
            });

            if config.audio.backend == AudioBackend::Asio {
                egui::ComboBox::new("audio_asio_driver", tr!(text, "settings-audio-asio-driver"))
                    .selected_text(if config.audio.asio_driver.is_empty() {
                        tr!(text, "common-unspecified")
                    } else {
                        config.audio.asio_driver.clone()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config.audio.asio_driver,
                            String::new(),
                            tr!(text, "common-unspecified"),
                        );
                        for name in state.audio_device_picker.names.iter() {
                            ui.selectable_value(&mut config.audio.asio_driver, name.clone(), name);
                        }
                    });
            } else {
                egui::ComboBox::new(
                    "audio_output_device",
                    tr!(text, "settings-audio-output-device"),
                )
                .selected_text(if config.audio.output_device.is_empty() {
                    tr!(text, "common-default")
                } else {
                    config.audio.output_device.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.audio.output_device,
                        String::new(),
                        tr!(text, "common-default"),
                    );
                    for name in state.audio_device_picker.names.iter() {
                        ui.selectable_value(&mut config.audio.output_device, name.clone(), name);
                    }
                });
            }
            if config.audio.backend == AudioBackend::Asio {
                egui::ComboBox::new(
                    "audio_output_channel",
                    tr!(text, "settings-audio-output-channel"),
                )
                .selected_text(audio_channel_pair_label(config.audio.output_channel_pair))
                .show_ui(ui, |ui| {
                    for pair in 0u32..6 {
                        ui.selectable_value(
                            &mut config.audio.output_channel_pair,
                            pair,
                            audio_channel_pair_label(pair),
                        );
                    }
                });
                ui.label(tr!(text, "settings-audio-channel-help"));
            }
            ui.label(tr!(text, "settings-audio-asio-buffer-help"));
            if ui.button(tr!(text, "settings-audio-apply")).clicked() {
                *apply_audio = true;
            }
            ui.label(tr!(text, "settings-audio-apply-help"));
        },
    );

    egui::CollapsingHeader::new(tr!(text, "settings-video-title")).id_salt("settings_video").show(
        ui,
        |ui| {
            egui::ComboBox::new("video_window_mode", tr!(text, "settings-video-window-mode"))
                .selected_text(window_mode_label(&config.video.mode, text))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.video.mode,
                        WindowMode::Windowed,
                        tr!(text, "settings-windowed"),
                    );
                    ui.selectable_value(
                        &mut config.video.mode,
                        WindowMode::BorderlessFullscreen,
                        tr!(text, "settings-borderless-fullscreen"),
                    );
                    ui.selectable_value(
                        &mut config.video.mode,
                        WindowMode::ExclusiveFullscreen,
                        tr!(text, "settings-exclusive-fullscreen"),
                    );
                });
            ui.add(
                egui::Slider::new(&mut config.video.width, 640..=3840)
                    .text(tr!(text, "settings-video-width")),
            );
            ui.add(
                egui::Slider::new(&mut config.video.height, 480..=2160)
                    .text(tr!(text, "settings-video-height")),
            );
            egui::ComboBox::new(
                "video_internal_resolution",
                tr!(text, "settings-video-internal-resolution"),
            )
            .selected_text(internal_resolution_mode_label(&config.video.internal_resolution, text))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut config.video.internal_resolution,
                    InternalResolutionModeConfig::Native,
                    tr!(text, "settings-video-internal-resolution-native"),
                );
                ui.selectable_value(
                    &mut config.video.internal_resolution,
                    InternalResolutionModeConfig::Skin,
                    tr!(text, "settings-video-internal-resolution-skin"),
                );
            });
            let available_monitors = window.available_monitors().collect::<Vec<_>>();
            let selected_monitor = if config.video.monitor_name.is_empty() {
                tr!(text, "settings-video-primary-monitor")
            } else if available_monitors
                .iter()
                .any(|monitor| monitor_config_name(monitor) == config.video.monitor_name)
            {
                config.video.monitor_name.clone()
            } else {
                tr!(
                    text,
                    "settings-video-monitor-disconnected",
                    "name" => config.video.monitor_name.as_str()
                )
            };
            egui::ComboBox::new("video_monitor", tr!(text, "settings-video-monitor"))
                .selected_text(selected_monitor)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.video.monitor_name,
                        String::new(),
                        tr!(text, "settings-video-primary-monitor"),
                    );
                    for monitor in &available_monitors {
                        let name = monitor_config_name(monitor);
                        ui.selectable_value(&mut config.video.monitor_name, name.clone(), name);
                    }
                });
            egui::ComboBox::new("video_vsync_mode", tr!(text, "settings-video-vsync-mode"))
                .selected_text(vsync_mode_label(&config.video.vsync_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.video.vsync_mode,
                        VsyncModeConfig::Vsync,
                        vsync_mode_label(&VsyncModeConfig::Vsync),
                    );
                    ui.selectable_value(
                        &mut config.video.vsync_mode,
                        VsyncModeConfig::AdaptiveVsync,
                        vsync_mode_label(&VsyncModeConfig::AdaptiveVsync),
                    );
                    ui.selectable_value(
                        &mut config.video.vsync_mode,
                        VsyncModeConfig::VsyncOff,
                        vsync_mode_label(&VsyncModeConfig::VsyncOff),
                    );
                    ui.selectable_value(
                        &mut config.video.vsync_mode,
                        VsyncModeConfig::FastVsync,
                        vsync_mode_label(&VsyncModeConfig::FastVsync),
                    );
                });
            ui.add(
                egui::DragValue::new(&mut config.video.target_fps)
                    .range(0..=u32::MAX)
                    .speed(1.0)
                    .suffix(" FPS"),
            );
            ui.label(tr!(text, "settings-video-target-fps-unlimited"));
            if ui.checkbox(show_fps, tr!(text, "settings-show-fps")).changed() {
                profile.ui.show_fps = *show_fps;
                *save_profile = true;
            }
            ui.add(
                egui::Slider::new(&mut config.video.frame_limit_in_background, 1..=120)
                    .text(tr!(text, "settings-video-background-fps")),
            );
            let available_renderer_backends = available_renderer_backends();
            if !available_renderer_backends.contains(&config.video.renderer) {
                config.video.renderer = RendererBackend::Auto;
            }
            egui::ComboBox::new("video_renderer", tr!(text, "settings-video-renderer"))
                .selected_text(renderer_backend_label(&config.video.renderer, text))
                .show_ui(ui, |ui| {
                    for backend in &available_renderer_backends {
                        ui.selectable_value(
                            &mut config.video.renderer,
                            backend.clone(),
                            renderer_backend_label(backend, text),
                        );
                    }
                });
            ui.label(tr!(text, "settings-video-apply-help"));
        },
    );

    egui::CollapsingHeader::new(tr!(text, "settings-screenshot-title"))
        .id_salt("settings_screenshot")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-screenshot-directory"));
                ui.add(
                    egui::TextEdit::singleline(&mut config.screenshot.dir)
                        .desired_width(300.0)
                        .hint_text("screenshots"),
                );
            });
            ui.horizontal(|ui| {
                if ui.button(tr!(text, "common-choose-folder")).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    config.screenshot.dir = dir.to_string_lossy().into_owned();
                }
                ui.checkbox(
                    &mut config.screenshot.copy_to_clipboard,
                    tr!(text, "settings-screenshot-copy-clipboard"),
                );
            });
        });

    *obs_enabled_changed |= build_obs_settings_section(
        ui,
        config,
        state.obs_scene_picker,
        state.obs_connection_status,
        text,
    );
    *save_profile |= build_play_overlay_settings_section(ui, profile, text);
}
