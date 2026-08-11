//! 本体設定 / スキン設定 / デバッグ表示のための egui レイヤ。
//!
//! `egui::Context` と winit 連携状態 (`egui_winit::State`) を所有し、毎フレーム
//! UI を構築して描画プリミティブ (`EguiFrame`) を生成する。bmz-render はその
//! プリミティブをゲーム / スキン描画の上にペイントするだけにする。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bmz_core::input::InputDeviceKind;
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::ResultGradeDiffDisplay;
use bmz_render::skin::{SkinDocument, SkinFilepathDef, SkinOffsetDef, SkinPropertyDef};
use bmz_render::skin_offset::SKIN_OFFSET_BAR_LINE;
use bmz_render::ui::EguiFrame;
use egui::{NumExt, ViewportId};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::config::app_config::{
    AppConfig, AudioBackend, AudioBufferSizeMode, AudioOutputMode, AudioSampleRateMode,
    DifficultyTableSource, GamepadBackendKind, InputBackendKind, InternalResolutionModeConfig,
    LogLevel, ObsActionConfig, ObsRecordingMode, PathEntry, RendererBackend, UpdateChannelConfig,
    VsyncModeConfig, WindowMode,
};
use crate::config::play::{TARGET_GREEN_NUMBER_MAX, TARGET_GREEN_NUMBER_MIN};
use crate::config::profile_config::{
    BUILTIN_IR_PROVIDER_COUNT, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    DoubleOptionConfig, FastSlowDisplayScope, GaugeAutoShiftConfig, GaugeTypeConfig,
    HISPEED_STEP_MAX, HISPEED_STEP_MIN, HispeedModeConfig, HsFixConfig, IrConfig,
    IrCredentialStoreConfig, IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig,
    JudgeAlgorithmConfig, LaneEffectConfig, PlayOverlayControllerModeConfig,
    PlayOverlayReleaseDisplayModeConfig, PlayOverlayUpdateRateConfig, ProfileConfig,
    RELEASE_BOUNCE_MS_MAX, RandomOptionConfig, ReplaySlotRule, SkinConfig, SkinHistoryEntryConfig,
    SkinOffsetConfig, TargetOptionConfig, default_hispeed_step_fhs, default_hispeed_step_nhs,
    normalize_hispeed_step, normalized_ir_base_url,
};
use crate::i18n::{AppLocale, FluentArgs, Localizer};
use crate::ln_policy::LnPolicySetting;
use crate::logging::{LogEntry, LogLevel as TracingLogLevel};
use crate::paths::{AppPaths, resolve_app_paths};
use crate::practice_ui::{PracticePanelContext, build_practice_panel};
use crate::profile_cmd;
use crate::random_trainer::RandomTrainerState;
use crate::screens::course_session::CourseResultSummary;
use crate::screens::select_model::SelectCourseRow;
use crate::select_options::SessionMode;
use crate::skin_loader::{RANDOM_FILE_SELECTION, is_lua_skin_path};
use crate::songs_cmd::add_song_root_entry;
use crate::storage::difficulty_table_db::DifficultyTableRecord;
use crate::storage::score_import::{ScoreImportKind, ScoreImportRequest};
use crate::update::{UpdateAssetKind, UpdateCandidate, current_version};
use crate::window_config::monitor_config_name;

const BUNDLED_THIRD_PARTY_NOTICES: &str = include_str!("../../../THIRD-PARTY-NOTICES.txt");
const THIRD_PARTY_NOTICE_PATH: &str = "licenses/third-party-notices.txt";
const RUST_DEPENDENCY_LICENSE_PATH: &str = "licenses/rust-dependency-licenses.txt";
const LOCAL_RUST_DEPENDENCY_LICENSE_FILE: &str = "rust-dependency-licenses.txt";

macro_rules! tr {
    ($text:expr, $key:literal) => {
        $text.text($key)
    };
    ($text:expr, $key:literal, $($name:literal => $value:expr),+ $(,)?) => {{
        let mut args = FluentArgs::new();
        $(args.set($name, $value);)+
        $text.format($key, &args)
    }};
}

#[path = "ui/auxiliary_panels/course.rs"]
mod auxiliary_course;
#[path = "ui/auxiliary_panels/debug.rs"]
mod auxiliary_debug;
#[path = "ui/auxiliary_panels/notice.rs"]
mod auxiliary_notice;
#[path = "ui/auxiliary_panels/result_ir.rs"]
mod auxiliary_result_ir;
#[path = "ui/auxiliary_panels/update.rs"]
mod auxiliary_update;
#[path = "ui/auxiliary_panels/window.rs"]
mod auxiliary_window;
mod profile_panel;
mod settings_panel;
mod skin_panel;

use auxiliary_course::*;
use auxiliary_debug::*;
use auxiliary_notice::*;
use auxiliary_result_ir::*;
use auxiliary_update::*;
use auxiliary_window::*;
use profile_panel::*;
use settings_panel::*;
use skin_panel::*;

mod ir_state;
mod menu;
mod model;
mod runtime;

use ir_state::*;
use menu::*;
use model::*;
pub use model::{
    DebugInfo, EguiLayer, EguiOutput, EguiRunContext, SceneSkinDefs, SkinCandidate,
    SkinCandidateOrigin, SkinCatalog, SkinConfigMeta, SkinReloadRequest, SongScanRequest,
    UpdateDialog, UpdateDialogAction,
};
use runtime::AudioDevicePickerState;
#[cfg(test)]
use runtime::{cjk_font_definitions, egui_frame_needs_full_state};

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;
