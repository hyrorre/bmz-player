use super::*;

pub(in crate::ui) fn build_settings_panel(
    ui: &mut egui::Ui,
    window: &Window,
    config: &mut AppConfig,
    profile: &mut ProfileConfig,
    show_fps: &mut bool,
    editable: bool,
    difficulty_tables: &[DifficultyTableRecord],
    text: Localizer,
    mut state: SettingsPanelState<'_>,
) -> SettingsPanelActions {
    let mut save_clicked = false;
    let mut obs_enabled_changed = false;
    let mut save_profile = false;
    let mut rescan_clicked = false;
    let mut check_update_clicked = false;
    let mut song_scan_requests = Vec::new();
    let mut table_fetch_urls = Vec::new();
    let mut score_import_request = None;
    let mut replay_import_request = None;
    let mut cancel_replay_import = false;
    let mut apply_audio = false;
    let unrestricted = editable;
    let editable = editable || SettingsNavigation::load(ui.ctx()).page.editable_during_play();
    // Disabled widgets can still normalize values while building their contents.
    let mut readonly_config = (!editable).then(|| config.clone());
    let config = readonly_config.as_mut().unwrap_or(config);
    if !editable {
        ui.label(tr!(text, "settings-disabled-during-play"));
        ui.separator();
    }
    ui.add_enabled_ui(editable, |ui| {
        build_library_settings_sections(
            ui,
            config,
            difficulty_tables,
            text,
            &mut state,
            LibrarySettingsActions {
                save_clicked: &mut save_clicked,
                rescan_clicked: &mut rescan_clicked,
                song_scan_requests: &mut song_scan_requests,
                table_fetch_urls: &mut table_fetch_urls,
                score_import_request: &mut score_import_request,
                replay_import_request: &mut replay_import_request,
                cancel_replay_import: &mut cancel_replay_import,
            },
        );
        build_audio_video_settings_sections(
            ui,
            AudioVideoSectionContext {
                window,
                unrestricted,
                config,
                profile,
                show_fps,
                text,
                state: &mut state,
                apply_audio: &mut apply_audio,
                save_profile: &mut save_profile,
                obs_enabled_changed: &mut obs_enabled_changed,
            },
        );
        build_integration_settings_sections(
            ui,
            config,
            text,
            &mut state,
            &mut save_clicked,
            &mut check_update_clicked,
        );
    });
    SettingsPanelActions {
        save: save_clicked || apply_audio,
        obs_enabled_changed,
        save_profile,
        check_update: check_update_clicked,
        rescan: rescan_clicked,
        song_scan_requests,
        table_fetch_urls,
        score_import_request,
        replay_import_request,
        cancel_replay_import,
        apply_audio,
    }
}
