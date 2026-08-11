use super::*;

/// 本体設定パネルからのアクション要求。
pub(super) struct SettingsPanelActions {
    pub(super) save: bool,
    pub(super) obs_enabled_changed: bool,
    pub(super) save_profile: bool,
    pub(super) check_update: bool,
    pub(super) rescan: bool,
    pub(super) song_scan_requests: Vec<SongScanRequest>,
    pub(super) table_fetch_urls: Vec<String>,
    pub(super) score_import_request: Option<ScoreImportRequest>,
    /// 音声出力(cpal ストリーム)を現在の設定で開き直す要求。
    pub(super) apply_audio: bool,
}

pub(super) struct SettingsPanelState<'a> {
    pub(super) new_root_path: &'a mut String,
    pub(super) add_root_error: &'a mut String,
    pub(super) new_table_url: &'a mut String,
    pub(super) add_table_error: &'a mut String,
    pub(super) score_import_path: &'a mut String,
    pub(super) score_import_kind: &'a mut ScoreImportKind,
    pub(super) score_import_device_type: &'a mut InputDeviceKind,
    pub(super) score_import_status: &'a str,
    pub(super) score_import_error: &'a str,
    pub(super) audio_device_picker: &'a mut AudioDevicePickerState,
    pub(super) obs_scene_picker: &'a mut ObsScenePickerState,
    pub(super) obs_connection_status: &'a crate::obs::ObsConnectionStatus,
    pub(super) connected_gamepads: &'a [crate::input::gamepad::ConnectedGamepad],
}

#[derive(Default)]
pub(super) struct ObsScenePickerState {
    busy: bool,
    scenes: Vec<String>,
    message: String,
    error: String,
    receiver: Option<std::sync::mpsc::Receiver<Result<crate::obs::ObsSceneList, String>>>,
}

impl ObsScenePickerState {
    fn poll(&mut self, text: Localizer) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        self.receiver = None;
        self.busy = false;
        match result {
            Ok(list) => {
                self.scenes = list.scenes;
                self.error.clear();
                self.message = tr!(
                    text,
                    "settings-obs-scenes-loaded",
                    "count" => self.scenes.len(),
                    "version" => list.version,
                    "recording" => if list.recording_active { "ON" } else { "OFF" }
                );
            }
            Err(error) => {
                self.message.clear();
                self.error = error;
            }
        }
    }

    fn start_load(&mut self, config: crate::config::app_config::ObsConfig) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.message.clear();
        self.error.clear();
        tokio::spawn(async move {
            let result =
                crate::obs::load_scenes(config).await.map_err(|error| format!("{error:#}"));
            let _ = sender.send(result);
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsListAction {
    MoveUp(usize),
    MoveDown(usize),
    MoveTo { from: usize, to: usize },
    Remove(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsDragList {
    SongRoots,
    TableSources,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SettingsDragPayload {
    list: SettingsDragList,
    index: usize,
}

const SETTINGS_LIST_BUTTONS_WIDTH: f32 = 224.0;
const SETTINGS_TABLE_LIST_BUTTONS_WIDTH: f32 = 224.0;
const SETTINGS_TABLE_ENABLED_WIDTH: f32 = 56.0;
const SETTINGS_LIST_DRAG_HANDLE_WIDTH: f32 = 28.0;
const SETTINGS_LIST_MIN_LABEL_WIDTH: f32 = 96.0;

pub(super) fn apply_settings_list_action<T>(items: &mut Vec<T>, action: SettingsListAction) {
    match action {
        SettingsListAction::MoveUp(index) if index > 0 && index < items.len() => {
            items.swap(index - 1, index);
        }
        SettingsListAction::MoveDown(index) if index + 1 < items.len() => {
            items.swap(index, index + 1);
        }
        SettingsListAction::MoveTo { from, to }
            if from < items.len() && to < items.len() && from != to =>
        {
            let item = items.remove(from);
            items.insert(to.min(items.len()), item);
        }
        SettingsListAction::Remove(index) if index < items.len() => {
            items.remove(index);
        }
        _ => {}
    }
}

pub(super) fn settings_list_label_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - SETTINGS_LIST_BUTTONS_WIDTH).max(SETTINGS_LIST_MIN_LABEL_WIDTH)
}

pub(super) fn settings_list_label(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized([width, ui.spacing().interact_size.y], egui::Label::new(text).truncate())
        .on_hover_text(text);
}

pub(super) fn settings_drag_handle(
    ui: &mut egui::Ui,
    payload: SettingsDragPayload,
    text: Localizer,
) {
    let response = ui.add_sized(
        [SETTINGS_LIST_DRAG_HANDLE_WIDTH, ui.spacing().interact_size.y],
        egui::Button::new(egui::RichText::new("≡").size(18.0)).sense(egui::Sense::drag()),
    );
    response.dnd_set_drag_payload(payload);
    response
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text(tr!(text, "settings-drag-to-reorder"));
}

pub(super) fn settings_drag_ghost(
    ctx: &egui::Context,
    id: egui::Id,
    label: &str,
    label_width: f32,
    show_song_options: bool,
    text: Localizer,
) {
    let Some(pointer_pos) = ctx.pointer_interact_pos() else {
        return;
    };
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(pointer_pos + egui::vec2(10.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [SETTINGS_LIST_DRAG_HANDLE_WIDTH, ui.spacing().interact_size.y],
                        egui::Label::new(egui::RichText::new("≡").size(18.0)),
                    );
                    settings_list_label(ui, label, label_width);
                });
                if show_song_options {
                    let mut enabled = true;
                    let mut recursive = true;
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            false,
                            egui::Checkbox::new(&mut enabled, tr!(text, "common-enabled")),
                        );
                        ui.add_enabled(
                            false,
                            egui::Checkbox::new(
                                &mut recursive,
                                tr!(text, "settings-song-recursive"),
                            ),
                        );
                    });
                }
            });
        });
}

#[path = "settings_panel/audio_video.rs"]
mod audio_video;
#[path = "settings_panel/integration.rs"]
mod integration;
#[path = "settings_panel/labels.rs"]
mod labels;
#[path = "settings_panel/library.rs"]
mod library;
/// `AppConfig` を編集する本体設定パネル。
#[path = "settings_panel/main.rs"]
mod main;
#[path = "settings_panel/obs.rs"]
mod obs;
#[path = "settings_panel/score_import.rs"]
mod score_import;

use audio_video::{AudioVideoSectionContext, build_audio_video_settings_sections};
use integration::build_integration_settings_sections;
pub(super) use labels::*;
use library::build_library_settings_sections;
pub(super) use main::build_settings_panel;
pub(super) use obs::{build_obs_settings_section, build_play_overlay_settings_section};
pub(super) use score_import::build_score_import_section;
