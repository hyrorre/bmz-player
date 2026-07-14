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
    AppConfig, AudioBackend, AudioBufferSizeMode, AudioSampleRateMode, DifficultyTableSource,
    GamepadBackendKind, InputBackendKind, LogLevel, ObsActionConfig, ObsRecordingMode, PathEntry,
    RendererBackend, UpdateChannelConfig, VsyncModeConfig, WindowMode,
};
use crate::config::play::{TARGET_GREEN_NUMBER_MAX, TARGET_GREEN_NUMBER_MIN};
use crate::config::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    DoubleOptionConfig, FastSlowDisplayScope, GaugeAutoShiftConfig, GaugeTypeConfig,
    HISPEED_STEP_MAX, HISPEED_STEP_MIN, HispeedModeConfig, HsFixConfig, IrConfig,
    IrCredentialStoreConfig, IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig,
    JudgeAlgorithmConfig, LaneEffectConfig, ProfileConfig, RandomOptionConfig, ReplaySlotRule,
    ScratchInputMode, SkinConfig, SkinHistoryEntryConfig, SkinOffsetConfig, TargetOptionConfig,
    default_hispeed_step_fhs, default_hispeed_step_nhs, normalize_hispeed_step,
};
use crate::ln_policy::LnPolicySetting;
use crate::paths::{AppPaths, resolve_app_paths};
use crate::practice_ui::{PracticePanelContext, build_practice_panel};
use crate::profile_cmd;
use crate::screens::course_session::CourseResultSummary;
use crate::screens::select_model::SelectCourseRow;
use crate::skin_loader::RANDOM_FILE_SELECTION;
use crate::songs_cmd::add_song_root_entry;
use crate::storage::score_import::{ScoreImportKind, ScoreImportRequest};
use crate::update::{UpdateAssetKind, UpdateCandidate, current_version};

const BUNDLED_THIRD_PARTY_NOTICES: &str = include_str!("../../../THIRD-PARTY-NOTICES.txt");
const THIRD_PARTY_NOTICE_PATH: &str = "licenses/third-party-notices.txt";
const RUST_DEPENDENCY_LICENSE_PATH: &str = "licenses/rust-dependency-licenses.txt";
const LOCAL_RUST_DEPENDENCY_LICENSE_FILE: &str = "rust-dependency-licenses.txt";

/// スキンが宣言する設定可能項目の定義 (1 シーン分)。
///
/// renderer が保持する `SkinDocument` から複製して egui パネルへ渡す。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinReloadRequest {
    pub select: bool,
    pub decide: bool,
    pub result: bool,
    pub course_result: bool,
    pub play4: bool,
    pub play5: bool,
    pub play6: bool,
    pub play7: bool,
    pub play8: bool,
    pub play9: bool,
    pub play10: bool,
    pub play14: bool,
    pub offsets: bool,
}

impl SkinReloadRequest {
    pub fn any_reload(self) -> bool {
        self.select
            || self.decide
            || self.result
            || self.course_result
            || self.play4
            || self.play5
            || self.play6
            || self.play7
            || self.play8
            || self.play9
            || self.play10
            || self.play14
    }

    pub fn any(self) -> bool {
        self.any_reload() || self.offsets
    }

    pub fn union(&mut self, other: Self) {
        self.select |= other.select;
        self.decide |= other.decide;
        self.result |= other.result;
        self.course_result |= other.course_result;
        self.play4 |= other.play4;
        self.play5 |= other.play5;
        self.play6 |= other.play6;
        self.play7 |= other.play7;
        self.play8 |= other.play8;
        self.play9 |= other.play9;
        self.play10 |= other.play10;
        self.play14 |= other.play14;
        self.offsets |= other.offsets;
    }
}

#[derive(Clone, Default)]
pub struct SceneSkinDefs {
    pub property: Vec<SkinPropertyDef>,
    pub filepath: Vec<SkinFilepathDef>,
    pub offset: Vec<SkinOffsetDef>,
}

impl SceneSkinDefs {
    /// renderer の `SkinDocument` から設定可能項目の定義を複製する。
    pub fn from_document(document: Option<&SkinDocument>) -> Self {
        match document {
            Some(doc) => Self {
                property: doc.property.clone(),
                filepath: doc.filepath.clone(),
                offset: doc.offset.clone(),
            },
            None => Self::default(),
        }
    }

    /// beatoraja はすべてのプレイ用スキンに共通 offset を追加するため、
    /// BMZ のスキン設定 UI でも play skin だけ同じ項目を常時出す。
    pub fn from_play_document(document: Option<&SkinDocument>) -> Self {
        let mut defs = Self::from_document(document);
        defs.append_missing_beatoraja_play_offsets();
        defs
    }

    fn is_empty(&self) -> bool {
        self.property.is_empty() && self.filepath.is_empty() && self.offset.is_empty()
    }

    fn append_missing_beatoraja_play_offsets(&mut self) {
        for offset in beatoraja_play_common_offsets() {
            if let Some(existing) = self.offset.iter_mut().find(|existing| existing.id == offset.id)
            {
                if offset.id == SKIN_OFFSET_BAR_LINE {
                    existing.h = true;
                    existing.a = true;
                }
            } else {
                self.offset.push(offset);
            }
        }
    }
}

fn beatoraja_play_common_offsets() -> [SkinOffsetDef; 5] {
    [
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "All offset(%)".to_string(),
            id: 10,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: false,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Notes offset".to_string(),
            id: 30,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: false,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Judge offset".to_string(),
            id: 32,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: true,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Judge Detail offset".to_string(),
            id: 33,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: true,
        },
        SkinOffsetDef {
            category: "bmz".to_string(),
            name: "Bar Line offset".to_string(),
            id: SKIN_OFFSET_BAR_LINE,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: true,
        },
    ]
}

/// 選曲 / プレイ / リザルト各スキンの設定可能項目。
#[derive(Default)]
pub struct SkinConfigMeta {
    pub select: SceneSkinDefs,
    pub decide: SceneSkinDefs,
    pub play4: SceneSkinDefs,
    pub play5: SceneSkinDefs,
    pub play6: SceneSkinDefs,
    pub play7: SceneSkinDefs,
    pub play8: SceneSkinDefs,
    pub play9: SceneSkinDefs,
    pub play10: SceneSkinDefs,
    pub play14: SceneSkinDefs,
    pub result: SceneSkinDefs,
    pub course_result: SceneSkinDefs,
}

#[derive(Debug, Clone, Default)]
pub struct SkinCatalog {
    pub select: Vec<SkinCandidate>,
    pub decide: Vec<SkinCandidate>,
    pub play4: Vec<SkinCandidate>,
    pub play5: Vec<SkinCandidate>,
    pub play6: Vec<SkinCandidate>,
    pub play7: Vec<SkinCandidate>,
    pub play8: Vec<SkinCandidate>,
    pub play9: Vec<SkinCandidate>,
    pub play10: Vec<SkinCandidate>,
    pub play14: Vec<SkinCandidate>,
    pub result: Vec<SkinCandidate>,
    pub course_result: Vec<SkinCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinCandidate {
    pub name: String,
    pub path: String,
    pub origin: SkinCandidateOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinCandidateOrigin {
    Bundled,
    User,
    External,
}

/// デバッグ表示パネルへ毎フレーム渡すアプリ側の情報。
pub struct DebugInfo {
    /// 現在のシーン種別 ("Select" / "Play" / "Result")。
    pub scene: &'static str,
    /// 描画サーフェスの幅 (px)。
    pub width: u32,
    /// 描画サーフェスの高さ (px)。
    pub height: u32,
}

/// `EguiLayer::run` の 1 フレーム入力。
pub struct EguiRunContext<'a, 'practice> {
    pub info: &'a DebugInfo,
    pub app_config: &'a mut AppConfig,
    pub profile_config: &'a mut ProfileConfig,
    pub skin_meta: &'a SkinConfigMeta,
    pub skin_catalog: &'a SkinCatalog,
    pub course_result: Option<&'a CourseResultSummary>,
    pub course_preview: Option<&'a SelectCourseRow>,
    pub practice: Option<&'a mut PracticePanelContext<'practice>>,
    pub result_ir: Option<&'a mut crate::screens::result_ir::ResultIrState>,
    pub profile_root: &'a Path,
    pub app_paths: &'a AppPaths,
    pub update_dialog: Option<UpdateDialog<'a>>,
    pub obs_connection_status: &'a crate::obs::ObsConnectionStatus,
    /// 接続中ゲームパッド一覧 (gilrs)。未初期化時は空。
    pub connected_gamepads: &'a [crate::input::gamepad::ConnectedGamepad],
}

/// `EguiLayer::run` の 1 フレーム出力。
pub struct EguiOutput {
    /// renderer へ渡す描画データ。
    pub frame: EguiFrame,
    /// OBS WebSocket の有効/無効変更を実行中のコントローラへ即時反映する要求。
    pub obs_enabled_changed: bool,
    /// 本体設定 (`AppConfig`) の保存が要求されたか。
    pub save_app_config: bool,
    /// プロファイル設定 (`ProfileConfig`) の保存が要求されたか。
    pub save_profile_config: bool,
    /// profile.toml からスキン設定を再読込して未保存変更を戻す要求。
    pub reset_skin_config: bool,
    /// スキン設定値のうち、再読込や即時反映が必要な対象。
    pub skin_reload_request: SkinReloadRequest,
    /// 有効な曲ルートをライブラリ DB へ再スキャンする要求。
    pub trigger_song_rescan: bool,
    /// 曲フォルダのスキャン要求。
    pub song_scan_requests: Vec<SongScanRequest>,
    /// 難易度表の取得要求。空なら取得しない。
    pub table_fetch_urls: Vec<String>,
    pub score_import_request: Option<ScoreImportRequest>,
    /// 現在の設定で音声出力(cpal ストリーム)を開き直す要求。
    pub apply_audio_output: bool,
    pub check_for_update: bool,
    pub update_dialog_action: Option<UpdateDialogAction>,
    pub practice_start: bool,
    pub practice_leave: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum UpdateDialog<'a> {
    Available(&'a UpdateCandidate),
    Downloading(&'a UpdateCandidate),
    Error { message: &'a str, candidate: Option<&'a UpdateCandidate> },
    UpToDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDialogAction {
    Update,
    NotNow,
    SkipRelease,
    OpenReleasePage,
}

#[derive(Clone, Debug)]
pub struct SongScanRequest {
    pub roots: Vec<PathEntry>,
    pub force: bool,
    pub label: String,
}

/// egui の状態管理とフレーム構築を担うレイヤ。
pub struct EguiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    /// メニュー全体の表示状態。F1 でトグルする。
    visible: bool,
    /// デバッグ表示パネルの開閉状態。
    show_debug: bool,
    /// 右上 FPS オーバーレイの表示状態。
    show_fps: bool,
    /// 本体設定パネルの開閉状態。
    show_settings: bool,
    /// プロファイル設定パネルの開閉状態。
    show_profile_settings: bool,
    /// スキン設定パネルの開閉状態。
    show_skin: bool,
    /// ライセンス / third-party notice 表示パネルの開閉状態。
    show_license_notice: bool,
    /// ライセンス表示パネルに出す結合済み notice text。
    license_notice_text: Option<String>,
    update_dialog_active: bool,
    /// 本体設定パネル: 曲フォルダ追加用の入力欄。
    settings_new_root_path: String,
    /// 本体設定パネル: 曲フォルダ追加の直近エラー。
    settings_add_root_error: String,
    settings_new_table_url: String,
    settings_add_table_error: String,
    score_import_path: String,
    score_import_kind: ScoreImportKind,
    score_import_device_type: InputDeviceKind,
    score_import_status: String,
    score_import_error: String,
    /// 本体設定パネル: 出力デバイス選択用の列挙キャッシュ。
    audio_device_picker: AudioDevicePickerState,
    /// 本体設定パネル: OBS scene list 取得状態。
    obs_scene_picker: ObsScenePickerState,
    /// プロファイル設定パネル: IR ログインフォームの状態。
    ir_login: IrLoginUiState,
    /// プロファイル設定パネル: IR device key 操作用の状態。
    ir_device_key: IrDeviceKeyUiState,
    /// プロファイル設定パネル: profile 作成 / 複製フォームの状態。
    profile_manager: ProfileManagerUiState,
    /// BMZ メニュー: OS のファイルマネージャでディレクトリを開いた直近結果。
    directory_open_status: Option<DirectoryOpenStatus>,
}

#[derive(Debug, Clone)]
struct DirectoryOpenStatus {
    label: &'static str,
    path: PathBuf,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryOpenTarget<'a> {
    label: &'static str,
    path: &'a Path,
}

/// プロファイル設定パネルの IR ログインフォーム状態。
///
/// ログインはネットワーク I/O なので tokio タスクで実行し、
/// 結果は channel 経由で次フレーム以降に反映する。
#[derive(Default)]
struct IrLoginUiState {
    email: String,
    password: String,
    busy: bool,
    busy_target: Option<IrProviderUiTarget>,
    message: Option<IrProviderUiMessage>,
    receiver: Option<std::sync::mpsc::Receiver<Result<IrLoginOutcome, String>>>,
}

#[derive(Default)]
struct ProfileManagerUiState {
    create_id: String,
    create_display_name: String,
    create_activate: bool,
    copy_source_id: String,
    copy_target_id: String,
    copy_display_name: String,
    copy_activate: bool,
    message: String,
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IrProviderUiTarget {
    provider: String,
    base_url: String,
}

impl IrProviderUiTarget {
    fn new(provider: String, base_url: String) -> Self {
        Self { provider, base_url }
    }

    fn matches(&self, provider: &str, base_url: &str) -> bool {
        self.provider == provider && self.base_url == base_url
    }
}

#[derive(Debug, Clone)]
struct IrProviderUiMessage {
    target: IrProviderUiTarget,
    ok: bool,
    text: String,
}

/// ログインタスクから UI スレッドへ返す結果。
struct IrLoginOutcome {
    provider: String,
    provider_key: String,
    base_url: String,
    account_id: String,
    display_name: String,
}

/// プロファイル設定パネルの IR device key 操作状態。
#[derive(Default)]
struct IrDeviceKeyUiState {
    busy_provider: Option<String>,
    busy_target: Option<IrProviderUiTarget>,
    message: Option<IrProviderUiMessage>,
    receiver: Option<std::sync::mpsc::Receiver<Result<IrDeviceKeyOutcome, String>>>,
}

struct IrDeviceKeyOutcome {
    provider: String,
    base_url: String,
    public_key: String,
    key_id: String,
}

impl IrDeviceKeyUiState {
    fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        self.receiver = None;
        let target = self.busy_target.take();
        self.busy_provider = None;
        self.message = match result {
            Ok(outcome) => Some(IrProviderUiMessage {
                target: IrProviderUiTarget::new(outcome.provider.clone(), outcome.base_url),
                ok: true,
                text: format!(
                    "{} の署名鍵を再生成しました: {} ({})",
                    outcome.provider,
                    short_public_key(&outcome.public_key),
                    outcome.key_id
                ),
            }),
            Err(error) => {
                target.map(|target| IrProviderUiMessage { target, ok: false, text: error })
            }
        };
    }

    fn start_rotate(
        &mut self,
        profile_root: std::path::PathBuf,
        provider: String,
        provider_key: String,
        base_url: String,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        self.busy_provider = Some(provider_key.clone());
        self.busy_target = Some(IrProviderUiTarget::new(provider.clone(), base_url.clone()));
        self.message = None;
        tokio::spawn(async move {
            let outcome = async {
                let credentials = crate::ir::sync::ensure_fresh_credentials(
                    &profile_root,
                    &provider_key,
                    &base_url,
                    now_unix_seconds(),
                )
                .await?;
                let client = crate::ir::bmz_official::BmzOfficialIrClient::new(
                    &base_url,
                    credentials.access_token,
                )?;
                let key = crate::ir::device_key::rotate_registered_device_key(
                    &profile_root,
                    &provider_key,
                    &client,
                )
                .await?;
                anyhow::Ok(IrDeviceKeyOutcome {
                    provider,
                    base_url,
                    public_key: key.public_key,
                    key_id: key.key_id.unwrap_or_default(),
                })
            }
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(outcome);
        });
    }
}

impl IrLoginUiState {
    /// ログインタスクの完了を取り込み、成功時は provider 設定を更新する。
    /// profile 設定が更新された (保存が必要な) 場合に true を返す。
    fn poll(&mut self, profile: &mut ProfileConfig) -> bool {
        let Some(receiver) = &self.receiver else {
            return false;
        };
        let Ok(result) = receiver.try_recv() else {
            return false;
        };
        self.receiver = None;
        self.busy = false;
        let target = self.busy_target.take();
        match result {
            Ok(outcome) => {
                self.password.clear();
                self.message = Some(IrProviderUiMessage {
                    target: IrProviderUiTarget::new(
                        outcome.provider.clone(),
                        outcome.base_url.clone(),
                    ),
                    ok: true,
                    text: format!("{} としてログインしました", outcome.display_name),
                });
                if let Some(entry) = profile.ir.providers.iter_mut().find(|entry| {
                    entry.provider == outcome.provider && entry.base_url == outcome.base_url
                }) {
                    entry.enabled = true;
                    entry.provider_key = outcome.provider_key.clone();
                    entry.account_id = outcome.account_id;
                    entry.account_display_name = outcome.display_name;
                    entry.last_login_at = Some(now_unix_seconds());
                    if profile.ir.primary_provider.is_empty() {
                        profile.ir.primary_provider = outcome.provider_key;
                        entry.role = IrProviderRoleConfig::Primary;
                    }
                    sync_ir_provider_roles(&mut profile.ir);
                    return true;
                }
                false
            }
            Err(error) => {
                self.message =
                    target.map(|target| IrProviderUiMessage { target, ok: false, text: error });
                false
            }
        }
    }

    /// ログインタスクを起動する。
    fn start_login(
        &mut self,
        profile_root: std::path::PathBuf,
        provider: String,
        base_url: String,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.busy_target = Some(IrProviderUiTarget::new(provider.clone(), base_url.clone()));
        self.message = None;
        let email = self.email.clone();
        let password = self.password.clone();
        tokio::spawn(async move {
            let outcome = async {
                let client = crate::ir::bmz_official::BmzOfficialIrClient::anonymous(&base_url)?;
                let tokens = client.login(&email, &password).await?;
                let provider_key = tokens.provider_key.clone();
                let display_name =
                    tokens.player.display_name.clone().unwrap_or_else(|| email.clone());
                crate::ir::credentials::save_credentials(
                    &profile_root,
                    &crate::ir::credentials::IrStoredCredentials {
                        provider: provider_key.clone(),
                        account_id: tokens.player.id.clone(),
                        display_name: display_name.clone(),
                        access_token: tokens.access_token,
                        refresh_token: tokens.refresh_token,
                        expires_at: tokens.expires_at,
                    },
                )?;
                anyhow::Ok(IrLoginOutcome {
                    provider,
                    provider_key,
                    base_url,
                    account_id: tokens.player.id,
                    display_name,
                })
            }
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(outcome);
        });
    }
}

fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn short_public_key(public_key: &str) -> String {
    if public_key.len() <= 16 {
        return public_key.to_string();
    }
    format!("{}…{}", &public_key[..8], &public_key[public_key.len() - 8..])
}

/// 設定パネルの出力デバイス選択 ComboBox 用キャッシュ。
#[derive(Default)]
struct AudioDevicePickerState {
    /// 列挙済み出力デバイス名(ASIO ならドライバ名)。
    names: Vec<String>,
    /// `names` を列挙したときのバックエンド。変化したら再列挙する。
    backend: Option<AudioBackend>,
}

impl EguiLayer {
    /// `show_fps` は右上 FPS オーバーレイの初期表示状態。
    pub fn new(window: &Window, show_fps: bool) -> Self {
        let ctx = egui::Context::default();
        install_japanese_font(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            ctx,
            state,
            visible: false,
            show_debug: false,
            show_fps,
            show_settings: false,
            show_profile_settings: false,
            show_skin: false,
            show_license_notice: false,
            license_notice_text: None,
            update_dialog_active: false,
            settings_new_root_path: String::new(),
            settings_add_root_error: String::new(),
            settings_new_table_url: String::new(),
            settings_add_table_error: String::new(),
            score_import_path: String::new(),
            score_import_kind: ScoreImportKind::default(),
            score_import_device_type: InputDeviceKind::Keyboard,
            score_import_status: String::new(),
            score_import_error: String::new(),
            audio_device_picker: AudioDevicePickerState::default(),
            obs_scene_picker: ObsScenePickerState::default(),
            ir_login: IrLoginUiState::default(),
            ir_device_key: IrDeviceKeyUiState::default(),
            profile_manager: ProfileManagerUiState::default(),
            directory_open_status: None,
        }
    }

    /// メニュー表示状態を反転する (F1)。
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        tracing::info!(visible = self.visible, "egui menu toggled");
    }

    /// 選曲画面の「詳細設定」から egui メニューと本体設定パネルを開く。
    pub fn open_advanced_settings(&mut self) {
        self.visible = true;
        self.show_settings = true;
        tracing::info!("egui advanced settings opened from select");
    }

    pub fn set_score_import_status(&mut self, status: String, error: bool) {
        if error {
            self.score_import_error = status;
            self.score_import_status.clear();
        } else {
            self.score_import_status = status;
            self.score_import_error.clear();
        }
    }

    /// winit イベントを egui へ供給する。
    ///
    /// 戻り値が true のとき、その入力は egui が消費したのでゲーム側へ伝播させない。
    /// メニュー非表示中は egui に状態は渡すが消費とは扱わず、ゲーム操作を妨げない。
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
        practice_overlay: bool,
    ) -> bool {
        let response = self.state.on_window_event(window, event);
        self.blocks_game_input(practice_overlay) && response.consumed
    }

    pub fn blocks_game_input(&self, practice_overlay: bool) -> bool {
        self.visible || practice_overlay || self.update_dialog_active
    }

    /// 1 フレーム分の UI を構築し、描画データと要求されたアクションを返す。
    pub fn run(&mut self, window: &Window, context: EguiRunContext<'_, '_>) -> EguiOutput {
        let EguiRunContext {
            info,
            app_config,
            profile_config,
            skin_meta,
            skin_catalog,
            course_result,
            course_preview,
            mut practice,
            mut result_ir,
            profile_root,
            app_paths,
            update_dialog,
            obs_connection_status,
            connected_gamepads,
        } = context;
        let raw_input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        let show_debug = &mut self.show_debug;
        let show_settings = &mut self.show_settings;
        let show_profile_settings = &mut self.show_profile_settings;
        let show_skin = &mut self.show_skin;
        let show_fps = &mut self.show_fps;
        let show_license_notice = &mut self.show_license_notice;
        let license_notice_text = &mut self.license_notice_text;
        let mut obs_enabled_changed = false;
        let mut save_app_config = false;
        let mut save_profile_config = false;
        let mut reset_skin_config = false;
        let mut skin_reload_request = SkinReloadRequest::default();
        let mut trigger_song_rescan = false;
        let mut song_scan_requests = Vec::new();
        let mut table_fetch_urls = Vec::new();
        let mut score_import_request = None;
        let mut apply_audio_output = false;
        let mut check_for_update = false;
        let mut update_dialog_action = None;
        let mut practice_start = false;
        let mut practice_leave = false;
        let settings_editable = !scene_restricts_settings(info.scene);
        let mut readonly_app_config = (!settings_editable).then(|| app_config.clone());
        let visible_flag = &mut self.visible;
        let ir_login = &mut self.ir_login;
        let directory_open_status = &mut self.directory_open_status;
        let update_dialog_allowed =
            update_dialog.is_some() && (info.scene == "Select" || *show_settings);
        self.update_dialog_active = update_dialog_allowed;
        let full_output = ctx.run_ui(raw_input, |ui| {
            if update_dialog_allowed && let Some(dialog) = update_dialog {
                update_dialog_action = build_update_dialog(ui.ctx(), dialog);
            }
            if let Some(practice_ctx) = practice.as_mut() {
                let panel = build_practice_panel(ui.ctx(), practice_ctx);
                practice_start |= panel.start_play;
                practice_leave |= panel.leave;
            }
            if *visible_flag {
                let ctx = ui.ctx();
                let result_ir_visible = result_ir.is_some();
                // IR ランキングも egui 補助ウィンドウなので、他の egui
                // ウィンドウと同じ F1 メニュー表示中だけ出す。
                if let Some(state) = result_ir.as_mut() {
                    build_result_ir_panel(ctx, state);
                }
                // Course info panels are developer/debug egui overlays, so keep
                // them behind the same F1 menu visibility gate as the other
                // egui windows.
                if let Some(summary) = course_result {
                    build_course_result_panel(ctx, summary, result_ir_visible);
                }
                if let Some(preview) = course_preview {
                    build_course_preview_panel(ctx, preview);
                }
                build_menu(
                    ctx,
                    visible_flag,
                    show_debug,
                    show_settings,
                    show_profile_settings,
                    show_skin,
                    show_license_notice,
                    app_paths,
                    directory_open_status,
                );
                build_third_party_notice_panel(
                    ctx,
                    show_license_notice,
                    app_paths,
                    license_notice_text,
                );
                build_debug_panel(ctx, show_debug, info);
                let settings_actions = build_settings_panel(
                    ctx,
                    show_settings,
                    if settings_editable {
                        app_config
                    } else {
                        readonly_app_config.as_mut().expect("read-only config must exist")
                    },
                    profile_config,
                    show_fps,
                    settings_editable,
                    SettingsPanelState {
                        new_root_path: &mut self.settings_new_root_path,
                        add_root_error: &mut self.settings_add_root_error,
                        new_table_url: &mut self.settings_new_table_url,
                        add_table_error: &mut self.settings_add_table_error,
                        score_import_path: &mut self.score_import_path,
                        score_import_kind: &mut self.score_import_kind,
                        score_import_device_type: &mut self.score_import_device_type,
                        score_import_status: &self.score_import_status,
                        score_import_error: &self.score_import_error,
                        audio_device_picker: &mut self.audio_device_picker,
                        obs_scene_picker: &mut self.obs_scene_picker,
                        obs_connection_status,
                        connected_gamepads,
                    },
                );
                obs_enabled_changed |= settings_actions.obs_enabled_changed;
                save_app_config |= settings_actions.save;
                save_profile_config |= settings_actions.save_profile;
                check_for_update |= settings_actions.check_update;
                trigger_song_rescan |= settings_actions.rescan;
                song_scan_requests.extend(settings_actions.song_scan_requests);
                table_fetch_urls.extend(settings_actions.table_fetch_urls);
                apply_audio_output |= settings_actions.apply_audio;
                score_import_request = settings_actions.score_import_request;
                let profile_settings_actions = build_profile_settings_panel(
                    ctx,
                    show_profile_settings,
                    profile_config,
                    app_config,
                    show_fps,
                    ir_login,
                    &mut self.ir_device_key,
                    &mut self.profile_manager,
                    profile_root,
                    settings_editable,
                );
                save_profile_config |= profile_settings_actions.save;
                save_app_config |= profile_settings_actions.save_app_config;
                let skin_actions = build_skin_panel(
                    ctx,
                    show_skin,
                    &mut profile_config.skin,
                    skin_meta,
                    skin_catalog,
                    app_paths,
                );
                save_profile_config |= skin_actions.save;
                reset_skin_config |= skin_actions.reset;
                skin_reload_request.union(skin_actions.reload);
            }
        });
        self.state.handle_platform_output(window, full_output.platform_output);
        let primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        EguiOutput {
            frame: EguiFrame {
                primitives,
                textures_delta: full_output.textures_delta,
                pixels_per_point: full_output.pixels_per_point,
            },
            obs_enabled_changed,
            save_app_config,
            save_profile_config,
            reset_skin_config,
            skin_reload_request,
            trigger_song_rescan,
            song_scan_requests,
            table_fetch_urls,
            score_import_request,
            apply_audio_output,
            check_for_update,
            update_dialog_action,
            practice_start,
            practice_leave,
        }
    }
}

/// egui のデフォルトフォントは日本語グリフを含まないため、OS の CJK 対応
/// フォントを各フォントファミリの末尾フォールバックとして登録する。
fn install_japanese_font(ctx: &egui::Context) {
    let Some(bytes) = bmz_render::renderer::load_japanese_font_bytes() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let font_data = egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.26,
        y_offset: 0.0,
        ..Default::default()
    });
    fonts.font_data.insert("bmz_jp".to_owned(), std::sync::Arc::new(font_data));
    // Latin は egui 既定フォントのまま、欠落グリフ (日本語) だけここへフォールバックさせる。
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(chain) = fonts.families.get_mut(&family) {
            chain.push("bmz_jp".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// 各サブパネルの開閉を切り替えるメインメニューハブ。
fn build_menu(
    ctx: &egui::Context,
    visible: &mut bool,
    show_debug: &mut bool,
    show_settings: &mut bool,
    show_profile_settings: &mut bool,
    show_skin: &mut bool,
    show_license_notice: &mut bool,
    app_paths: &AppPaths,
    directory_open_status: &mut Option<DirectoryOpenStatus>,
) {
    egui::Window::new("BMZ メニュー")
        .open(visible)
        .constrain_to(ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN))
        .default_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            ui.label("F1 でこのメニューを開閉します。");
            ui.separator();
            ui.checkbox(show_debug, "デバッグ表示");
            ui.checkbox(show_settings, "本体設定");
            ui.checkbox(show_profile_settings, "プロファイル設定");
            ui.checkbox(show_skin, "スキン設定");
            ui.checkbox(show_license_notice, "ライセンス表記");
            ui.separator();
            ui.label("ディレクトリを開く");
            ui.horizontal_wrapped(|ui| {
                for target in directory_open_targets(app_paths) {
                    if ui
                        .button(target.label)
                        .on_hover_text(target.path.display().to_string())
                        .clicked()
                    {
                        *directory_open_status = Some(open_directory_target(target));
                    }
                }
            });
            if let Some(status) = directory_open_status.as_ref() {
                match status.error.as_deref() {
                    Some(error) => {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("{} を開けません: {error}", status.label),
                        )
                        .on_hover_text(status.path.display().to_string());
                    }
                    None => {
                        ui.small(format!("{} を開きました", status.label))
                            .on_hover_text(status.path.display().to_string());
                    }
                }
            }
        });
}

fn build_third_party_notice_panel(
    ctx: &egui::Context,
    open: &mut bool,
    app_paths: &AppPaths,
    notice_text: &mut Option<String>,
) {
    if !*open {
        return;
    }
    let notice = notice_text.get_or_insert_with(|| combined_license_notice_text(app_paths));
    let mut notice = notice.as_str();
    sized_panel_window("ライセンス表記", ctx, open, 620.0, 560.0, egui::pos2(936.0, 320.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut notice)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });
        },
    );
}

fn combined_license_notice_text(app_paths: &AppPaths) -> String {
    combined_license_notice_text_with_repo_root(app_paths, &repo_root())
}

fn combined_license_notice_text_with_repo_root(app_paths: &AppPaths, repo_root: &Path) -> String {
    let third_party = third_party_notice_text(app_paths);
    let rust_dependencies = rust_dependency_license_text(app_paths, repo_root);

    format!(
        "{third_party}\n\n\n================================================================\nGenerated Rust Dependency License Report\n================================================================\n\n{rust_dependencies}"
    )
}

fn third_party_notice_text(app_paths: &AppPaths) -> String {
    let packaged = app_paths.resource_dir.join(THIRD_PARTY_NOTICE_PATH);
    read_non_empty_text(&packaged).unwrap_or_else(|| BUNDLED_THIRD_PARTY_NOTICES.to_string())
}

fn rust_dependency_license_text(app_paths: &AppPaths, repo_root: &Path) -> String {
    let packaged = app_paths.resource_dir.join(RUST_DEPENDENCY_LICENSE_PATH);
    if let Some(text) = read_non_empty_text(&packaged) {
        return text;
    }

    let local = repo_root.join(LOCAL_RUST_DEPENDENCY_LICENSE_FILE);
    if let Some(text) = read_non_empty_text(&local) {
        return text;
    }

    missing_rust_dependency_license_text(&packaged, &local)
}

fn read_non_empty_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().filter(|text| !text.trim().is_empty())
}

fn missing_rust_dependency_license_text(packaged: &Path, local: &Path) -> String {
    format!(
        "BMZ Player Rust Dependency Licenses\n===================================\n\nThe generated Rust dependency license report was not found.\n\nExpected packaged path:\n  {}\n\nLocal development fallback:\n  {}\n\nGenerate it from the repository root with:\n\n  cargo-about generate --workspace --locked --fail \\\n    --output-file rust-dependency-licenses.txt \\\n    about.hbs\n",
        packaged.display(),
        local.display()
    )
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn directory_open_targets(app_paths: &AppPaths) -> [DirectoryOpenTarget<'_>; 4] {
    [
        DirectoryOpenTarget { label: "resource_dir", path: &app_paths.resource_dir },
        DirectoryOpenTarget { label: "data_dir", path: &app_paths.data_dir },
        DirectoryOpenTarget { label: "cache_dir", path: &app_paths.cache_dir },
        DirectoryOpenTarget { label: "logs_dir", path: &app_paths.logs_dir },
    ]
}

fn open_directory_target(target: DirectoryOpenTarget<'_>) -> DirectoryOpenStatus {
    let error = open_directory(target.path).err();
    DirectoryOpenStatus { label: target.label, path: target.path.to_path_buf(), error }
}

fn open_directory(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("ディレクトリが存在しません: {}", path.display()));
    }
    spawn_directory_opener(path).map_err(|error| format!("{} ({})", error, path.display()))
}

#[cfg(target_os = "macos")]
fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    run_directory_opener("open", path)
}

#[cfg(target_os = "windows")]
fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    // explorer.exe may hand the request to the existing shell process and
    // return a non-zero status even though the directory was opened.
    Command::new("explorer").arg(path).spawn().map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_directory_opener(path: &Path) -> std::io::Result<()> {
    run_directory_opener("xdg-open", path)
}

#[cfg(unix)]
fn run_directory_opener(program: &str, path: &Path) -> std::io::Result<()> {
    let status = Command::new(program).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("{program} exited with {status}")))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
fn spawn_directory_opener(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "opening directories is not supported on this platform",
    ))
}

/// Window 内コンテンツを全体スクロール可能にする。
fn scrollable_window_content<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    // レイアウト確定前に inner が膨らむのを防ぐため、
    // 利用可能矩形から ScrollArea 高さを明示的に制限する。
    let available = ui.available_rect_before_wrap();
    let max_height = available.height().max(64.0);
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(available.width());
            add_contents(ui)
        })
        .inner
}

/// パネル Window の default / max サイズと初期位置をビューポート内に収める。
const PANEL_VIEWPORT_MARGIN: f32 = 16.0;

/// Window の outer サイズ = inner + chrome。egui `Window` の resize margin 計算に合わせる。
fn panel_window_chrome(ctx: &egui::Context) -> egui::Vec2 {
    let style = ctx.global_style();
    let frame = egui::Frame::window(&style);
    let title_bar_inner_height = ctx
        .fonts_mut(|fonts| fonts.row_height(&style.text_styles[&egui::TextStyle::Heading]))
        .at_least(style.spacing.interact_size.y)
        + frame.inner_margin.sum().y;
    let title_content_spacing = frame.stroke.width;
    let frame_margin = frame.total_margin().sum();
    egui::vec2(frame_margin.x, frame_margin.y + title_bar_inner_height + title_content_spacing)
}

fn clamp_panel_layout(
    constrain: egui::Rect,
    chrome: egui::Vec2,
    preferred_width: f32,
    preferred_height: f32,
    preferred_pos: egui::Pos2,
) -> (egui::Vec2, egui::Vec2, egui::Pos2) {
    let max_inner = egui::vec2(
        (constrain.width() - chrome.x).max(200.0),
        (constrain.height() - chrome.y).max(80.0),
    );
    let default_inner =
        egui::vec2(preferred_width.min(max_inner.x), preferred_height.min(max_inner.y));
    let outer = default_inner + chrome;
    let max_x = (constrain.max.x - outer.x).max(constrain.min.x);
    let max_y = (constrain.max.y - outer.y).max(constrain.min.y);
    let default_pos = egui::pos2(
        preferred_pos.x.clamp(constrain.min.x, max_x),
        preferred_pos.y.clamp(constrain.min.y, max_y),
    );
    (default_inner, max_inner, default_pos)
}

/// 既存 Window の outer rect が constrain からはみ出していれば位置を補正する。
fn panel_window_pos(
    ctx: &egui::Context,
    title: &'static str,
    constrain: egui::Rect,
    default_pos: egui::Pos2,
) -> egui::Pos2 {
    let id = egui::Id::new(title);
    let Some(rect) = ctx.memory(|memory| memory.area_rect(id)) else {
        return default_pos;
    };
    constrain_window_rect_to_area(rect, constrain).min
}

/// egui `Context::constrain_window_rect_to_area` と同等 (crate 外からは非公開のため)。
fn constrain_window_rect_to_area(window: egui::Rect, area: egui::Rect) -> egui::Rect {
    let mut pos = window.min;
    let margin_x = (window.width() - area.width()).at_least(0.0);
    let margin_y = (window.height() - area.height()).at_least(0.0);
    pos.x = pos.x.at_most(area.right() + margin_x - window.width());
    pos.x = pos.x.at_least(area.left() - margin_x);
    pos.y = pos.y.at_most(area.bottom() + margin_y - window.height());
    pos.y = pos.y.at_least(area.top() - margin_y);
    egui::Rect::from_min_size(pos, window.size())
}

fn sized_panel_window<'open>(
    title: &'static str,
    ctx: &egui::Context,
    open: &'open mut bool,
    preferred_width: f32,
    preferred_height: f32,
    default_pos: egui::Pos2,
) -> egui::Window<'open> {
    let constrain = ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN);
    let chrome = panel_window_chrome(ctx);
    let (default_inner, max_inner, clamped_default_pos) =
        clamp_panel_layout(constrain, chrome, preferred_width, preferred_height, default_pos);
    let pos = panel_window_pos(ctx, title, constrain, clamped_default_pos);
    egui::Window::new(title)
        .open(open)
        .resizable(true)
        .constrain_to(constrain)
        .current_pos(pos)
        .default_size(default_inner)
        .max_size(max_inner)
        .min_size([280.0, 80.0])
}

/// コース全体リザルトを画面上にオーバーレイ表示する。
///
/// `finished_course` が `Some` のあいだ表示され続け、リザルト画面を抜けると
/// `None` になって自動的に消える。最小実装として egui::Window を 1 枚出すだけ。
/// リザルト画面の IR 送信状況とランキングを表示するオーバーレイ。
fn build_result_ir_panel(
    ctx: &egui::Context,
    state: &mut crate::screens::result_ir::ResultIrState,
) {
    use crate::screens::result_ir::{IrSubmitState, RankingLoadState, ResultRankingTab};

    let content_rect = ctx.content_rect();
    let panel_width = 360.0_f32;
    let pos = egui::pos2(content_rect.right() - panel_width - 16.0, 16.0);

    egui::Window::new("IR ランキング")
        .id(egui::Id::new("result_ir_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .current_pos(pos)
        .default_width(panel_width)
        .show(ctx, |ui| {
            match &state.submit {
                IrSubmitState::Sending => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("スコア送信中...");
                    });
                }
                IrSubmitState::Done { submitted, failed, message } => {
                    if *failed > 0 {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("送信失敗 {failed} 件 (成功 {submitted} 件)"),
                        );
                        if let Some(message) = message {
                            ui.small(message.clone());
                        }
                    } else if *submitted > 0 {
                        ui.colored_label(
                            egui::Color32::LIGHT_GREEN,
                            format!("スコア送信済み ({submitted} 件)"),
                        );
                    } else {
                        ui.label("送信対象なし");
                    }
                }
            }

            ui.separator();
            let mut selected_tab = None;
            ui.horizontal(|ui| {
                let global = state.active_tab == ResultRankingTab::Global;
                let rivals = state.active_tab == ResultRankingTab::SelfAndRivals;
                if ui.selectable_label(global, "全体").clicked() && !global {
                    selected_tab = Some(ResultRankingTab::Global);
                }
                if state.supports_tab(ResultRankingTab::SelfAndRivals)
                    && ui.selectable_label(rivals, "ライバル").clicked()
                    && !rivals
                {
                    selected_tab = Some(ResultRankingTab::SelfAndRivals);
                }
            });
            if let Some(tab) = selected_tab {
                state.select_tab(tab);
            }
            // タブ未選択のまま NotRequested の場合 (prefetch OFF) も取得を開始する。
            if matches!(state.active_state(), RankingLoadState::NotRequested) {
                state.select_tab(state.active_tab);
            }

            match state.active_state() {
                RankingLoadState::NotRequested | RankingLoadState::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("ランキング取得中...");
                    });
                }
                RankingLoadState::Failed(error) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, "ランキング取得失敗");
                    ui.small(error.clone());
                }
                RankingLoadState::Loaded(ranking) => {
                    if ranking.entries.is_empty() {
                        ui.label("この条件のスコアはまだありません");
                    } else {
                        egui::Grid::new("result_ir_ranking_grid")
                            .num_columns(5)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("#");
                                ui.strong("プレイヤー");
                                ui.strong("EX");
                                ui.strong("クリア");
                                ui.strong("BP");
                                ui.end_row();
                                for entry in &ranking.entries {
                                    ui.monospace(entry.rank.to_string());
                                    ui.label(&entry.player_name);
                                    ui.monospace(entry.ex_score.to_string());
                                    ui.label(&entry.clear);
                                    ui.monospace(entry.bp.to_string());
                                    ui.end_row();
                                }
                            });
                        if let Some(rank) = ranking.self_rank {
                            ui.separator();
                            ui.label(format!("自分の順位: {} 位", rank));
                        }
                    }
                }
            }
        });
}

fn build_course_result_panel(
    ctx: &egui::Context,
    summary: &CourseResultSummary,
    result_ir_visible: bool,
) {
    let content_rect = ctx.content_rect();
    // Panel widened from 360px to 440px so the 6-column per-chart grid
    // (#/title/EX/combo/clear/miss) fits without horizontal scroll.
    let panel_width = 440.0_f32;
    let right_margin = if result_ir_visible { 360.0 + 32.0 } else { 16.0 };
    let pos_x = (content_rect.right() - panel_width - right_margin).max(content_rect.left() + 16.0);
    let pos = egui::pos2(pos_x, 16.0);

    egui::Window::new("コースリザルト")
        .id(egui::Id::new("course_result_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .title_bar(true)
        .current_pos(pos)
        .default_width(panel_width)
        .show(ctx, |ui| {
            ui.heading(&summary.title);

            ui.horizontal(|ui| {
                let kind_label = match summary.kind {
                    bmz_core::course::CourseKind::Dan => "段位",
                    bmz_core::course::CourseKind::Course => "コース",
                };
                ui.label(kind_label);
                ui.separator();
                if summary.course_failed {
                    ui.colored_label(egui::Color32::LIGHT_RED, "FAILED");
                } else if summary.course_clear {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "CLEAR");
                } else {
                    ui.colored_label(egui::Color32::LIGHT_YELLOW, "NO TROPHY");
                }
                ui.separator();
                ui.label(format!("{}/{}", summary.played_entries, summary.total_entries));
            });

            ui.separator();

            // Totals.
            let score_rate = if summary.max_ex_score > 0 {
                summary.total_ex_score as f32 / summary.max_ex_score as f32 * 100.0
            } else {
                0.0
            };
            egui::Grid::new("course_result_totals").num_columns(2).show(ui, |ui| {
                ui.label("EX SCORE");
                ui.label(format!(
                    "{} / {} ({:.2}%)",
                    summary.total_ex_score, summary.max_ex_score, score_rate
                ));
                ui.end_row();
                ui.label("NOTES");
                ui.label(format!("{}", summary.total_notes));
                ui.end_row();
                ui.label("PG / GR");
                ui.label(format!(
                    "{} / {}",
                    summary.judge_counts.pgreat, summary.judge_counts.great
                ));
                ui.end_row();
                ui.label("GD / BD / PR");
                ui.label(format!(
                    "{} / {} / {}",
                    summary.judge_counts.good, summary.judge_counts.bad, summary.judge_counts.poor,
                ));
                ui.end_row();
            });

            if !summary.trophy_results.is_empty() {
                ui.separator();
                ui.label("トロフィー");
                // `trophy_results` is built only from `definition.trophies`
                // in `ActiveCourseSession::into_result`, so it cannot show
                // a name that the course author did not declare.
                ui.horizontal_wrapped(|ui| {
                    for trophy in &summary.trophy_results {
                        let color = if trophy.achieved {
                            egui::Color32::from_rgb(255, 215, 0) // gold
                        } else {
                            egui::Color32::DARK_GRAY
                        };
                        ui.colored_label(color, &trophy.name);
                    }
                });
            }

            // BEST section: shows the highest persisted attempt for this
            // course.  Includes the current attempt if it improved the
            // record (the lookup runs after insert_course_score).
            if let Some(best) = &summary.best_score {
                ui.separator();
                ui.label("ベスト");
                let best_rate = if best.max_ex_score > 0 {
                    best.ex_score as f32 / best.max_ex_score as f32 * 100.0
                } else {
                    0.0
                };
                let is_new_record = best.ex_score == summary.total_ex_score
                    && best.max_ex_score == summary.max_ex_score
                    && !summary.course_failed;
                egui::Grid::new("course_result_best").num_columns(2).show(ui, |ui| {
                    ui.label("EX SCORE");
                    let ex_text =
                        format!("{} / {} ({:.2}%)", best.ex_score, best.max_ex_score, best_rate);
                    if is_new_record {
                        ui.colored_label(egui::Color32::from_rgb(255, 215, 0), ex_text);
                    } else {
                        ui.label(ex_text);
                    }
                    ui.end_row();
                    ui.label("CLEAR");
                    ui.label(&best.clear_type);
                    ui.end_row();
                    ui.label("MAX COMBO");
                    ui.label(format!("{}", best.max_combo));
                    ui.end_row();
                });
                if is_new_record {
                    ui.colored_label(egui::Color32::from_rgb(255, 215, 0), "★ NEW RECORD");
                }
            }

            if !summary.entry_summaries.is_empty() {
                ui.separator();
                ui.label("各曲");
                egui::Grid::new("course_result_entries").num_columns(6).striped(true).show(
                    ui,
                    |ui| {
                        // Header row.
                        ui.label("#");
                        ui.label("曲名");
                        ui.label("EX");
                        ui.label("COMBO");
                        ui.label("CLEAR");
                        ui.label("BP");
                        ui.end_row();
                        for (i, entry) in summary.entry_summaries.iter().enumerate() {
                            ui.label(format!("{}", i + 1));
                            let title =
                                if entry.title.is_empty() { "(no title)" } else { &entry.title };
                            ui.label(title);
                            ui.label(format!("{}", entry.ex_score));
                            ui.label(format!("{}", entry.max_combo));
                            // Color the clear cell so failed entries stand out.
                            let clear_text = entry.clear_type.as_str();
                            let clear_color = match entry.clear_type {
                                bmz_core::clear::ClearType::Failed => egui::Color32::LIGHT_RED,
                                bmz_core::clear::ClearType::FullCombo
                                | bmz_core::clear::ClearType::Perfect
                                | bmz_core::clear::ClearType::Max => egui::Color32::LIGHT_GREEN,
                                _ => ui.visuals().text_color(),
                            };
                            ui.colored_label(clear_color, clear_text);
                            let miss = entry.judge_counts.bad
                                + entry.judge_counts.poor
                                + entry.judge_counts.empty_poor;
                            ui.label(format!("{}", miss));
                            ui.end_row();
                        }
                    },
                );
            }
        });
}

/// 選曲画面でコース行にカーソルがある間、コース内の各曲のメタ情報を表示する
/// プレビューパネル。
fn build_course_preview_panel(ctx: &egui::Context, preview: &SelectCourseRow) {
    let content_rect = ctx.content_rect();
    let pos = egui::pos2(16.0, content_rect.bottom() - 320.0);

    egui::Window::new("コース内訳")
        .id(egui::Id::new("course_preview_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .title_bar(true)
        .current_pos(pos)
        .default_width(380.0)
        .max_height(300.0)
        .show(ctx, |ui| {
            ui.heading(&preview.title);
            ui.horizontal(|ui| {
                ui.label(&preview.category_label);
                ui.separator();
                ui.label(format!("{}/{} resolved", preview.resolved_count, preview.entry_count));
                ui.separator();
                ui.label(format!("notes {}", preview.total_notes));
            });
            if !preview.trophy_names.is_empty() {
                ui.label(format!("trophies: {}", preview.trophy_names.join(" / ")));
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("course_preview_entries").num_columns(4).striped(true).show(
                    ui,
                    |ui| {
                        ui.label("#");
                        ui.label("曲名");
                        ui.label("☆");
                        ui.label("notes");
                        ui.end_row();
                        for (i, entry) in preview.entry_previews.iter().enumerate() {
                            ui.label(format!("{}", i + 1));
                            let title =
                                if entry.title.is_empty() { "(no title)" } else { &entry.title };
                            if entry.resolved {
                                ui.label(title);
                            } else {
                                ui.colored_label(
                                    egui::Color32::GRAY,
                                    format!("{} (missing)", title),
                                );
                            }
                            ui.label(&entry.play_level);
                            ui.label(format!("{}", entry.total_notes));
                            ui.end_row();
                        }
                    },
                );
            });
        });
}

/// FPS / フレーム時間 / シーン / 解像度を表示するデバッグパネル。
fn build_debug_panel(ctx: &egui::Context, open: &mut bool, info: &DebugInfo) {
    sized_panel_window("デバッグ表示", ctx, open, 320.0, 200.0, egui::pos2(16.0, 140.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
                let dt = ctx.input(|i| i.stable_dt);
                let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                egui::Grid::new("debug_grid").num_columns(2).show(ui, |ui| {
                    ui.label("FPS");
                    ui.label(format!("{fps:.1}"));
                    ui.end_row();
                    ui.label("フレーム時間");
                    ui.label(format!("{:.2} ms", dt * 1000.0));
                    ui.end_row();
                    ui.label("シーン");
                    ui.label(info.scene);
                    ui.end_row();
                    ui.label("解像度");
                    ui.label(format!("{} x {}", info.width, info.height));
                    ui.end_row();
                });
            });
        },
    );
}

fn build_update_dialog(
    ctx: &egui::Context,
    dialog: UpdateDialog<'_>,
) -> Option<UpdateDialogAction> {
    let mut action = None;
    egui::Window::new("アップデート")
        .id(egui::Id::new("update_dialog"))
        .collapsible(false)
        .resizable(false)
        .default_width(440.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| match dialog {
            UpdateDialog::Available(candidate) => {
                ui.heading(format!("BMZ Player {} が利用できます", candidate.version));
                ui.label(format!("現在のバージョン: {}", current_version()));
                if let Some(published_at) = candidate.published_at.as_deref() {
                    ui.label(format!("公開日: {published_at}"));
                }
                if let Some(asset) = candidate.asset.as_ref() {
                    ui.label(format!("更新ファイル: {}", asset.name));
                    ui.label(update_asset_kind_label(asset.kind));
                } else {
                    ui.label("この環境向けの自動更新ファイルはまだありません。");
                }
                if let Some(body) = release_body_excerpt(&candidate.body) {
                    ui.separator();
                    ui.label(body);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let can_update = candidate.asset.is_some();
                    if ui.add_enabled(can_update, egui::Button::new("アップデート")).clicked()
                    {
                        action = Some(UpdateDialogAction::Update);
                    }
                    if ui.button("今回はアップデートしない").clicked() {
                        action = Some(UpdateDialogAction::NotNow);
                    }
                    if ui.button("このリリースをスキップ").clicked() {
                        action = Some(UpdateDialogAction::SkipRelease);
                    }
                });
                if ui.button("リリースページを開く").clicked() {
                    action = Some(UpdateDialogAction::OpenReleasePage);
                }
            }
            UpdateDialog::Downloading(candidate) => {
                ui.heading(format!("BMZ Player {} をダウンロード中", candidate.version));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("更新ファイルを取得しています。");
                });
                if let Some(asset) = candidate.asset.as_ref() {
                    ui.label(format!("更新ファイル: {}", asset.name));
                }
            }
            UpdateDialog::Error { message, candidate } => {
                ui.heading("アップデート確認に失敗しました");
                ui.colored_label(egui::Color32::LIGHT_RED, message);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("閉じる").clicked() {
                        action = Some(UpdateDialogAction::NotNow);
                    }
                    if candidate.is_some() && ui.button("リリースページを開く").clicked()
                    {
                        action = Some(UpdateDialogAction::OpenReleasePage);
                    }
                });
            }
            UpdateDialog::UpToDate => {
                ui.heading("BMZ Player は最新です");
                ui.label(format!("現在のバージョン: {}", current_version()));
                if ui.button("閉じる").clicked() {
                    action = Some(UpdateDialogAction::NotNow);
                }
            }
        });
    action
}

fn release_body_excerpt(body: &str) -> Option<String> {
    let mut lines =
        body.lines().map(str::trim).filter(|line| !line.is_empty()).take(6).collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let mut text = lines.join("\n");
    const MAX_LEN: usize = 480;
    if text.len() > MAX_LEN {
        text = text.chars().take(MAX_LEN).collect();
        text.push_str("...");
    } else if body.lines().filter(|line| !line.trim().is_empty()).count() > lines.len() {
        text.push_str("\n...");
    }
    lines.clear();
    Some(text)
}

fn update_asset_kind_label(kind: UpdateAssetKind) -> &'static str {
    match kind {
        UpdateAssetKind::WindowsInstaller => "インストーラーを起動して更新します。",
        UpdateAssetKind::MacosAppZip => "macOS 版はリリースページから手動で更新します。",
        UpdateAssetKind::Other => "リリースページから手動で更新します。",
    }
}

/// 本体設定パネルからのアクション要求。
struct SettingsPanelActions {
    save: bool,
    obs_enabled_changed: bool,
    save_profile: bool,
    check_update: bool,
    rescan: bool,
    song_scan_requests: Vec<SongScanRequest>,
    table_fetch_urls: Vec<String>,
    score_import_request: Option<ScoreImportRequest>,
    /// 音声出力(cpal ストリーム)を現在の設定で開き直す要求。
    apply_audio: bool,
}

struct SettingsPanelState<'a> {
    new_root_path: &'a mut String,
    add_root_error: &'a mut String,
    new_table_url: &'a mut String,
    add_table_error: &'a mut String,
    score_import_path: &'a mut String,
    score_import_kind: &'a mut ScoreImportKind,
    score_import_device_type: &'a mut InputDeviceKind,
    score_import_status: &'a str,
    score_import_error: &'a str,
    audio_device_picker: &'a mut AudioDevicePickerState,
    obs_scene_picker: &'a mut ObsScenePickerState,
    obs_connection_status: &'a crate::obs::ObsConnectionStatus,
    connected_gamepads: &'a [crate::input::gamepad::ConnectedGamepad],
}

#[derive(Default)]
struct ObsScenePickerState {
    busy: bool,
    scenes: Vec<String>,
    message: String,
    error: String,
    receiver: Option<std::sync::mpsc::Receiver<Result<crate::obs::ObsSceneList, String>>>,
}

impl ObsScenePickerState {
    fn poll(&mut self) {
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
                self.message = format!(
                    "{} 件のシーンを取得しました ({}, 録画: {})",
                    self.scenes.len(),
                    list.version,
                    if list.recording_active { "ON" } else { "OFF" }
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
enum SettingsListAction {
    MoveUp(usize),
    MoveDown(usize),
    MoveTo { from: usize, to: usize },
    Remove(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsDragList {
    SongRoots,
    TableSources,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SettingsDragPayload {
    list: SettingsDragList,
    index: usize,
}

const SETTINGS_LIST_BUTTONS_WIDTH: f32 = 224.0;
const SETTINGS_TABLE_LIST_BUTTONS_WIDTH: f32 = 224.0;
const SETTINGS_TABLE_ENABLED_WIDTH: f32 = 56.0;
const SETTINGS_LIST_DRAG_HANDLE_WIDTH: f32 = 28.0;
const SETTINGS_LIST_MIN_LABEL_WIDTH: f32 = 96.0;

fn apply_settings_list_action<T>(items: &mut Vec<T>, action: SettingsListAction) {
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

fn settings_list_label_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - SETTINGS_LIST_BUTTONS_WIDTH).max(SETTINGS_LIST_MIN_LABEL_WIDTH)
}

fn settings_list_label(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized([width, ui.spacing().interact_size.y], egui::Label::new(text).truncate())
        .on_hover_text(text);
}

fn settings_drag_handle(ui: &mut egui::Ui, payload: SettingsDragPayload) {
    let response = ui.add_sized(
        [SETTINGS_LIST_DRAG_HANDLE_WIDTH, ui.spacing().interact_size.y],
        egui::Button::new(egui::RichText::new("≡").size(18.0)).sense(egui::Sense::drag()),
    );
    response.dnd_set_drag_payload(payload);
    response.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("ドラッグして並び替え");
}

fn settings_drag_ghost(
    ctx: &egui::Context,
    id: egui::Id,
    text: &str,
    label_width: f32,
    show_song_options: bool,
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
                    settings_list_label(ui, text, label_width);
                });
                if show_song_options {
                    let mut enabled = true;
                    let mut recursive = true;
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Checkbox::new(&mut enabled, "有効"));
                        ui.add_enabled(false, egui::Checkbox::new(&mut recursive, "再帰スキャン"));
                    });
                }
            });
        });
}

/// `AppConfig` を編集する本体設定パネル。
fn build_settings_panel(
    ctx: &egui::Context,
    open: &mut bool,
    config: &mut AppConfig,
    profile: &mut ProfileConfig,
    show_fps: &mut bool,
    editable: bool,
    state: SettingsPanelState<'_>,
) -> SettingsPanelActions {
    let mut save_clicked = false;
    let mut obs_enabled_changed = false;
    let mut save_profile = false;
    let mut rescan_clicked = false;
    let mut check_update_clicked = false;
    let mut song_scan_requests = Vec::new();
    let mut table_fetch_urls = Vec::new();
    let mut score_import_request = None;
    let mut apply_audio = false;
    sized_panel_window("本体設定", ctx, open, 440.0, 520.0, egui::pos2(16.0, 320.0)).show(
        ctx,
        |ui| {
            if !editable {
                ui.label("Decide / Play 中は本体設定を変更できません。");
                ui.separator();
            }
            ui.add_enabled_ui(editable, |ui| {
                scrollable_window_content(ui, |ui| {
                egui::CollapsingHeader::new("曲フォルダ (BMS)")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut root_action = None;
                        let root_len = config.songs.roots.len();
                        for (index, root) in config.songs.roots.iter_mut().enumerate() {
                            ui.push_id(index, |ui| {
                                let label_width = (settings_list_label_width(ui)
                                    - SETTINGS_LIST_DRAG_HANDLE_WIDTH)
                                    .max(SETTINGS_LIST_MIN_LABEL_WIDTH);
                                let (_, dropped) = ui.dnd_drop_zone::<SettingsDragPayload, _>(
                                    egui::Frame::NONE,
                                    |ui| {
                                        let payload = SettingsDragPayload {
                                            list: SettingsDragList::SongRoots,
                                            index,
                                        };
                                        ui.horizontal(|ui| {
                                            settings_drag_handle(ui, payload);
                                            settings_list_label(ui, &root.path, label_width);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui.button("削除").clicked() {
                                                        root_action =
                                                            Some(SettingsListAction::Remove(index));
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            root.enabled,
                                                            egui::Button::new("再読込"),
                                                        )
                                                        .clicked()
                                                    {
                                                        song_scan_requests.push(SongScanRequest {
                                                            roots: vec![root.clone()],
                                                            force: true,
                                                            label: "egui song reload".to_string(),
                                                        });
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            index + 1 < root_len,
                                                            egui::Button::new("下へ"),
                                                        )
                                                        .clicked()
                                                    {
                                                        root_action = Some(
                                                            SettingsListAction::MoveDown(index),
                                                        );
                                                    }
                                                    if ui
                                                        .add_enabled(
                                                            index > 0,
                                                            egui::Button::new("上へ"),
                                                        )
                                                        .clicked()
                                                    {
                                                        root_action =
                                                            Some(SettingsListAction::MoveUp(index));
                                                    }
                                                },
                                            );
                                        });
                                        ui.horizontal(|ui| {
                                            ui.checkbox(&mut root.enabled, "有効");
                                            ui.checkbox(&mut root.recursive, "再帰スキャン");
                                        });
                                    },
                                );
                                if egui::DragAndDrop::payload::<SettingsDragPayload>(ui.ctx())
                                    .is_some_and(|payload| {
                                        payload.list == SettingsDragList::SongRoots
                                            && payload.index == index
                                    })
                                {
                                    settings_drag_ghost(
                                        ui.ctx(),
                                        egui::Id::new(("settings_song_root_ghost", index)),
                                        &root.path,
                                        label_width,
                                        true,
                                    );
                                }
                                if let Some(payload) = dropped
                                    && payload.list == SettingsDragList::SongRoots
                                {
                                    root_action = Some(SettingsListAction::MoveTo {
                                        from: payload.index,
                                        to: index,
                                    });
                                }
                                ui.separator();
                            });
                        }
                        if let Some(action) = root_action {
                            apply_settings_list_action(&mut config.songs.roots, action);
                        }
                        if config.songs.roots.is_empty() {
                            ui.label("登録された曲フォルダはありません。");
                        }
                        ui.horizontal(|ui| {
                            ui.label("パス");
                            ui.add(
                                egui::TextEdit::singleline(state.new_root_path)
                                    .desired_width(240.0)
                                    .hint_text("/path/to/bms"),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.button("フォルダを選択…").clicked()
                                && let Some(folder) = rfd::FileDialog::new().pick_folder()
                            {
                                *state.new_root_path = folder.to_string_lossy().into_owned();
                                state.add_root_error.clear();
                            }
                            if ui.button("追加").clicked() {
                                let path = state.new_root_path.trim().to_string();
                                if path.is_empty() {
                                    *state.add_root_error =
                                        "パスを入力するかフォルダを選択してください。".to_string();
                                } else {
                                    match add_song_root_entry(
                                        &mut config.songs.roots,
                                        &path,
                                        true,
                                        true,
                                    ) {
                                        Ok(()) => {
                                            song_scan_requests.push(SongScanRequest {
                                                roots: vec![PathEntry {
                                                    path,
                                                    enabled: true,
                                                    recursive: true,
                                                }],
                                                force: false,
                                                label: "egui song load".to_string(),
                                            });
                                            save_clicked = true;
                                            state.new_root_path.clear();
                                            state.add_root_error.clear();
                                        }
                                        Err(error) => *state.add_root_error = error.to_string(),
                                    }
                                }
                            }
                        });
                        if !state.add_root_error.is_empty() {
                            ui.colored_label(egui::Color32::RED, state.add_root_error.as_str());
                        }
                        if ui.button("ライブラリを再スキャン").clicked() {
                            rescan_clicked = true;
                        }
                        ui.label(
                            "追加したフォルダは自動でスキャンします。再スキャンは有効なルート全体を対象にします。",
                        );
                    });

                egui::CollapsingHeader::new("スキャン").show(ui, |ui| {
                    ui.checkbox(&mut config.scan.follow_symlinks, "シンボリックリンクを辿る");
                    ui.checkbox(&mut config.scan.skip_hidden, "隠しファイル / フォルダをスキップ");
                    ui.checkbox(
                        &mut config.scan.auto_rescan_on_startup,
                        "起動時に自動再スキャン",
                    );
                    ui.checkbox(
                        &mut config.scan.rescan_missing_files,
                        "存在しないファイルを DB から除去 (未実装)",
                    );
                });

                egui::CollapsingHeader::new("選曲").show(ui, |ui| {
                    ui.add(
                        egui::Slider::new(
                            &mut config.select.scroll_duration_low_ms,
                            2..=1000,
                        )
                        .text("スクロール初回 (ms)"),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut config.select.scroll_duration_high_ms,
                            1..=1000,
                        )
                        .text("スクロール連続 (ms)"),
                    );
                    ui.label("選曲バー移動とキー長押しリピートに即時反映されます。");
                });

                egui::CollapsingHeader::new("難易度表").show(ui, |ui| {
                    ui.checkbox(&mut config.tables.auto_fetch_on_startup, "起動時に自動取得");
                    let mut table_action = None;
                    let table_len = config.tables.sources.len();
                    for (index, source) in config.tables.sources.iter_mut().enumerate() {
                        ui.push_id(("table_source", index), |ui| {
                            let label_width = (ui.available_width()
                                - SETTINGS_TABLE_LIST_BUTTONS_WIDTH
                                - SETTINGS_TABLE_ENABLED_WIDTH
                                - SETTINGS_LIST_DRAG_HANDLE_WIDTH)
                                .max(64.0);
                            let (_, dropped) = ui.dnd_drop_zone::<SettingsDragPayload, _>(
                                egui::Frame::NONE,
                                |ui| {
                                    let payload = SettingsDragPayload {
                                        list: SettingsDragList::TableSources,
                                        index,
                                    };
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [
                                                SETTINGS_TABLE_ENABLED_WIDTH,
                                                ui.spacing().interact_size.y,
                                            ],
                                            egui::Checkbox::new(&mut source.enabled, "有効"),
                                        );
                                        settings_drag_handle(ui, payload);
                                        settings_list_label(ui, &source.url, label_width);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button("削除").clicked() {
                                                    table_action =
                                                        Some(SettingsListAction::Remove(index));
                                                }
                                                if ui.button("取得").clicked() {
                                                    table_fetch_urls.push(source.url.clone());
                                                }
                                                if ui
                                                    .add_enabled(
                                                        index + 1 < table_len,
                                                        egui::Button::new("下へ"),
                                                    )
                                                    .clicked()
                                                {
                                                    table_action =
                                                        Some(SettingsListAction::MoveDown(index));
                                                }
                                                if ui
                                                    .add_enabled(
                                                        index > 0,
                                                        egui::Button::new("上へ"),
                                                    )
                                                    .clicked()
                                                {
                                                    table_action =
                                                        Some(SettingsListAction::MoveUp(index));
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                            if egui::DragAndDrop::payload::<SettingsDragPayload>(ui.ctx())
                                .is_some_and(|payload| {
                                    payload.list == SettingsDragList::TableSources
                                        && payload.index == index
                                })
                            {
                                settings_drag_ghost(
                                    ui.ctx(),
                                    egui::Id::new(("settings_table_source_ghost", index)),
                                    &source.url,
                                    label_width,
                                    false,
                                );
                            }
                            if let Some(payload) = dropped
                                && payload.list == SettingsDragList::TableSources
                            {
                                table_action = Some(SettingsListAction::MoveTo {
                                    from: payload.index,
                                    to: index,
                                });
                            }
                        });
                    }
                    if let Some(action) = table_action {
                        apply_settings_list_action(&mut config.tables.sources, action);
                    }
                    if config.tables.sources.is_empty() {
                        ui.label("登録された難易度表はありません。");
                    }
                    let enabled_table_urls: Vec<String> = config
                        .tables
                        .sources
                        .iter()
                        .filter(|source| source.enabled)
                        .map(|source| source.url.clone())
                        .collect();
                    if ui
                        .add_enabled(
                            !enabled_table_urls.is_empty(),
                            egui::Button::new("有効な表を全件取得"),
                        )
                        .clicked()
                    {
                        table_fetch_urls.extend(enabled_table_urls);
                    }
                    ui.horizontal(|ui| {
                        ui.label("URL");
                        ui.add(
                            egui::TextEdit::singleline(state.new_table_url)
                                .desired_width(300.0)
                                .hint_text("https://.../header.json"),
                        );
                    });
                    if ui.button("追加").clicked() {
                        let url = state.new_table_url.trim().to_string();
                        match add_difficulty_table_source(
                            &mut config.tables.sources,
                            &url,
                        ) {
                            Ok(()) => {
                                table_fetch_urls.push(url);
                                save_clicked = true;
                                state.new_table_url.clear();
                                state.add_table_error.clear();
                            }
                            Err(error) => *state.add_table_error = error,
                        }
                    }
                    if !state.add_table_error.is_empty() {
                        ui.colored_label(egui::Color32::RED, state.add_table_error.as_str());
                    }
                    ui.label("追加した表は自動で取得します。手動取得は各行または全件取得を使います。");
                });

                build_score_import_section(
                    ui,
                    state.score_import_path,
                    state.score_import_kind,
                    state.score_import_device_type,
                    state.score_import_status,
                    state.score_import_error,
                    &mut score_import_request,
                );

                egui::CollapsingHeader::new("音声").show(ui, |ui| {
                    egui::ComboBox::from_label("バックエンド")
                        .selected_text(audio_backend_label(&config.audio.backend))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Auto,
                                "自動選択",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::CoreAudio,
                                "Core Audio",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Wasapi,
                                "WASAPI",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Asio,
                                "ASIO",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Alsa,
                                "ALSA",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Pulse,
                                "PulseAudio",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::PipeWire,
                                "PipeWire",
                            );
                        });
                    let sample_rate_text =
                        if config.audio.sample_rate_mode == AudioSampleRateMode::Auto {
                            "自動 (ドライバ / OS 既定)".to_string()
                        } else {
                            audio_sample_rate_label(config.audio.sample_rate)
                        };
                    egui::ComboBox::from_label("サンプルレート")
                        .selected_text(sample_rate_text)
                        .show_ui(ui, |ui| {
                            let is_auto =
                                config.audio.sample_rate_mode == AudioSampleRateMode::Auto;
                            if ui.selectable_label(is_auto, "自動 (ドライバ / OS 既定)").clicked() {
                                config.audio.sample_rate_mode = AudioSampleRateMode::Auto;
                            }
                            for hz in [44_100u32, 48_000, 96_000, 192_000, 384_000] {
                                let selected = config.audio.sample_rate_mode
                                    == AudioSampleRateMode::Fixed
                                    && config.audio.sample_rate == hz;
                                if ui
                                    .selectable_label(selected, audio_sample_rate_label(hz))
                                    .clicked()
                                {
                                    config.audio.sample_rate_mode = AudioSampleRateMode::Fixed;
                                    config.audio.sample_rate = hz;
                                }
                            }
                        });
                    egui::ComboBox::from_label("バッファサイズモード")
                        .selected_text(audio_buffer_size_mode_label(&config.audio.buffer_size_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.audio.buffer_size_mode,
                                AudioBufferSizeMode::Auto,
                                "自動",
                            );
                            ui.selectable_value(
                                &mut config.audio.buffer_size_mode,
                                AudioBufferSizeMode::Fixed,
                                "固定",
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut config.audio.buffer_size, 32..=4096)
                            .text("バッファサイズ (フレーム)"),
                    );
                    ui.horizontal(|ui| {
                        ui.label("プリセット:");
                        for frames in [32u32, 48, 64, 96, 128, 256] {
                            if ui.button(frames.to_string()).clicked() {
                                config.audio.buffer_size = frames;
                                config.audio.buffer_size_mode = AudioBufferSizeMode::Fixed;
                            }
                        }
                    });
                    ui.checkbox(&mut config.audio.exclusive_mode, "排他モード (未実装)");

                    // ASIO 以外は安価なのでバックエンド変更時に自動列挙する。
                    // ASIO はドライバ初期化を伴い得るため、更新ボタンでのみ列挙する。
                    let backend = config.audio.backend.clone();
                    if backend != AudioBackend::Asio
                        && state.audio_device_picker.backend.as_ref() != Some(&backend)
                    {
                        state.audio_device_picker.names =
                            crate::audio::list_output_devices(&backend);
                        state.audio_device_picker.backend = Some(backend);
                    }

                    ui.horizontal(|ui| {
                        if ui.button("デバイス一覧を更新").clicked() {
                            state.audio_device_picker.names =
                                crate::audio::list_output_devices(&config.audio.backend);
                            state.audio_device_picker.backend = Some(config.audio.backend.clone());
                        }
                        ui.label(format!("{} 件", state.audio_device_picker.names.len()));
                    });

                    if config.audio.backend == AudioBackend::Asio {
                        egui::ComboBox::from_label("ASIO ドライバ")
                            .selected_text(if config.audio.asio_driver.is_empty() {
                                "(未指定)"
                            } else {
                                config.audio.asio_driver.as_str()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut config.audio.asio_driver,
                                    String::new(),
                                    "(未指定)",
                                );
                                for name in state.audio_device_picker.names.iter() {
                                    ui.selectable_value(
                                        &mut config.audio.asio_driver,
                                        name.clone(),
                                        name,
                                    );
                                }
                            });
                    } else {
                        egui::ComboBox::from_label("出力デバイス")
                            .selected_text(if config.audio.output_device.is_empty() {
                                "(デフォルト)"
                            } else {
                                config.audio.output_device.as_str()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut config.audio.output_device,
                                    String::new(),
                                    "(デフォルト)",
                                );
                                for name in state.audio_device_picker.names.iter() {
                                    ui.selectable_value(
                                        &mut config.audio.output_device,
                                        name.clone(),
                                        name,
                                    );
                                }
                            });
                    }
                    egui::ComboBox::from_label("出力チャンネル")
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
                    ui.label(
                        "出力チャンネルは多ch ASIO デバイス向け。デバイスのch数を超える指定は先頭ペアに戻ります。",
                    );
                    ui.label(
                        "ASIO ではドライバ側のバッファ設定が優先される場合があります。",
                    );
                    if ui.button("適用 (音声出力を開き直す)").clicked() {
                        apply_audio = true;
                    }
                    ui.label(
                        "「適用」で現在の設定を保存し音声出力を再構築します(再生中は不可)。排他モードは未実装です。",
                    );
                });

                egui::CollapsingHeader::new("映像").show(ui, |ui| {
                    egui::ComboBox::from_label("ウィンドウモード")
                        .selected_text(window_mode_label(&config.video.mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::Windowed,
                                "ウィンドウ",
                            );
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::BorderlessFullscreen,
                                "ボーダレスフルスクリーン",
                            );
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::ExclusiveFullscreen,
                                "排他フルスクリーン",
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut config.video.width, 640..=3840).text("幅 (px)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut config.video.height, 480..=2160).text("高さ (px)"),
                    );
                    egui::ComboBox::from_label("同期モード")
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
                        egui::Slider::new(&mut config.video.target_fps, 30..=480).text("目標 FPS"),
                    );
                    if ui.checkbox(show_fps, "FPS 表示").changed() {
                        profile.ui.show_fps = *show_fps;
                        save_profile = true;
                    }
                    ui.add(
                        egui::Slider::new(&mut config.video.frame_limit_in_background, 1..=120)
                            .text("バックグラウンド FPS 上限"),
                    );
                    egui::ComboBox::from_label("レンダリングバックエンド")
                        .selected_text(renderer_backend_label(&config.video.renderer))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Auto,
                                "自動選択",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Vulkan,
                                "Vulkan",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Metal,
                                "Metal",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Dx12,
                                "DirectX 12",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Gl,
                                "OpenGL",
                            );
                        });
                    ui.label(
                        "VSync / Present Mode / ウィンドウモード / 目標 FPS は即時反映。幅 / 高さ / レンダリングバックエンドは次回起動時に反映されます。",
                    );
                });

                egui::CollapsingHeader::new("スクリーンショット").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("保存先");
                        ui.add(
                            egui::TextEdit::singleline(&mut config.screenshot.dir)
                                .desired_width(300.0)
                                .hint_text("screenshots"),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("フォルダを選択...").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            config.screenshot.dir = dir.to_string_lossy().into_owned();
                        }
                        ui.checkbox(
                            &mut config.screenshot.copy_to_clipboard,
                            "クリップボードにもコピー",
                        );
                    });
                });

                obs_enabled_changed |= build_obs_settings_section(
                    ui,
                    config,
                    state.obs_scene_picker,
                    state.obs_connection_status,
                );

                egui::CollapsingHeader::new("アップデート").show(ui, |ui| {
                    ui.checkbox(&mut config.updates.enabled, "アップデート通知");
                    ui.checkbox(&mut config.updates.check_on_startup, "起動時に確認");
                    egui::ComboBox::from_label("チャンネル")
                        .selected_text(update_channel_label(config.updates.channel))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.updates.channel,
                                UpdateChannelConfig::Stable,
                                update_channel_label(UpdateChannelConfig::Stable),
                            );
                            ui.selectable_value(
                                &mut config.updates.channel,
                                UpdateChannelConfig::Prerelease,
                                update_channel_label(UpdateChannelConfig::Prerelease),
                            );
                        });
                    if config.updates.skipped_version.is_empty() {
                        ui.label("スキップ中のリリースはありません。");
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "スキップ中: BMZ Player {}",
                                config.updates.skipped_version
                            ));
                            if ui.button("解除").clicked() {
                                config.updates.skipped_version.clear();
                                save_clicked = true;
                            }
                        });
                    }
                    if ui.button("更新を確認").clicked() {
                        check_update_clicked = true;
                    }
                });

                egui::CollapsingHeader::new("Discord").show(ui, |ui| {
                    ui.checkbox(&mut config.discord.enabled, "Rich Presence");
                    ui.horizontal(|ui| {
                        ui.label("Application ID");
                        ui.add(
                            egui::TextEdit::singleline(&mut config.discord.application_id)
                                .desired_width(260.0)
                                .hint_text("空欄なら BMZ 既定"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Large image key");
                        ui.add(
                            egui::TextEdit::singleline(&mut config.discord.large_image_key)
                                .desired_width(160.0)
                                .hint_text("bmz"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Large image text");
                        ui.add(
                            egui::TextEdit::singleline(&mut config.discord.large_image_text)
                                .desired_width(220.0)
                                .hint_text("BMZ Player"),
                        );
                    });
                    ui.checkbox(&mut config.discord.show_song_details, "曲名とアーティストを表示");
                    ui.label("Application ID が空欄なら BMZ の既定 ID を使います。");
                });

                egui::CollapsingHeader::new("入力デバイス").show(ui, |ui| {
                    egui::ComboBox::from_label("バックエンド")
                        .selected_text(input_backend_label(&config.input.backend))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.input.backend,
                                InputBackendKind::Auto,
                                input_backend_label(&InputBackendKind::Auto),
                            );
                            ui.selectable_value(
                                &mut config.input.backend,
                                InputBackendKind::Winit,
                                input_backend_label(&InputBackendKind::Winit),
                            );
                            ui.selectable_value(
                                &mut config.input.backend,
                                InputBackendKind::RawInput,
                                input_backend_label(&InputBackendKind::RawInput),
                            );
                            ui.selectable_value(
                                &mut config.input.backend,
                                InputBackendKind::Hid,
                                input_backend_label(&InputBackendKind::Hid),
                            );
                            ui.selectable_value(
                                &mut config.input.backend,
                                InputBackendKind::Midi,
                                input_backend_label(&InputBackendKind::Midi),
                            );
                        });
                    egui::ComboBox::from_label("ゲームパッドバックエンド")
                        .selected_text(gamepad_backend_label(&config.input.gamepad_backend))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.input.gamepad_backend,
                                GamepadBackendKind::Auto,
                                gamepad_backend_label(&GamepadBackendKind::Auto),
                            );
                            ui.selectable_value(
                                &mut config.input.gamepad_backend,
                                GamepadBackendKind::Gilrs,
                                gamepad_backend_label(&GamepadBackendKind::Gilrs),
                            );
                            ui.selectable_value(
                                &mut config.input.gamepad_backend,
                                GamepadBackendKind::GameInput,
                                gamepad_backend_label(&GamepadBackendKind::GameInput),
                            );
                        });
                    ui.checkbox(&mut config.input.keyboard_enabled, "キーボード");
                    ui.checkbox(&mut config.input.gamepad_enabled, "ゲームパッド");
                    ui.checkbox(&mut config.input.midi_enabled, "MIDI (未実装)");
                    ui.label(
                        "入力バックエンド設定は次回起動時に反映されます。HID / MIDI は未実装です。",
                    );
                    ui.separator();
                    ui.label("10K / 14K 用コントローラ割り当て");
                    ui.label(format!(
                        "接続中: {} 台",
                        state.connected_gamepads.iter().filter(|pad| pad.is_connected).count()
                    ));
                    if state.connected_gamepads.is_empty() {
                        ui.label("ゲームパッドが検出されていません。");
                    } else {
                        for pad in state.connected_gamepads {
                            let status = if pad.is_connected { "接続中" } else { "切断" };
                            ui.label(format!(
                                "#{} {} ({})",
                                pad.backend_id, pad.name, status
                            ));
                        }
                    }
                    for (slot_index, label) in [(0usize, "1P コントローラ"), (1usize, "2P コントローラ")]
                    {
                        let current = config.input.gamepad_slot_device_ids[slot_index].as_deref();
                        let selected_text = match current {
                            Some(stable_id) => state
                                .connected_gamepads
                                .iter()
                                .find(|pad| pad.stable_id == stable_id)
                                .map(|pad| format!("#{} {}", pad.backend_id, pad.name))
                                .unwrap_or_else(|| {
                                    let end = stable_id.len().min(20);
                                    format!("{}... (未接続)", &stable_id[..end])
                                }),
                            None => config.input.gamepad_slot_gilrs_ids[slot_index]
                                .and_then(|id| {
                                    state
                                        .connected_gamepads
                                        .iter()
                                        .find(|pad| pad.backend_id == id)
                                        .map(|pad| {
                                            format!("#{} {} (旧設定)", pad.backend_id, pad.name)
                                        })
                                })
                                .unwrap_or_else(|| "自動 (接続順)".to_string()),
                        };
                        egui::ComboBox::from_label(label)
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(
                                        &mut config.input.gamepad_slot_device_ids[slot_index],
                                        None,
                                        "自動 (接続順)",
                                    )
                                    .clicked()
                                {
                                    config.input.gamepad_slot_gilrs_ids[slot_index] = None;
                                }
                                for pad in state.connected_gamepads {
                                    if ui
                                        .selectable_value(
                                            &mut config.input.gamepad_slot_device_ids[slot_index],
                                            Some(pad.stable_id.clone()),
                                            format!("#{} {}", pad.backend_id, pad.name),
                                        )
                                        .clicked()
                                    {
                                        config.input.gamepad_slot_gilrs_ids[slot_index] = None;
                                    }
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("接続順で自動割り当て").clicked() {
                            let connected: Vec<String> = state
                                .connected_gamepads
                                .iter()
                                .filter(|pad| pad.is_connected)
                                .map(|pad| pad.stable_id.clone())
                                .collect();
                            config.input.gamepad_slot_device_ids[0] = connected.first().cloned();
                            config.input.gamepad_slot_device_ids[1] = connected.get(1).cloned();
                            config.input.gamepad_slot_gilrs_ids = [None, None];
                        }
                        if ui.button("1P / 2P を入れ替え").clicked() {
                            config.input.gamepad_slot_device_ids.swap(0, 1);
                            config.input.gamepad_slot_gilrs_ids.swap(0, 1);
                        }
                        if ui.button("割り当て解除").clicked() {
                            config.input.gamepad_slot_device_ids = [None, None];
                            config.input.gamepad_slot_gilrs_ids = [None, None];
                        }
                    });
                    ui.label(
                        "未割当は接続順フォールバック (1台目=1P, 2台目=2P)。変更は次回プレイ開始から反映されます。",
                    );
                });

                egui::CollapsingHeader::new("ログ (未実装)").show(ui, |ui| {
                    egui::ComboBox::from_label("レベル")
                        .selected_text(log_level_label(&config.logging.level))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.logging.level,
                                LogLevel::Trace,
                                log_level_label(&LogLevel::Trace),
                            );
                            ui.selectable_value(
                                &mut config.logging.level,
                                LogLevel::Debug,
                                log_level_label(&LogLevel::Debug),
                            );
                            ui.selectable_value(
                                &mut config.logging.level,
                                LogLevel::Info,
                                log_level_label(&LogLevel::Info),
                            );
                            ui.selectable_value(
                                &mut config.logging.level,
                                LogLevel::Warn,
                                log_level_label(&LogLevel::Warn),
                            );
                            ui.selectable_value(
                                &mut config.logging.level,
                                LogLevel::Error,
                                log_level_label(&LogLevel::Error),
                            );
                        });
                    ui.checkbox(&mut config.logging.file_logging, "ファイル出力 (未実装)");
                    ui.label("ログ設定は未実装です。現在は起動時の固定ログ設定を使用します。");
                });

                ui.separator();
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
                });
            });
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
        apply_audio,
    }
}

fn build_obs_settings_section(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    state: &mut ObsScenePickerState,
    connection_status: &crate::obs::ObsConnectionStatus,
) -> bool {
    state.poll();
    let mut enabled_changed = false;
    egui::CollapsingHeader::new("OBS WebSocket").show(ui, |ui| {
        enabled_changed = ui.checkbox(&mut config.obs.enabled, "OBS WebSocket 連携").changed();
        let (status_label, status_color) = obs_connection_status_label(connection_status.kind);
        ui.horizontal(|ui| {
            ui.label("接続状態");
            ui.colored_label(status_color, status_label);
            if let Some(retry_in_ms) = connection_status.retry_in_ms {
                ui.label(format!("次の再試行: {:.1} 秒", retry_in_ms as f64 / 1000.0));
            }
        });
        if let Some(detail) = &connection_status.detail {
            ui.label(detail);
        }
        if let Some(error) = &connection_status.last_error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.horizontal(|ui| {
            ui.label("ホスト");
            ui.add(
                egui::TextEdit::singleline(&mut config.obs.host)
                    .desired_width(180.0)
                    .hint_text("localhost"),
            );
            ui.label("ポート");
            ui.add(egui::DragValue::new(&mut config.obs.port).range(0..=65535));
        });
        ui.horizontal(|ui| {
            ui.label("パスワード");
            ui.add(
                egui::TextEdit::singleline(&mut config.obs.password)
                    .desired_width(220.0)
                    .password(true),
            );
        });
        egui::ComboBox::from_label("録画保存モード")
            .selected_text(obs_recording_mode_label(config.obs.recording_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::KeepAll,
                    obs_recording_mode_label(ObsRecordingMode::KeepAll),
                );
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::OnScreenshot,
                    obs_recording_mode_label(ObsRecordingMode::OnScreenshot),
                );
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::OnReplay,
                    obs_recording_mode_label(ObsRecordingMode::OnReplay),
                );
            });
        ui.add(
            egui::Slider::new(&mut config.obs.record_stop_wait_ms, 0..=10_000)
                .text("StopRecord 遅延 (ms)"),
        );

        ui.horizontal(|ui| {
            if ui.add_enabled(!state.busy, egui::Button::new("シーン一覧を取得")).clicked()
            {
                state.start_load(config.obs.clone());
            }
            if state.busy {
                ui.label("取得中...");
            }
        });
        if !state.message.is_empty() {
            ui.label(state.message.as_str());
        }
        if !state.error.is_empty() {
            ui.colored_label(egui::Color32::RED, state.error.as_str());
        }

        ui.separator();
        ui.strong("状態別設定");
        egui::Grid::new("obs_state_mapping_grid").striped(true).show(ui, |ui| {
            ui.label("状態");
            ui.label("シーン");
            ui.label("アクション");
            ui.end_row();
            for event in crate::obs::ObsEventKey::ALL {
                let key = event.config_key();
                ui.label(event.label());

                let mut scene = config.obs.scenes.get(key).cloned().unwrap_or_default();
                let selected_scene = if scene.is_empty() { "(No Change)" } else { scene.as_str() };
                egui::ComboBox::from_id_salt(("obs_scene", key))
                    .selected_text(selected_scene)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut scene, String::new(), "(No Change)");
                        if !scene.is_empty() && !state.scenes.iter().any(|name| name == &scene) {
                            let current_scene = scene.clone();
                            ui.selectable_value(&mut scene, current_scene.clone(), current_scene);
                        }
                        for candidate in &state.scenes {
                            ui.selectable_value(&mut scene, candidate.clone(), candidate);
                        }
                    });
                if scene.is_empty() {
                    config.obs.scenes.remove(key);
                } else {
                    config.obs.scenes.insert(key.to_string(), scene);
                }

                let mut action = config.obs.actions.get(key).copied().unwrap_or_default();
                egui::ComboBox::from_id_salt(("obs_action", key))
                    .selected_text(obs_action_label(action))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::None,
                            obs_action_label(ObsActionConfig::None),
                        );
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::StartRecord,
                            obs_action_label(ObsActionConfig::StartRecord),
                        );
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::StopRecord,
                            obs_action_label(ObsActionConfig::StopRecord),
                        );
                    });
                if action == ObsActionConfig::None {
                    config.obs.actions.remove(key);
                } else {
                    config.obs.actions.insert(key.to_string(), action);
                }
                ui.end_row();
            }
        });
    });
    enabled_changed
}

fn obs_connection_status_label(
    kind: crate::obs::ObsConnectionStatusKind,
) -> (&'static str, egui::Color32) {
    match kind {
        crate::obs::ObsConnectionStatusKind::Disabled => ("無効", egui::Color32::GRAY),
        crate::obs::ObsConnectionStatusKind::Connecting => {
            ("接続中", egui::Color32::from_rgb(120, 190, 255))
        }
        crate::obs::ObsConnectionStatusKind::WaitingForServer => {
            ("OBS を待機中", egui::Color32::from_rgb(225, 185, 75))
        }
        crate::obs::ObsConnectionStatusKind::Connected => ("接続済み", egui::Color32::GREEN),
        crate::obs::ObsConnectionStatusKind::Reconnecting => {
            ("再接続待機中", egui::Color32::YELLOW)
        }
        crate::obs::ObsConnectionStatusKind::AuthenticationFailed => {
            ("認証失敗", egui::Color32::RED)
        }
        crate::obs::ObsConnectionStatusKind::ConfigurationError => {
            ("設定エラー", egui::Color32::RED)
        }
    }
}

fn obs_recording_mode_label(mode: ObsRecordingMode) -> &'static str {
    match mode {
        ObsRecordingMode::KeepAll => "すべて保持",
        ObsRecordingMode::OnScreenshot => "スクリーンショット時に保持",
        ObsRecordingMode::OnReplay => "リプレイ保存時に保持",
    }
}

fn obs_action_label(action: ObsActionConfig) -> &'static str {
    match action {
        ObsActionConfig::None => "(Do Nothing)",
        ObsActionConfig::StartRecord => "Start Recording",
        ObsActionConfig::StopRecord => "Stop Recording",
    }
}

fn build_score_import_section(
    ui: &mut egui::Ui,
    path: &mut String,
    kind: &mut ScoreImportKind,
    device_type: &mut InputDeviceKind,
    status: &str,
    error: &str,
    request: &mut Option<ScoreImportRequest>,
) {
    egui::CollapsingHeader::new("スコアインポート").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("DB");
            ui.add(
                egui::TextEdit::singleline(path)
                    .desired_width(260.0)
                    .hint_text("score.db / scoredatalog.db / LR2 score db"),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("ファイルを選択…").clicked()
                && let Some(file) =
                    rfd::FileDialog::new().add_filter("SQLite DB", &["db"]).pick_file()
            {
                *path = file.to_string_lossy().into_owned();
            }
            egui::ComboBox::from_id_salt("score_import_kind").selected_text(kind.label()).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(kind, ScoreImportKind::Lr2, ScoreImportKind::Lr2.label());
                    ui.selectable_value(
                        kind,
                        ScoreImportKind::Beatoraja,
                        ScoreImportKind::Beatoraja.label(),
                    );
                    ui.selectable_value(
                        kind,
                        ScoreImportKind::Lr2Oraja,
                        ScoreImportKind::Lr2Oraja.label(),
                    );
                    ui.selectable_value(
                        kind,
                        ScoreImportKind::Lr2OrajaDx,
                        ScoreImportKind::Lr2OrajaDx.label(),
                    );
                },
            );
        });
        ui.horizontal(|ui| {
            ui.label("入力デバイス");
            ui.selectable_value(device_type, InputDeviceKind::Keyboard, "キーボード");
            ui.selectable_value(device_type, InputDeviceKind::Controller, "コントローラー");
        });
        if ui.button("インポート").clicked() {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                *request = None;
            } else {
                *request = Some(ScoreImportRequest {
                    path: PathBuf::from(trimmed),
                    kind: *kind,
                    device_type: *device_type,
                });
            }
        }
        if !status.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, status);
        }
        if !error.is_empty() {
            ui.colored_label(egui::Color32::RED, error);
        }
    });
}

fn audio_backend_label(backend: &AudioBackend) -> &'static str {
    match backend {
        AudioBackend::Auto => "自動選択",
        AudioBackend::Wasapi => "WASAPI",
        AudioBackend::Asio => "ASIO",
        AudioBackend::CoreAudio => "Core Audio",
        AudioBackend::Alsa => "ALSA",
        AudioBackend::Pulse => "PulseAudio",
        AudioBackend::PipeWire => "PipeWire",
    }
}

fn audio_buffer_size_mode_label(mode: &AudioBufferSizeMode) -> &'static str {
    match mode {
        AudioBufferSizeMode::Auto => "自動",
        AudioBufferSizeMode::Fixed => "固定",
    }
}

/// 出力チャンネルペア(0 始まり)を "1-2ch" のような表示文字列にする。
fn audio_channel_pair_label(pair: u32) -> String {
    let left = pair * 2 + 1;
    format!("{}-{}ch", left, left + 1)
}

/// サンプルレート(Hz)を "48kHz" / "44.1kHz" のような表示文字列にする。
fn audio_sample_rate_label(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        format!("{}kHz", hz / 1000)
    } else {
        format!("{:.1}kHz", hz as f64 / 1000.0)
    }
}

fn update_channel_label(channel: UpdateChannelConfig) -> &'static str {
    match channel {
        UpdateChannelConfig::Stable => "Stable",
        UpdateChannelConfig::Prerelease => "Prerelease",
    }
}

fn window_mode_label(mode: &WindowMode) -> &'static str {
    match mode {
        WindowMode::Windowed => "ウィンドウ",
        WindowMode::BorderlessFullscreen => "ボーダレスフルスクリーン",
        WindowMode::ExclusiveFullscreen => "排他フルスクリーン",
    }
}

fn renderer_backend_label(backend: &RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "自動選択",
        RendererBackend::Vulkan => "Vulkan",
        RendererBackend::Metal => "Metal",
        RendererBackend::Dx12 => "DirectX 12",
        RendererBackend::Gl => "OpenGL",
    }
}

fn vsync_mode_label(mode: &VsyncModeConfig) -> &'static str {
    match mode {
        VsyncModeConfig::Vsync => "Vsync (Fifo)",
        VsyncModeConfig::AdaptiveVsync => "Adaptive Vsync (Fifo Relaxed)",
        VsyncModeConfig::VsyncOff => "Vsync Off (Immediate)",
        VsyncModeConfig::FastVsync => "Fast Vsync (Mailbox)",
    }
}

fn input_backend_label(backend: &InputBackendKind) -> &'static str {
    match backend {
        InputBackendKind::Auto => "自動選択",
        InputBackendKind::Winit => "winit",
        InputBackendKind::RawInput => "Raw Input (Windowsのみ)",
        InputBackendKind::Hid => "HID (未実装)",
        InputBackendKind::Midi => "MIDI (未実装)",
    }
}

fn gamepad_backend_label(backend: &GamepadBackendKind) -> &'static str {
    match backend {
        GamepadBackendKind::Auto => "自動選択",
        GamepadBackendKind::Gilrs => "gilrs",
        GamepadBackendKind::GameInput => "GameInput (Windowsのみ)",
    }
}

fn log_level_label(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn add_difficulty_table_source(
    sources: &mut Vec<DifficultyTableSource>,
    url: &str,
) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL を入力してください。".to_string());
    }
    if sources.iter().any(|source| source.url == url) {
        return Err("同じ URL の難易度表が既に登録されています。".to_string());
    }
    sources.push(DifficultyTableSource { url: url.to_string(), enabled: true });
    Ok(())
}

struct ProfileSettingsPanelActions {
    save: bool,
    save_app_config: bool,
}

fn scene_restricts_settings(scene: &str) -> bool {
    matches!(scene, "Decide" | "Play")
}

fn restore_restricted_profile_settings(profile: &mut ProfileConfig, mut readonly: ProfileConfig) {
    readonly.audio_mix = profile.audio_mix.clone();
    readonly.judge = profile.judge.clone();
    readonly.lane = profile.lane.clone();
    readonly.input = profile.input.clone();
    *profile = readonly;
}

fn build_profile_settings_panel(
    ctx: &egui::Context,
    open: &mut bool,
    profile: &mut ProfileConfig,
    app_config: &mut AppConfig,
    show_fps: &mut bool,
    ir_login: &mut IrLoginUiState,
    ir_device_key: &mut IrDeviceKeyUiState,
    profile_manager: &mut ProfileManagerUiState,
    profile_root: &std::path::Path,
    unrestricted: bool,
) -> ProfileSettingsPanelActions {
    let mut save_clicked = false;
    let mut save_app_config = false;
    // ログインタスクの完了を反映。provider 設定が更新されたら保存する。
    save_clicked |= ir_login.poll(profile);
    ir_device_key.poll();
    let readonly_profile = (!unrestricted).then(|| profile.clone());
    let readonly_app_config = (!unrestricted).then(|| app_config.clone());
    sized_panel_window("プロファイル設定", ctx, open, 460.0, 560.0, egui::pos2(476.0, 320.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
                if !unrestricted {
                    ui.label("Decide / Play 中は音量・判定・表示・入力のみ変更できます。");
                    ui.separator();
                }
                egui::CollapsingHeader::new("基本").default_open(true).show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        ui.label("表示名");
                        ui.text_edit_singleline(&mut profile.display_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("ID");
                        ui.monospace(&profile.id);
                    });
                });

                save_app_config |= build_profile_manager_section(
                    ui,
                    app_config,
                    profile,
                    profile_manager,
                    unrestricted,
                );

                egui::CollapsingHeader::new("音量").default_open(true).show(ui, |ui| {
                    ui.checkbox(
                        &mut profile.audio_mix.normalize_chart_volume,
                        "譜面・プレビュー音量正規化",
                    );
                    volume_slider(ui, &mut profile.audio_mix.master_volume, "マスター");
                    volume_slider(ui, &mut profile.audio_mix.key_volume, "キー音");
                    volume_slider(ui, &mut profile.audio_mix.bgm_volume, "BGM");
                    volume_slider(ui, &mut profile.audio_mix.preview_volume, "選曲プレビュー");
                    volume_slider(ui, &mut profile.audio_mix.system_bgm_volume, "システム BGM");
                    volume_slider(ui, &mut profile.audio_mix.system_se_volume, "システム SE");
                    ui.label("音量は即時反映されます。");
                });

                egui::CollapsingHeader::new("判定").show(ui, |ui| {
                    offset_ms_slider(ui, &mut profile.judge.input_offset_us, "入力オフセット");
                    offset_ms_slider(ui, &mut profile.judge.visual_offset_us, "表示オフセット");
                    ui.checkbox(&mut profile.judge.visual_offset_auto_adjust, "自動判定調整");
                    egui::ComboBox::from_label("判定アルゴリズム")
                        .selected_text(judge_algorithm_label(profile.judge.judge_algorithm))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.judge.judge_algorithm,
                                JudgeAlgorithmConfig::Combo,
                                judge_algorithm_label(JudgeAlgorithmConfig::Combo),
                            );
                            ui.selectable_value(
                                &mut profile.judge.judge_algorithm,
                                JudgeAlgorithmConfig::Duration,
                                judge_algorithm_label(JudgeAlgorithmConfig::Duration),
                            );
                            ui.selectable_value(
                                &mut profile.judge.judge_algorithm,
                                JudgeAlgorithmConfig::Lowest,
                                judge_algorithm_label(JudgeAlgorithmConfig::Lowest),
                            );
                            ui.selectable_value(
                                &mut profile.judge.judge_algorithm,
                                JudgeAlgorithmConfig::Score,
                                judge_algorithm_label(JudgeAlgorithmConfig::Score),
                            );
                        });
                    egui::ComboBox::from_label("FAST/SLOW 表示モード")
                        .selected_text(fast_slow_scope_label(profile.judge.fast_slow_display_scope))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.judge.fast_slow_display_scope,
                                FastSlowDisplayScope::Auto,
                                fast_slow_scope_label(FastSlowDisplayScope::Auto),
                            );
                            ui.selectable_value(
                                &mut profile.judge.fast_slow_display_scope,
                                FastSlowDisplayScope::ThresholdMs,
                                fast_slow_scope_label(FastSlowDisplayScope::ThresholdMs),
                            );
                        });
                    if profile.judge.fast_slow_display_scope == FastSlowDisplayScope::ThresholdMs {
                        ui.add(
                            egui::Slider::new(
                                &mut profile.judge.fast_slow_display_threshold_ms,
                                0..=50,
                            )
                            .text("FAST/SLOW 表示閾値 (ms)"),
                        );
                        ui.label(
                            "0 = 常時表示。|差分| がこれ未満の判定は FAST/SLOW を表示しません。",
                        );
                    }
                });

                egui::CollapsingHeader::new("プレイ").show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    egui::ComboBox::from_label("ルール")
                        .selected_text(rule_mode_label(profile.play.rule_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.play.rule_mode,
                                RuleMode::Beatoraja,
                                rule_mode_label(RuleMode::Beatoraja),
                            );
                            ui.selectable_value(
                                &mut profile.play.rule_mode,
                                RuleMode::Lr2Oraja,
                                rule_mode_label(RuleMode::Lr2Oraja),
                            );
                            ui.selectable_value(
                                &mut profile.play.rule_mode,
                                RuleMode::Dx,
                                rule_mode_label(RuleMode::Dx),
                            );
                        });
                    egui::ComboBox::from_label("LN モード")
                        .selected_text(profile.play.ln_mode_policy.display_label())
                        .show_ui(ui, |ui| {
                            for value in LnPolicySetting::ORDER {
                                ui.selectable_value(
                                    &mut profile.play.ln_mode_policy,
                                    value,
                                    value.display_label(),
                                );
                            }
                        });
                    egui::ComboBox::from_label("ゲージ")
                        .selected_text(gauge_label(profile.play.gauge))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (GaugeTypeConfig::AssistEasy, "ASSIST EASY"),
                                (GaugeTypeConfig::Easy, "EASY"),
                                (GaugeTypeConfig::Normal, "NORMAL"),
                                (GaugeTypeConfig::Hard, "HARD"),
                                (GaugeTypeConfig::ExHard, "EX HARD"),
                                (GaugeTypeConfig::Hazard, "HAZARD"),
                                (GaugeTypeConfig::AutoShift, "AUTO SHIFT"),
                            ] {
                                ui.selectable_value(&mut profile.play.gauge, value, label);
                            }
                        });
                    egui::ComboBox::from_label("ゲージオートシフト")
                        .selected_text(gauge_auto_shift_label(profile.play.gauge_auto_shift))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (GaugeAutoShiftConfig::Off, "OFF"),
                                (GaugeAutoShiftConfig::Continue, "CONTINUE"),
                                (GaugeAutoShiftConfig::HardToGroove, "HARD->GROOVE"),
                                (GaugeAutoShiftConfig::BestClear, "BEST CLEAR"),
                                (GaugeAutoShiftConfig::SelectToUnder, "SELECT UNDER"),
                            ] {
                                ui.selectable_value(
                                    &mut profile.play.gauge_auto_shift,
                                    value,
                                    label,
                                );
                            }
                        });
                    egui::ComboBox::from_label("GAS 下限ゲージ")
                        .selected_text(bottom_shiftable_gauge_label(
                            profile.play.bottom_shiftable_gauge,
                        ))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (BottomShiftableGaugeConfig::AssistEasy, "ASSIST EASY"),
                                (BottomShiftableGaugeConfig::Easy, "EASY"),
                                (BottomShiftableGaugeConfig::Normal, "NORMAL"),
                            ] {
                                ui.selectable_value(
                                    &mut profile.play.bottom_shiftable_gauge,
                                    value,
                                    label,
                                );
                            }
                        });
                    egui::ComboBox::from_label("ランダム")
                        .selected_text(random_label(profile.play.random))
                        .show_ui(ui, |ui| {
                            for (value, label) in random_options() {
                                ui.selectable_value(&mut profile.play.random, value, label);
                            }
                        });
                    egui::ComboBox::from_label("ランダム 2P")
                        .selected_text(random_label(profile.play.random2))
                        .show_ui(ui, |ui| {
                            for (value, label) in random_options() {
                                ui.selectable_value(&mut profile.play.random2, value, label);
                            }
                        });
                    egui::ComboBox::from_label("DP オプション")
                        .selected_text(double_option_label(profile.play.double_option))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (DoubleOptionConfig::Off, "OFF"),
                                (DoubleOptionConfig::Flip, "FLIP"),
                                (DoubleOptionConfig::Battle, "BATTLE"),
                                (DoubleOptionConfig::BattleAutoScratch, "BATTLE AS"),
                            ] {
                                ui.selectable_value(&mut profile.play.double_option, value, label);
                            }
                        });
                    egui::ComboBox::from_label("HS-FIX")
                        .selected_text(hs_fix_label(profile.play.hs_fix))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (HsFixConfig::Off, "OFF"),
                                (HsFixConfig::StartBpm, "START BPM"),
                                (HsFixConfig::MaxBpm, "MAX BPM"),
                                (HsFixConfig::MainBpm, "MAIN BPM"),
                                (HsFixConfig::MinBpm, "MIN BPM"),
                            ] {
                                ui.selectable_value(&mut profile.play.hs_fix, value, label);
                            }
                        });
                    egui::ComboBox::from_label("ターゲット")
                        .selected_text(target_label(profile.play.target))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (TargetOptionConfig::None, "NONE"),
                                (TargetOptionConfig::RankA, "RANK_A"),
                                (TargetOptionConfig::RankAaMinus, "RANK_AA-"),
                                (TargetOptionConfig::RankAa, "RANK_AA"),
                                (TargetOptionConfig::RankAaaMinus, "RANK_AAA-"),
                                (TargetOptionConfig::RankAaa, "RANK_AAA"),
                                (TargetOptionConfig::RankMaxMinus, "RANK_MAX-"),
                                (TargetOptionConfig::Max, "MAX"),
                                (TargetOptionConfig::RankNext, "RANK_NEXT"),
                                (TargetOptionConfig::IrTop, "IR_TOP"),
                                (TargetOptionConfig::IrNext, "IR_NEXT"),
                                (TargetOptionConfig::RivalTop, "RIVAL TOP"),
                                (TargetOptionConfig::RivalNext, "RIVAL NEXT"),
                            ] {
                                ui.selectable_value(&mut profile.play.target, value, label);
                            }
                        });
                    egui::ComboBox::from_label("リザルト差分表示")
                        .selected_text(grade_diff_display_label(profile.play.grade_diff_display))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.play.grade_diff_display,
                                ResultGradeDiffDisplay::Next,
                                grade_diff_display_label(ResultGradeDiffDisplay::Next),
                            );
                            ui.selectable_value(
                                &mut profile.play.grade_diff_display,
                                ResultGradeDiffDisplay::Nearest,
                                grade_diff_display_label(ResultGradeDiffDisplay::Nearest),
                            );
                        });
                    egui::ComboBox::from_label("レーンエフェクト")
                        .selected_text(lane_effect_label(profile.play.lane_effect))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (LaneEffectConfig::Off, "OFF"),
                                (LaneEffectConfig::Hidden, "HIDDEN"),
                                (LaneEffectConfig::Sudden, "SUDDEN"),
                                (LaneEffectConfig::HiddenSudden, "HIDDEN+SUDDEN"),
                            ] {
                                ui.selectable_value(&mut profile.play.lane_effect, value, label);
                            }
                        });
                    egui::ComboBox::from_label("アシスト")
                        .selected_text(assist_label(profile.play.assist))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (AssistOptionConfig::None, "NONE"),
                                (AssistOptionConfig::AutoScratch, "AUTO SCRATCH"),
                                (AssistOptionConfig::LegacyNote, "LEGACY NOTE"),
                            ] {
                                ui.selectable_value(&mut profile.play.assist, value, label);
                            }
                        });
                    egui::ComboBox::from_label("BGA")
                        .selected_text(bga_mode_label(profile.play.bga))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (BgaModeConfig::On, "ON"),
                                (BgaModeConfig::Auto, "AUTO"),
                                (BgaModeConfig::Off, "OFF"),
                            ] {
                                ui.selectable_value(&mut profile.play.bga, value, label);
                            }
                        });
                    egui::ComboBox::from_label("BGA 表示")
                        .selected_text(bga_expand_label(profile.play.bga_expand))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (BgaExpandConfig::KeepAspect, "KEEP ASPECT"),
                                (BgaExpandConfig::Full, "FULL"),
                                (BgaExpandConfig::Off, "OFF"),
                            ] {
                                ui.selectable_value(&mut profile.play.bga_expand, value, label);
                            }
                        });
                    ui.checkbox(&mut profile.play.auto_play, "オートプレイ");
                    ui.checkbox(&mut profile.play.show_ln_tail_cap, "LN終端キャップを表示");
                    ui.add(
                        egui::Slider::new(&mut profile.play.misslayer_duration_ms, 0..=5000)
                            .text("ミスレイヤー表示時間 (ms)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut profile.play.play_exit_hold_ms, 100..=5000)
                            .text("E1+E2 強制終了長押し時間 (ms)"),
                    );
                });

                egui::CollapsingHeader::new("表示").show(ui, |ui| {
                    let hispeed_step = match profile.lane.hispeed_mode {
                        HispeedModeConfig::Normal => normalize_hispeed_step(
                            profile.lane.hispeed_step_nhs,
                            default_hispeed_step_nhs(),
                        ),
                        HispeedModeConfig::Floating => normalize_hispeed_step(
                            profile.lane.hispeed_step_fhs,
                            default_hispeed_step_fhs(),
                        ),
                    };
                    ui.add(
                        egui::Slider::new(&mut profile.lane.hispeed, 0.5..=10.0)
                            .step_by(hispeed_step as f64)
                            .text("ハイスピード"),
                    );
                    egui::ComboBox::from_label("ハイスピードモード")
                        .selected_text(hispeed_mode_label(profile.lane.hispeed_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.lane.hispeed_mode,
                                HispeedModeConfig::Normal,
                                hispeed_mode_label(HispeedModeConfig::Normal),
                            );
                            ui.selectable_value(
                                &mut profile.lane.hispeed_mode,
                                HispeedModeConfig::Floating,
                                hispeed_mode_label(HispeedModeConfig::Floating),
                            );
                        });
                    ui.add(
                        egui::Slider::new(
                            &mut profile.lane.hispeed_step_nhs,
                            HISPEED_STEP_MIN..=HISPEED_STEP_MAX,
                        )
                        .step_by(0.05)
                        .text("NHS HS変更刻み"),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut profile.lane.hispeed_step_fhs,
                            HISPEED_STEP_MIN..=HISPEED_STEP_MAX,
                        )
                        .step_by(0.05)
                        .text("FHS HS変更刻み"),
                    );
                    ui.label("HS変更刻みの範囲: 0.05..=1.00");
                    let sudden_max =
                        crate::config::play::lane_unit_max_for_other(profile.lane.lift);
                    lane_unit_slider_with_max(ui, &mut profile.lane.sudden, "SUDDEN+", sudden_max);
                    let lift_max =
                        crate::config::play::lane_unit_max_for_other(profile.lane.sudden);
                    lane_unit_slider_with_max(ui, &mut profile.lane.lift, "LIFT", lift_max);
                    lane_unit_slider(ui, &mut profile.lane.hidden, "HIDDEN");
                    ui.add(
                        egui::Slider::new(
                            &mut profile.lane.target_green_number,
                            TARGET_GREEN_NUMBER_MIN..=TARGET_GREEN_NUMBER_MAX,
                        )
                        .text("緑数字ターゲット"),
                    );
                });

                egui::CollapsingHeader::new("入力").show(ui, |ui| {
                    egui::ComboBox::from_label("スクラッチ")
                        .selected_text(scratch_input_mode_label(profile.input.scratch_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.input.scratch_mode,
                                ScratchInputMode::Normal,
                                scratch_input_mode_label(ScratchInputMode::Normal),
                            );
                            ui.selectable_value(
                                &mut profile.input.scratch_mode,
                                ScratchInputMode::AnyDirection,
                                scratch_input_mode_label(ScratchInputMode::AnyDirection),
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut profile.input.analog_scratch_sensitivity, 0.1..=5.0)
                            .text("アナログ感度"),
                    );
                    ui.add(
                        egui::Slider::new(&mut profile.input.analog_scratch_threshold, 1..=1000)
                            .text("アナログ停止閾値"),
                    );
                    ui.label("キー割り当ては選曲画面の設定ツリーで編集できます。");
                });

                egui::CollapsingHeader::new("リプレイ").show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    ui.checkbox(&mut profile.replay.auto_save, "自動保存");
                    ui.checkbox(&mut profile.replay.compress, "圧縮");
                    for (index, rule) in profile.replay.slot_rules.iter_mut().enumerate() {
                        egui::ComboBox::from_label(format!("スロット {}", index + 1))
                            .selected_text(replay_slot_rule_label(*rule))
                            .show_ui(ui, |ui| {
                                for value in [
                                    ReplaySlotRule::Disabled,
                                    ReplaySlotRule::Always,
                                    ReplaySlotRule::ScoreUpdate,
                                    ReplaySlotRule::BpUpdate,
                                    ReplaySlotRule::MaxComboUpdate,
                                    ReplaySlotRule::ClearUpdate,
                                ] {
                                    ui.selectable_value(rule, value, replay_slot_rule_label(value));
                                }
                            });
                    }
                });

                egui::CollapsingHeader::new("システム音").show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    system_sound_path_row(ui, "BGM ルート", &mut profile.system_sound.bgm_dir);
                    system_sound_path_row(ui, "SE ルート", &mut profile.system_sound.se_dir);
                    system_sound_path_row(
                        ui,
                        "フォールバック",
                        &mut profile.system_sound.default_sound_dir,
                    );
                    ui.label("システム音の再スキャンは次回起動時に反映されます。");
                });

                egui::CollapsingHeader::new("IR").show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    sync_ir_provider_roles(&mut profile.ir);
                    let primary_options: Vec<_> = profile
                        .ir
                        .providers
                        .iter()
                        .filter_map(|provider| {
                            crate::ir::provider_key::configured_provider_key(provider).map(
                                |provider_key| {
                                    (
                                        provider_key.to_string(),
                                        ir_primary_provider_label(provider, provider_key),
                                    )
                                },
                            )
                        })
                        .collect();
                    let mut selected_primary = profile.ir.primary_provider.clone();
                    let selected_primary_text = primary_options
                        .iter()
                        .find(|(provider_key, _)| provider_key == &profile.ir.primary_provider)
                        .map(|(_, label)| label.clone())
                        .unwrap_or_else(|| {
                            if profile.ir.primary_provider.is_empty() {
                                "未設定".to_string()
                            } else {
                                profile.ir.primary_provider.clone()
                            }
                        });
                    egui::ComboBox::from_label("Primary IR")
                        .selected_text(selected_primary_text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected_primary, String::new(), "未設定");
                            for (provider_key, label) in &primary_options {
                                ui.selectable_value(
                                    &mut selected_primary,
                                    provider_key.clone(),
                                    label,
                                );
                            }
                        });
                    if selected_primary != profile.ir.primary_provider {
                        profile.ir.primary_provider = selected_primary;
                        sync_ir_provider_roles(&mut profile.ir);
                    }
                    ui.checkbox(
                        &mut profile.ir.prefetch_global_ranking_on_score_submit,
                        "スコア送信後に全体順位を取得",
                    );
                    egui::ComboBox::from_label("秘密情報の保存先 (要再起動)")
                        .selected_text(match profile.ir.credential_store {
                            IrCredentialStoreConfig::File => "ファイル (プロファイル内)",
                            IrCredentialStoreConfig::Os => "OS credential store",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.ir.credential_store,
                                IrCredentialStoreConfig::File,
                                "ファイル (プロファイル内)",
                            );
                            ui.selectable_value(
                                &mut profile.ir.credential_store,
                                IrCredentialStoreConfig::Os,
                                "OS credential store",
                            );
                        });
                    ui.checkbox(
                        &mut profile.ir.prefetch_rival_ranking_on_score_submit,
                        "スコア送信後にライバル順位を取得",
                    );
                    let mut remove_index = None;
                    for (index, provider) in profile.ir.providers.iter_mut().enumerate() {
                        ui.push_id(("ir_provider", index), |ui| {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut provider.enabled, "");
                                ui.label(format!("provider {}", index + 1));
                                if ui.button("削除").clicked() {
                                    remove_index = Some(index);
                                }
                            });
                            ir_provider_text_row(ui, "Base URL", &mut provider.base_url);
                            let row_target = IrProviderUiTarget::new(
                                provider.provider.clone(),
                                provider.base_url.clone(),
                            );
                            let provider_key =
                                crate::ir::provider_key::configured_provider_key(provider)
                                    .map(str::to_string);
                            ui.horizontal(|ui| {
                                ui.label("Key");
                                ui.monospace(
                                    provider_key.as_deref().unwrap_or("(ログイン後に取得)"),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("メール");
                                ui.text_edit_singleline(&mut ir_login.email);
                            });
                            ui.horizontal(|ui| {
                                ui.label("パスワード");
                                ui.add(
                                    egui::TextEdit::singleline(&mut ir_login.password)
                                        .password(true),
                                );
                            });
                            ui.horizontal(|ui| {
                                let can_login = !ir_login.busy
                                    && !provider.base_url.is_empty()
                                    && !ir_login.email.is_empty()
                                    && !ir_login.password.is_empty();
                                if ui
                                    .add_enabled(can_login, egui::Button::new("ログイン"))
                                    .clicked()
                                {
                                    ir_login.start_login(
                                        profile_root.to_path_buf(),
                                        provider.provider.clone(),
                                        provider.base_url.clone(),
                                    );
                                }
                                let login_busy =
                                    ir_login.busy_target.as_ref().is_some_and(|target| {
                                        target.matches(&provider.provider, &provider.base_url)
                                    });
                                if login_busy {
                                    ui.spinner();
                                }
                                if ui.button("ログアウト").clicked() {
                                    let result = provider_key
                                        .as_deref()
                                        .map(|provider_key| {
                                            crate::ir::credentials::delete_credentials(
                                                profile_root,
                                                provider_key,
                                            )
                                        })
                                        .transpose();
                                    match result {
                                        Ok(_) => {
                                            provider.enabled = false;
                                            ir_login.message = Some(IrProviderUiMessage {
                                                target: row_target.clone(),
                                                ok: true,
                                                text: "ログアウトしました".to_string(),
                                            });
                                            save_clicked = true;
                                        }
                                        Err(error) => {
                                            ir_login.message = Some(IrProviderUiMessage {
                                                target: row_target.clone(),
                                                ok: false,
                                                text: format!("{error:#}"),
                                            });
                                        }
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                let busy = ir_device_key.busy_provider.as_deref()
                                    == provider_key.as_deref();
                                let can_rotate = !busy
                                    && !provider.base_url.is_empty()
                                    && provider_key.is_some();
                                if ui
                                    .add_enabled(can_rotate, egui::Button::new("署名鍵を再生成"))
                                    .clicked()
                                {
                                    ir_device_key.start_rotate(
                                        profile_root.to_path_buf(),
                                        provider.provider.clone(),
                                        provider_key.clone().unwrap_or_default(),
                                        provider.base_url.clone(),
                                    );
                                }
                                if busy {
                                    ui.spinner();
                                }
                            });
                            if let Some(message) = &ir_login.message
                                && message.target.matches(&provider.provider, &provider.base_url)
                            {
                                let color = if message.ok {
                                    egui::Color32::LIGHT_GREEN
                                } else {
                                    egui::Color32::LIGHT_RED
                                };
                                ui.colored_label(color, message.text.clone());
                            }
                            if let Some(message) = &ir_device_key.message
                                && message.target.matches(&provider.provider, &provider.base_url)
                            {
                                let color = if message.ok {
                                    egui::Color32::LIGHT_GREEN
                                } else {
                                    egui::Color32::LIGHT_RED
                                };
                                ui.colored_label(color, message.text.clone());
                            }
                            egui::ComboBox::from_label("送信方針")
                                .selected_text(ir_send_policy_label(provider.send_policy))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        IrSendPolicyConfig::UpdateScore,
                                        IrSendPolicyConfig::Always,
                                        IrSendPolicyConfig::CompleteSong,
                                    ] {
                                        ui.selectable_value(
                                            &mut provider.send_policy,
                                            value,
                                            ir_send_policy_label(value),
                                        );
                                    }
                                });
                            ui.horizontal(|ui| {
                                ui.label("最終ログイン");
                                ui.monospace(format_optional_timestamp(provider.last_login_at));
                            });
                            ui.horizontal(|ui| {
                                ui.label("最終成功");
                                ui.monospace(format_optional_timestamp(provider.last_success_at));
                            });
                        });
                    }
                    if let Some(index) = remove_index {
                        profile.ir.providers.remove(index);
                    }
                    if ui.button("provider を追加").clicked() {
                        profile.ir.providers.push(IrProviderConfig {
                            provider: "bmz".to_string(),
                            provider_key: String::new(),
                            base_url: String::new(),
                            enabled: false,
                            account_display_name: String::new(),
                            account_id: String::new(),
                            send_policy: IrSendPolicyConfig::default(),
                            role: IrProviderRoleConfig::default(),
                            last_login_at: None,
                            last_success_at: None,
                        });
                    }
                });

                egui::CollapsingHeader::new("UI").show(ui, |ui| {
                    if !unrestricted {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        ui.label("言語 (未実装)");
                        ui.text_edit_singleline(&mut profile.ui.language);
                    });
                    ui.horizontal(|ui| {
                        ui.label("テーマ (未実装)");
                        ui.text_edit_singleline(&mut profile.ui.theme);
                    });
                    if ui.checkbox(show_fps, "FPS 表示").changed() {
                        profile.ui.show_fps = *show_fps;
                    }
                    ui.checkbox(&mut profile.ui.confirm_on_exit, "終了確認 (未実装)");
                });

                ui.separator();
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
            });
        },
    );
    if let Some(readonly) = readonly_profile {
        restore_restricted_profile_settings(profile, readonly);
    }
    if let Some(readonly) = readonly_app_config {
        *app_config = readonly;
        save_app_config = false;
    }
    ProfileSettingsPanelActions { save: save_clicked, save_app_config }
}

fn build_profile_manager_section(
    ui: &mut egui::Ui,
    app_config: &mut AppConfig,
    profile: &ProfileConfig,
    state: &mut ProfileManagerUiState,
    editable: bool,
) -> bool {
    let mut save_app_config = false;
    egui::CollapsingHeader::new("プロファイル管理").default_open(false).show(ui, |ui| {
        if !editable {
            ui.disable();
        }
        let app_paths = match resolve_app_paths() {
            Ok(paths) => paths,
            Err(error) => {
                ui.colored_label(egui::Color32::RED, format!("{error:#}"));
                return;
            }
        };
        let profiles = match profile_cmd::profile_summaries(&app_paths) {
            Ok(profiles) => profiles,
            Err(error) => {
                ui.colored_label(egui::Color32::RED, format!("{error:#}"));
                return;
            }
        };

        if state.copy_source_id.is_empty() {
            state.copy_source_id = profile.id.clone();
        }

        ui.horizontal(|ui| {
            ui.label("実行中");
            ui.monospace(&profile.id);
        });
        ui.horizontal(|ui| {
            ui.label("次回起動");
            egui::ComboBox::from_id_salt("profile_active_next")
                .selected_text(profile_selection_label(&profiles, &app_config.active_profile))
                .show_ui(ui, |ui| {
                    let active_profile = app_config.active_profile.clone();
                    for summary in &profiles {
                        let selected = summary.id == active_profile;
                        let label = profile_selection_label(&profiles, &summary.id);
                        if ui.selectable_label(selected, label).clicked() && !selected {
                            app_config.active_profile = summary.id.clone();
                            state.message =
                                format!("次回起動 profile を {} に変更しました。", summary.id);
                            state.error.clear();
                            save_app_config = true;
                        }
                    }
                });
        });

        ui.separator();
        ui.label("新規作成");
        ui.horizontal(|ui| {
            ui.label("ID");
            profile_id_text_edit(ui, &mut state.create_id);
        });
        ui.horizontal(|ui| {
            ui.label("表示名");
            ui.text_edit_singleline(&mut state.create_display_name);
        });
        ui.checkbox(&mut state.create_activate, "次回起動 profile にする");
        if ui.button("作成").clicked() {
            let id = state.create_id.trim().to_string();
            let display_name = trimmed_non_empty(&state.create_display_name).map(str::to_string);
            match profile_cmd::create_profile(&app_paths, &id, display_name.as_deref(), false) {
                Ok(()) => {
                    if state.create_activate {
                        app_config.active_profile = id.clone();
                        save_app_config = true;
                    }
                    state.message = format!("profile を作成しました: {id}");
                    state.error.clear();
                    state.create_id.clear();
                    state.create_display_name.clear();
                }
                Err(error) => {
                    state.error = format!("{error:#}");
                    state.message.clear();
                }
            }
        }

        ui.separator();
        ui.label("複製");
        ui.horizontal(|ui| {
            ui.label("複製元");
            egui::ComboBox::from_id_salt("profile_copy_source")
                .selected_text(profile_selection_label(&profiles, &state.copy_source_id))
                .show_ui(ui, |ui| {
                    for summary in &profiles {
                        let selected = summary.id == state.copy_source_id;
                        let label = profile_selection_label(&profiles, &summary.id);
                        if ui.selectable_label(selected, label).clicked() {
                            state.copy_source_id = summary.id.clone();
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("新ID");
            profile_id_text_edit(ui, &mut state.copy_target_id);
        });
        ui.horizontal(|ui| {
            ui.label("表示名");
            ui.text_edit_singleline(&mut state.copy_display_name);
        });
        ui.checkbox(&mut state.copy_activate, "次回起動 profile にする");
        if ui.button("複製").clicked() {
            let source_id = state.copy_source_id.trim().to_string();
            let target_id = state.copy_target_id.trim().to_string();
            let display_name = trimmed_non_empty(&state.copy_display_name).map(str::to_string);
            match profile_cmd::copy_profile(
                &app_paths,
                &source_id,
                &target_id,
                display_name.as_deref(),
                false,
            ) {
                Ok(()) => {
                    if state.copy_activate {
                        app_config.active_profile = target_id.clone();
                        save_app_config = true;
                    }
                    state.message = format!("profile を複製しました: {source_id} -> {target_id}");
                    state.error.clear();
                    state.copy_target_id.clear();
                    state.copy_display_name.clear();
                }
                Err(error) => {
                    state.error = format!("{error:#}");
                    state.message.clear();
                }
            }
        }

        if !state.message.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, state.message.as_str());
        }
        if !state.error.is_empty() {
            ui.colored_label(egui::Color32::RED, state.error.as_str());
        }
    });
    save_app_config
}

fn profile_selection_label(
    profiles: &[crate::storage::profile::ProfileSummary],
    profile_id: &str,
) -> String {
    profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| format!("{} ({})", profile.id, profile.display_name))
        .unwrap_or_else(|| profile_id.to_string())
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn profile_id_text_edit(ui: &mut egui::Ui, value: &mut String) {
    if ui.text_edit_singleline(value).changed() {
        sanitize_profile_id_input(value);
    }
}

fn sanitize_profile_id_input(value: &mut String) {
    value.retain(is_profile_id_char);
    if value.len() > 64 {
        value.truncate(64);
    }
}

fn is_profile_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn volume_slider(ui: &mut egui::Ui, value: &mut u32, label: &str) {
    ui.add(egui::Slider::new(value, 0..=100).text(label));
}

fn lane_unit_slider(ui: &mut egui::Ui, value: &mut u32, label: &str) {
    lane_unit_slider_with_max(ui, value, label, 1000);
}

fn lane_unit_slider_with_max(ui: &mut egui::Ui, value: &mut u32, label: &str, max: u32) {
    *value = (*value).min(max);
    ui.add(egui::Slider::new(value, 0..=max).text(label));
}

fn offset_ms_slider(ui: &mut egui::Ui, value_us: &mut i64, label: &str) {
    let mut value_ms = (*value_us / 1_000).clamp(-500, 500);
    if ui.add(egui::Slider::new(&mut value_ms, -500..=500).text(format!("{label} (ms)"))).changed()
    {
        *value_us = value_ms * 1_000;
    }
}

fn judge_algorithm_label(value: JudgeAlgorithmConfig) -> &'static str {
    match value {
        JudgeAlgorithmConfig::Combo => "COMBO",
        JudgeAlgorithmConfig::Duration => "DURATION",
        JudgeAlgorithmConfig::Lowest => "LOWEST",
        JudgeAlgorithmConfig::Score => "SCORE",
    }
}

fn fast_slow_scope_label(value: FastSlowDisplayScope) -> &'static str {
    match value {
        FastSlowDisplayScope::Auto => "Auto (beatoraja 準拠)",
        FastSlowDisplayScope::ThresholdMs => "閾値 ms (PGREAT 含む全判定)",
    }
}

fn rule_mode_label(value: RuleMode) -> &'static str {
    match value {
        RuleMode::Beatoraja => "BEATORAJA",
        RuleMode::Lr2Oraja => "LR2ORAJA",
        RuleMode::Dx => "DX",
    }
}

fn gauge_label(value: GaugeTypeConfig) -> &'static str {
    match value {
        GaugeTypeConfig::AssistEasy => "ASSIST EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX HARD",
        GaugeTypeConfig::AutoShift => "AUTO SHIFT",
        GaugeTypeConfig::Hazard => "HAZARD",
    }
}

fn gauge_auto_shift_label(value: GaugeAutoShiftConfig) -> &'static str {
    match value {
        GaugeAutoShiftConfig::Off => "OFF",
        GaugeAutoShiftConfig::Continue => "CONTINUE",
        GaugeAutoShiftConfig::HardToGroove => "HARD->GROOVE",
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR",
        GaugeAutoShiftConfig::SelectToUnder => "SELECT UNDER",
    }
}

fn bottom_shiftable_gauge_label(value: BottomShiftableGaugeConfig) -> &'static str {
    match value {
        BottomShiftableGaugeConfig::AssistEasy => "ASSIST EASY",
        BottomShiftableGaugeConfig::Easy => "EASY",
        BottomShiftableGaugeConfig::Normal => "NORMAL",
    }
}

fn random_label(value: RandomOptionConfig) -> &'static str {
    match value {
        RandomOptionConfig::Off => "OFF",
        RandomOptionConfig::Mirror => "MIRROR",
        RandomOptionConfig::Random => "RANDOM",
        RandomOptionConfig::RRandom => "R-RANDOM",
        RandomOptionConfig::SRandom => "S-RANDOM",
        RandomOptionConfig::Spiral => "SPIRAL",
        RandomOptionConfig::HRandom => "H-RANDOM",
        RandomOptionConfig::AllScratch => "ALL-SCR",
        RandomOptionConfig::RandomEx => "RANDOM-EX",
        RandomOptionConfig::SRandomEx => "S-RANDOM-EX",
        RandomOptionConfig::FRandom => "F-RANDOM",
        RandomOptionConfig::MFRandom => "MF-RANDOM",
    }
}

fn random_options() -> [(RandomOptionConfig, &'static str); 12] {
    [
        (RandomOptionConfig::Off, "OFF"),
        (RandomOptionConfig::Mirror, "MIRROR"),
        (RandomOptionConfig::Random, "RANDOM"),
        (RandomOptionConfig::RRandom, "R-RANDOM"),
        (RandomOptionConfig::SRandom, "S-RANDOM"),
        (RandomOptionConfig::Spiral, "SPIRAL"),
        (RandomOptionConfig::HRandom, "H-RANDOM"),
        (RandomOptionConfig::AllScratch, "ALL-SCR"),
        (RandomOptionConfig::RandomEx, "RANDOM-EX"),
        (RandomOptionConfig::SRandomEx, "S-RANDOM-EX"),
        (RandomOptionConfig::FRandom, "F-RANDOM"),
        (RandomOptionConfig::MFRandom, "MF-RANDOM"),
    ]
}

fn double_option_label(value: DoubleOptionConfig) -> &'static str {
    match value {
        DoubleOptionConfig::Off => "OFF",
        DoubleOptionConfig::Flip => "FLIP",
        DoubleOptionConfig::Battle => "BATTLE",
        DoubleOptionConfig::BattleAutoScratch => "BATTLE AS",
    }
}

fn hs_fix_label(value: HsFixConfig) -> &'static str {
    match value {
        HsFixConfig::Off => "OFF",
        HsFixConfig::StartBpm => "START BPM",
        HsFixConfig::MinBpm => "MIN BPM",
        HsFixConfig::MaxBpm => "MAX BPM",
        HsFixConfig::MainBpm => "MAIN BPM",
    }
}

fn target_label(value: TargetOptionConfig) -> String {
    match value {
        TargetOptionConfig::None => "NONE".to_string(),
        TargetOptionConfig::RankA => "RANK_A".to_string(),
        TargetOptionConfig::RankAaMinus => "RANK_AA-".to_string(),
        TargetOptionConfig::RankAa => "RANK_AA".to_string(),
        TargetOptionConfig::RankAaaMinus => "RANK_AAA-".to_string(),
        TargetOptionConfig::RankAaa => "RANK_AAA".to_string(),
        TargetOptionConfig::RankMaxMinus => "RANK_MAX-".to_string(),
        TargetOptionConfig::Max => "MAX".to_string(),
        TargetOptionConfig::RankNext => "RANK_NEXT".to_string(),
        TargetOptionConfig::IrTop => "IR_TOP".to_string(),
        TargetOptionConfig::IrNext => "IR_NEXT".to_string(),
        TargetOptionConfig::RivalTop => "RIVAL TOP".to_string(),
        TargetOptionConfig::RivalNext => "RIVAL NEXT".to_string(),
        TargetOptionConfig::RivalIndex(index) => format!("RIVAL_{index}"),
    }
}

fn grade_diff_display_label(value: ResultGradeDiffDisplay) -> &'static str {
    match value {
        ResultGradeDiffDisplay::Next => "NEXT",
        ResultGradeDiffDisplay::Nearest => "NEAREST",
    }
}

fn lane_effect_label(value: LaneEffectConfig) -> &'static str {
    match value {
        LaneEffectConfig::Off => "OFF",
        LaneEffectConfig::Hidden => "HIDDEN",
        LaneEffectConfig::Sudden => "SUDDEN",
        LaneEffectConfig::HiddenSudden => "HIDDEN+SUDDEN",
    }
}

fn assist_label(value: AssistOptionConfig) -> &'static str {
    match value {
        AssistOptionConfig::None => "NONE",
        AssistOptionConfig::AutoScratch => "AUTO SCRATCH",
        AssistOptionConfig::LegacyNote => "LEGACY NOTE",
    }
}

fn bga_mode_label(value: BgaModeConfig) -> &'static str {
    match value {
        BgaModeConfig::On => "ON",
        BgaModeConfig::Auto => "AUTO",
        BgaModeConfig::Off => "OFF",
    }
}

fn bga_expand_label(value: BgaExpandConfig) -> &'static str {
    match value {
        BgaExpandConfig::Full => "FULL",
        BgaExpandConfig::KeepAspect => "KEEP ASPECT",
        BgaExpandConfig::Off => "OFF",
    }
}

fn hispeed_mode_label(value: HispeedModeConfig) -> &'static str {
    match value {
        HispeedModeConfig::Normal => "NORMAL",
        HispeedModeConfig::Floating => "FLOATING",
    }
}

fn scratch_input_mode_label(value: ScratchInputMode) -> &'static str {
    match value {
        ScratchInputMode::Normal => "NORMAL",
        ScratchInputMode::AnyDirection => "ANY DIRECTION",
    }
}

fn replay_slot_rule_label(value: ReplaySlotRule) -> &'static str {
    match value {
        ReplaySlotRule::Disabled => "DISABLED",
        ReplaySlotRule::Always => "ALWAYS",
        ReplaySlotRule::ScoreUpdate => "SCORE UPDATE",
        ReplaySlotRule::BpUpdate => "BP UPDATE",
        ReplaySlotRule::MaxComboUpdate => "MAX COMBO UPDATE",
        ReplaySlotRule::ClearUpdate => "CLEAR UPDATE",
    }
}

fn system_sound_path_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
        if ui.button("選択…").clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            *value = folder.to_string_lossy().into_owned();
        }
    });
}

fn ir_provider_text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

fn ir_send_policy_label(value: IrSendPolicyConfig) -> &'static str {
    match value {
        IrSendPolicyConfig::UpdateScore => "UPDATE SCORE",
        IrSendPolicyConfig::Always => "ALWAYS",
        IrSendPolicyConfig::CompleteSong => "COMPLETE SONG",
    }
}

fn ir_primary_provider_label(provider: &IrProviderConfig, provider_key: &str) -> String {
    let account = provider.account_display_name.trim();
    if account.is_empty() {
        format!("{provider_key} ({})", provider.base_url)
    } else {
        format!("{provider_key} - {account} ({})", provider.base_url)
    }
}

fn sync_ir_provider_roles(ir_config: &mut IrConfig) -> bool {
    let primary_provider = ir_config.primary_provider.trim();
    let mut changed = false;
    for provider in &mut ir_config.providers {
        let next_role = if !primary_provider.is_empty()
            && crate::ir::provider_key::configured_provider_key(provider)
                .is_some_and(|provider_key| provider_key == primary_provider)
        {
            IrProviderRoleConfig::Primary
        } else {
            IrProviderRoleConfig::SubmitOnly
        };
        if provider.role != next_role {
            provider.role = next_role;
            changed = true;
        }
    }
    changed
}

fn format_optional_timestamp(value: Option<i64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_else(|| "-".to_string())
}

/// スキン設定パネルからのアクション要求。
struct SkinPanelActions {
    /// 「保存」ボタンが押された (profile.toml へ書き出し)。
    save: bool,
    /// 「リセット」ボタンが押された (profile.toml の値へ戻す)。
    reset: bool,
    /// パネル内のスキン設定変更に対して必要な反映対象。
    reload: SkinReloadRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinSlot {
    Select,
    Decide,
    Play4,
    Play5,
    Play6,
    Play7,
    Play8,
    Play9,
    Play10,
    Play14,
    Result,
    CourseResult,
}

fn skin_reload_request_from_diff(before: &SkinConfig, after: &SkinConfig) -> SkinReloadRequest {
    let mut request = SkinReloadRequest::default();
    if before.select != after.select
        || before.select_options != after.select_options
        || before.select_files != after.select_files
    {
        request.select = true;
    }
    if before.decide != after.decide
        || before.decide_options != after.decide_options
        || before.decide_files != after.decide_files
    {
        request.decide = true;
    }
    if before.play4 != after.play4
        || before.play4_options != after.play4_options
        || before.play4_files != after.play4_files
    {
        request.play4 = true;
    }
    if before.play5 != after.play5
        || before.play5_options != after.play5_options
        || before.play5_files != after.play5_files
    {
        request.play5 = true;
    }
    if before.play6 != after.play6
        || before.play6_options != after.play6_options
        || before.play6_files != after.play6_files
    {
        request.play6 = true;
    }
    if before.play7 != after.play7
        || before.play7_options != after.play7_options
        || before.play7_files != after.play7_files
    {
        request.play7 = true;
    }
    if before.play8 != after.play8
        || before.play8_options != after.play8_options
        || before.play8_files != after.play8_files
    {
        request.play8 = true;
    }
    if before.play9 != after.play9
        || before.play9_options != after.play9_options
        || before.play9_files != after.play9_files
    {
        request.play9 = true;
    }
    if before.play10 != after.play10
        || before.play10_options != after.play10_options
        || before.play10_files != after.play10_files
    {
        request.play10 = true;
    }
    if before.play14 != after.play14
        || before.play14_options != after.play14_options
        || before.play14_files != after.play14_files
    {
        request.play14 = true;
    }
    if before.result != after.result
        || before.result_options != after.result_options
        || before.result_files != after.result_files
    {
        request.result = true;
    }
    if before.course_result != after.course_result
        || before.course_result_options != after.course_result_options
        || before.course_result_files != after.course_result_files
    {
        request.course_result = true;
    }
    request.offsets = before.offsets != after.offsets;
    request
}

fn skin_path_combo(
    ui: &mut egui::Ui,
    skin: &mut SkinConfig,
    slot: SkinSlot,
    label: &str,
    candidates: &[SkinCandidate],
    show_bundled_origin: bool,
) -> bool {
    ui.label(label);
    let current = skin_slot_path(skin, slot).to_string();
    let mut selected = current.clone();
    let selected_text = skin_candidate_label(candidates, &current, show_bundled_origin);
    egui::ComboBox::from_id_salt(("skin_path_combo", label))
        .selected_text(selected_text)
        .width(320.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, String::new(), "(デフォルト)");
            for candidate in candidates {
                let response = ui.selectable_value(
                    &mut selected,
                    candidate.path.clone(),
                    skin_candidate_display(candidate, show_bundled_origin),
                );
                match candidate.origin {
                    SkinCandidateOrigin::Bundled if show_bundled_origin => {
                        response.on_hover_text(
                            "同梱スキンです。編集する場合は data_dir/skins にコピーしてユーザースキンとして選択してください。",
                        );
                    }
                    SkinCandidateOrigin::Bundled => {}
                    SkinCandidateOrigin::User => {
                        response.on_hover_text("data_dir/skins 配下のユーザースキンです。");
                    }
                    SkinCandidateOrigin::External => {
                        response.on_hover_text("BMZ の data_dir 外にある外部スキンです。");
                    }
                }
            }
        });
    let combo_changed = selected != current;
    if combo_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = selected;
        restore_skin_slot_history(skin, slot);
    }
    let mut edited_path = skin_slot_path(skin, slot).to_string();
    let text_changed = ui.text_edit_singleline(&mut edited_path).changed();
    if text_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = edited_path;
        restore_skin_slot_history(skin, slot);
    }
    combo_changed || text_changed
}

fn skin_candidate_label(
    candidates: &[SkinCandidate],
    current: &str,
    show_bundled_origin: bool,
) -> String {
    if current.is_empty() {
        return "(デフォルト)".to_string();
    }
    candidates
        .iter()
        .find(|candidate| candidate.path == current)
        .map(|candidate| skin_candidate_display(candidate, show_bundled_origin))
        .unwrap_or_else(|| current.to_string())
}

fn skin_candidate_display(candidate: &SkinCandidate, show_bundled_origin: bool) -> String {
    let label = skin_candidate_origin_label(candidate.origin, show_bundled_origin);
    let text = if candidate.name.is_empty() {
        candidate.path.clone()
    } else {
        format!("{} ({})", candidate.name, candidate.path)
    };
    if let Some(label) = label { format!("{label} {text}") } else { text }
}

fn skin_candidate_origin_label(
    origin: SkinCandidateOrigin,
    show_bundled_origin: bool,
) -> Option<&'static str> {
    match origin {
        SkinCandidateOrigin::Bundled if show_bundled_origin => Some("[同梱]"),
        SkinCandidateOrigin::Bundled => None,
        SkinCandidateOrigin::User => Some("[ユーザー]"),
        SkinCandidateOrigin::External => Some("[外部]"),
    }
}

fn show_bundled_skin_origin(app_paths: &AppPaths, skin_catalog: &SkinCatalog) -> bool {
    !app_paths.hides_bundled_skin_label() && skin_catalog_has_non_bundled_candidate(skin_catalog)
}

fn skin_catalog_has_non_bundled_candidate(skin_catalog: &SkinCatalog) -> bool {
    let groups: [&[SkinCandidate]; 12] = [
        &skin_catalog.select,
        &skin_catalog.decide,
        &skin_catalog.play4,
        &skin_catalog.play5,
        &skin_catalog.play6,
        &skin_catalog.play7,
        &skin_catalog.play8,
        &skin_catalog.play9,
        &skin_catalog.play10,
        &skin_catalog.play14,
        &skin_catalog.result,
        &skin_catalog.course_result,
    ];
    groups.iter().any(|candidates| {
        candidates.iter().any(|candidate| candidate.origin != SkinCandidateOrigin::Bundled)
    })
}

fn skin_slot_path(skin: &SkinConfig, slot: SkinSlot) -> &str {
    match slot {
        SkinSlot::Select => &skin.select,
        SkinSlot::Decide => &skin.decide,
        SkinSlot::Play4 => &skin.play4,
        SkinSlot::Play5 => &skin.play5,
        SkinSlot::Play6 => &skin.play6,
        SkinSlot::Play7 => &skin.play7,
        SkinSlot::Play8 => &skin.play8,
        SkinSlot::Play9 => &skin.play9,
        SkinSlot::Play10 => &skin.play10,
        SkinSlot::Play14 => &skin.play14,
        SkinSlot::Result => &skin.result,
        SkinSlot::CourseResult => &skin.course_result,
    }
}

fn skin_slot_path_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut String {
    match slot {
        SkinSlot::Select => &mut skin.select,
        SkinSlot::Decide => &mut skin.decide,
        SkinSlot::Play4 => &mut skin.play4,
        SkinSlot::Play5 => &mut skin.play5,
        SkinSlot::Play6 => &mut skin.play6,
        SkinSlot::Play7 => &mut skin.play7,
        SkinSlot::Play8 => &mut skin.play8,
        SkinSlot::Play9 => &mut skin.play9,
        SkinSlot::Play10 => &mut skin.play10,
        SkinSlot::Play14 => &mut skin.play14,
        SkinSlot::Result => &mut skin.result,
        SkinSlot::CourseResult => &mut skin.course_result,
    }
}

fn skin_slot_options_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_options,
        SkinSlot::Decide => &mut skin.decide_options,
        SkinSlot::Play4 => &mut skin.play4_options,
        SkinSlot::Play5 => &mut skin.play5_options,
        SkinSlot::Play6 => &mut skin.play6_options,
        SkinSlot::Play7 => &mut skin.play7_options,
        SkinSlot::Play8 => &mut skin.play8_options,
        SkinSlot::Play9 => &mut skin.play9_options,
        SkinSlot::Play10 => &mut skin.play10_options,
        SkinSlot::Play14 => &mut skin.play14_options,
        SkinSlot::Result => &mut skin.result_options,
        SkinSlot::CourseResult => &mut skin.course_result_options,
    }
}

fn skin_slot_files_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_files,
        SkinSlot::Decide => &mut skin.decide_files,
        SkinSlot::Play4 => &mut skin.play4_files,
        SkinSlot::Play5 => &mut skin.play5_files,
        SkinSlot::Play6 => &mut skin.play6_files,
        SkinSlot::Play7 => &mut skin.play7_files,
        SkinSlot::Play8 => &mut skin.play8_files,
        SkinSlot::Play9 => &mut skin.play9_files,
        SkinSlot::Play10 => &mut skin.play10_files,
        SkinSlot::Play14 => &mut skin.play14_files,
        SkinSlot::Result => &mut skin.result_files,
        SkinSlot::CourseResult => &mut skin.course_result_files,
    }
}

fn save_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    if path.is_empty() {
        return;
    }
    let options = skin_slot_options_mut(skin, slot).clone();
    let files = skin_slot_files_mut(skin, slot).clone();
    skin.history
        .insert(path, SkinHistoryEntryConfig { options, files, offsets: skin.offsets.clone() });
}

fn restore_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    let Some(entry) = skin.history.get(&path).cloned() else {
        skin_slot_options_mut(skin, slot).clear();
        skin_slot_files_mut(skin, slot).clear();
        skin.offsets.clear();
        return;
    };
    *skin_slot_options_mut(skin, slot) = entry.options;
    *skin_slot_files_mut(skin, slot) = entry.files;
    skin.offsets = entry.offsets;
}

/// プロファイルのスキン設定 (`SkinConfig`) を編集するパネル。
fn build_skin_panel(
    ctx: &egui::Context,
    open: &mut bool,
    skin: &mut SkinConfig,
    skin_meta: &SkinConfigMeta,
    skin_catalog: &SkinCatalog,
    app_paths: &AppPaths,
) -> SkinPanelActions {
    let mut save_clicked = false;
    let mut reset_clicked = false;
    let mut changed = false;
    let before_skin = skin.clone();
    let show_bundled_origin = show_bundled_skin_origin(app_paths, skin_catalog);
    sized_panel_window("スキン設定", ctx, open, 440.0, 560.0, egui::pos2(16.0, 480.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
            ui.label("各画面のスキン。空欄なら内蔵描画 / デフォルトスキンを使用します。");
            egui::Grid::new("skin_grid").num_columns(2).show(ui, |ui| {
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Select,
                    "選曲",
                    &skin_catalog.select,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Decide,
                    "決定",
                    &skin_catalog.decide,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play4,
                    "プレイ (4K)",
                    &skin_catalog.play4,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play5,
                    "プレイ (5K)",
                    &skin_catalog.play5,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play6,
                    "プレイ (6K)",
                    &skin_catalog.play6,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play7,
                    "プレイ (7K)",
                    &skin_catalog.play7,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play8,
                    "プレイ (8K)",
                    &skin_catalog.play8,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play9,
                    "プレイ (9K)",
                    &skin_catalog.play9,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play10,
                    "プレイ (10K)",
                    &skin_catalog.play10,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play14,
                    "プレイ (14K)",
                    &skin_catalog.play14,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Result,
                    "リザルト",
                    &skin_catalog.result,
                    show_bundled_origin,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::CourseResult,
                    "コースリザルト",
                    &skin_catalog.course_result,
                    show_bundled_origin,
                );
                ui.end_row();
            });
            ui.separator();
            ui.label("読み込み済みスキンが宣言する設定可能項目:");
            let select_root = skin_root_path(app_paths, &skin.select);
            let decide_root = skin_root_path(app_paths, &skin.decide);
            let play4_root = skin_root_path(app_paths, &skin.play4);
            let play5_root = skin_root_path(app_paths, &skin.play5);
            let play6_root = skin_root_path(app_paths, &skin.play6);
            let play7_root = skin_root_path(app_paths, &skin.play7);
            let play8_root = skin_root_path(app_paths, &skin.play8);
            let play9_root = skin_root_path(app_paths, &skin.play9);
            let play10_root = skin_root_path(app_paths, &skin.play10);
            let play14_root = skin_root_path(app_paths, &skin.play14);
            let result_root = skin_root_path(app_paths, &skin.result);
            let course_result_root = skin_root_path(app_paths, &skin.course_result);
            changed |= build_scene_skin_defs(
                ui,
                "選曲スキン",
                &skin_meta.select,
                select_root.as_deref(),
                &mut skin.select_options,
                &mut skin.select_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "決定スキン",
                &skin_meta.decide,
                decide_root.as_deref(),
                &mut skin.decide_options,
                &mut skin.decide_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (4K)",
                &skin_meta.play4,
                play4_root.as_deref(),
                &mut skin.play4_options,
                &mut skin.play4_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (5K)",
                &skin_meta.play5,
                play5_root.as_deref(),
                &mut skin.play5_options,
                &mut skin.play5_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (6K)",
                &skin_meta.play6,
                play6_root.as_deref(),
                &mut skin.play6_options,
                &mut skin.play6_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (7K)",
                &skin_meta.play7,
                play7_root.as_deref(),
                &mut skin.play7_options,
                &mut skin.play7_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (8K)",
                &skin_meta.play8,
                play8_root.as_deref(),
                &mut skin.play8_options,
                &mut skin.play8_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (9K)",
                &skin_meta.play9,
                play9_root.as_deref(),
                &mut skin.play9_options,
                &mut skin.play9_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (10K)",
                &skin_meta.play10,
                play10_root.as_deref(),
                &mut skin.play10_options,
                &mut skin.play10_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (14K)",
                &skin_meta.play14,
                play14_root.as_deref(),
                &mut skin.play14_options,
                &mut skin.play14_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "リザルトスキン",
                &skin_meta.result,
                result_root.as_deref(),
                &mut skin.result_options,
                &mut skin.result_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "コースリザルトスキン",
                &skin_meta.course_result,
                course_result_root.as_deref(),
                &mut skin.course_result_options,
                &mut skin.course_result_files,
                &mut skin.offsets,
            );
            ui.separator();
            ui.label(
                "「保存」で profile.toml へ書き出し。「リセット」で保存済みの設定へ戻します。オプションの「デフォルトに戻す」は保存までディスクへ書きません。",
            );
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
                if ui.button("リセット").clicked() {
                    reset_clicked = true;
                }
            });
            });
        },
    );
    let reload = if changed {
        skin_reload_request_from_diff(&before_skin, skin)
    } else {
        Default::default()
    };
    SkinPanelActions { save: save_clicked, reset: reset_clicked, reload }
}

/// 1 シーン分のスキン設定可能項目を折りたたみ表示・編集する。
///
/// - property: ComboBox で選択肢を選び `options` へ書き込む。
/// - filepath: `path` グロブにマッチするファイルを ComboBox で選び `files` へ書き込む。
/// - offset: 宣言された要素ごとに x/y/w/h/r/a を編集し `offsets` (id 単位) へ反映。
fn build_scene_skin_defs(
    ui: &mut egui::Ui,
    label: &str,
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(label).show(ui, |ui| {
        if defs.is_empty() {
            ui.label("設定可能項目はありません (スキン未読込、または定義なし)。");
            return;
        }
        let _ = fill_missing_skin_defaults(defs, skin_root, options, files);
        if !defs.property.is_empty() {
            ui.strong("オプション");
            // property / filepath は同名 (例: "シャッター") を持ちうるので、egui の
            // ComboBox ID 衝突を防ぐためにカテゴリで名前空間を切る。
            ui.push_id("property", |ui| {
                for prop in &defs.property {
                    let mut selected =
                        options.get(&prop.name).cloned().unwrap_or_else(|| property_default(prop));
                    let before = selected.clone();
                    egui::ComboBox::from_label(&prop.name).selected_text(&selected).show_ui(
                        ui,
                        |ui| {
                            for item in &prop.item {
                                ui.selectable_value(&mut selected, item.name.clone(), &item.name);
                            }
                        },
                    );
                    if selected != before {
                        options.insert(prop.name.clone(), selected);
                        changed = true;
                    }
                }
            });
        }
        if !defs.filepath.is_empty() {
            ui.strong("ファイル選択");
            ui.push_id("filepath", |ui| {
                for filepath in &defs.filepath {
                    let mut selected = files.get(&filepath.name).cloned().unwrap_or_default();
                    let before = selected.clone();
                    let display = if selected.is_empty() {
                        "(未選択)"
                    } else if selected == RANDOM_FILE_SELECTION {
                        "ランダム"
                    } else {
                        filepath_selection_label(&selected)
                    };
                    egui::ComboBox::from_label(&filepath.name).selected_text(display).show_ui(
                        ui,
                        |ui| {
                            // beatoraja 同様、具体ファイルに加えて「ランダム」を選べる。
                            // ランダム選択時は毎ロードで候補からランダムに解決する。
                            ui.selectable_value(
                                &mut selected,
                                RANDOM_FILE_SELECTION.to_string(),
                                "ランダム",
                            );
                            // 候補列挙は ComboBox を開いたときだけ行う (毎フレームの fs 走査を回避)。
                            let candidates = match skin_root {
                                Some(root) => glob_candidates(root, &filepath.path),
                                None => Vec::new(),
                            };
                            if let Some(normalized) =
                                normalize_filepath_selection(&selected, &candidates)
                            {
                                selected = normalized;
                            }
                            if candidates.is_empty() {
                                ui.label("候補なし");
                            }
                            for candidate in candidates {
                                let label = filepath_selection_label(&candidate);
                                ui.selectable_value(&mut selected, candidate.clone(), label);
                            }
                        },
                    );
                    if selected != before {
                        files.insert(filepath.name.clone(), selected);
                        changed = true;
                    }
                }
            });
        }
        if !defs.offset.is_empty() {
            ui.strong("オフセット可能要素");
            for offset_def in &defs.offset {
                ui.push_id(offset_def.id, |ui| {
                    ui.label(format!(
                        "{} [{}] — id {}",
                        offset_def.name, offset_def.category, offset_def.id
                    ));
                    let existing = offsets.iter().find(|o| o.id == offset_def.id).copied();
                    let mut value = existing
                        .unwrap_or(SkinOffsetConfig { id: offset_def.id, ..Default::default() });
                    let before = value;
                    ui.horizontal(|ui| {
                        changed |= add_offset_drag_values(ui, offset_def, &mut value);
                    });
                    if value != before {
                        match offsets.iter_mut().find(|o| o.id == offset_def.id) {
                            Some(entry) => *entry = value,
                            None => offsets.push(value),
                        }
                        changed = true;
                    }
                });
            }
        }
        if !defs.is_empty() && ui.button("デフォルトに戻す").clicked() {
            changed |= reset_scene_skin_to_defaults(defs, skin_root, options, files, offsets);
        }
    });
    changed
}

/// 1 シーン分の options / files / 当該 offset id をスキン定義の factory default へ戻す。
fn reset_scene_skin_to_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    if defs.is_empty() {
        return false;
    }
    options.clear();
    files.clear();
    let scene_offset_ids: std::collections::HashSet<i32> =
        defs.offset.iter().map(|offset| offset.id).collect();
    offsets.retain(|offset| !scene_offset_ids.contains(&offset.id));
    fill_missing_skin_defaults(defs, skin_root, options, files)
}

fn fill_missing_skin_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
) -> bool {
    let mut changed = false;
    for prop in &defs.property {
        let current = options.get(&prop.name).map(String::as_str);
        if current.is_none() || !property_selection_is_valid(prop, current.unwrap_or_default()) {
            let default = property_default(prop);
            if current != Some(default.as_str()) {
                options.insert(prop.name.clone(), default);
                changed = true;
            }
        }
    }
    let Some(skin_root) = skin_root else {
        return changed;
    };
    for filepath in &defs.filepath {
        let candidates = glob_candidates(skin_root, &filepath.path);
        let current = files.get(&filepath.name).map(|value| value.replace('\\', "/"));
        // beatoraja は保存済み filepath を候補内に存在するか検証せず尊重する。
        // BMZ 旧版の相対パス保存も含め、空でなければここでは置き換えない。
        if current.as_ref().is_some_and(|selected| !selected.is_empty()) {
            continue;
        }
        if let Some(default) = filepath_default(filepath, &candidates) {
            if current.as_deref() != Some(default.as_str()) {
                files.insert(filepath.name.clone(), default);
                changed = true;
            }
        } else if current.as_deref() != Some("") {
            files.insert(filepath.name.clone(), String::new());
            changed = true;
        }
    }
    changed
}

fn add_offset_drag_values(
    ui: &mut egui::Ui,
    def: &SkinOffsetDef,
    value: &mut SkinOffsetConfig,
) -> bool {
    let mut changed = false;
    let mut any = false;
    if def.x {
        changed |= ui.add(egui::DragValue::new(&mut value.x).prefix("x:")).changed();
        any = true;
    }
    if def.y {
        changed |= ui.add(egui::DragValue::new(&mut value.y).prefix("y:")).changed();
        any = true;
    }
    if def.w {
        changed |= ui.add(egui::DragValue::new(&mut value.w).prefix("w:")).changed();
        any = true;
    }
    if def.h {
        changed |= ui.add(egui::DragValue::new(&mut value.h).prefix("h:")).changed();
        any = true;
    }
    if def.r {
        changed |= ui.add(egui::DragValue::new(&mut value.r).prefix("r:")).changed();
        any = true;
    }
    if def.a {
        changed |= ui.add(egui::DragValue::new(&mut value.a).prefix("a:")).changed();
        any = true;
    }
    if !any {
        ui.label("調整可能な値はありません");
    }
    changed
}

/// スキンパス文字列からスキンルートディレクトリ (親ディレクトリ) を得る。
fn skin_root_path(app_paths: &AppPaths, skin_path: &str) -> Option<PathBuf> {
    let trimmed = skin_path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = app_paths.resolve_path_ref(trimmed).ok()?;
    if path.is_dir() { Some(path) } else { path.parent().map(Path::to_path_buf) }
}

/// `pattern` (スキンルート相対、末尾要素にワイルドカード `*` を 1 個まで) に
/// マッチするファイルの相対パス一覧を返す。
///
/// beatoraja の `path|filter|` 形式の `|...|` 接尾辞 (lanecover などの
/// アセット用途タグ) は対象ファイル名には含まれないので、列挙前に取り除く。
fn glob_candidates(root: &Path, pattern: &str) -> Vec<String> {
    let pattern = pattern.replace('\\', "/");
    let pattern = pattern.split_once('|').map_or(pattern.as_str(), |(path, _)| path).to_string();
    let (dir_part, name_part) = match pattern.rfind('/') {
        Some(index) => (&pattern[..=index], &pattern[index + 1..]),
        None => ("", pattern.as_str()),
    };
    let Some((prefix, suffix)) = name_part.split_once('*') else {
        // ワイルドカード無し: パターンそのものを唯一の候補とする。
        return vec![pattern.clone()];
    };
    let dir = root.join(dir_part);
    let mut candidates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
            {
                candidates.push(format!("{dir_part}{name}"));
            }
        }
    }
    candidates.sort();
    candidates
}

fn normalize_filepath_selection(selected: &str, candidates: &[String]) -> Option<String> {
    if selected.is_empty() || selected == RANDOM_FILE_SELECTION {
        return None;
    }
    let normalized = selected.replace('\\', "/");
    if candidates.iter().any(|candidate| candidate == &normalized) {
        return (normalized != selected).then_some(normalized);
    }
    if normalized.contains('/') {
        return None;
    }
    candidates
        .iter()
        .find(|candidate| {
            filepath_selection_label(candidate).eq_ignore_ascii_case(normalized.as_str())
        })
        .cloned()
}

fn filepath_selection_label(value: &str) -> &str {
    let slash = value.rfind('/').into_iter().chain(value.rfind('\\')).max();
    match slash {
        Some(index) if index + 1 < value.len() => &value[index + 1..],
        _ => value,
    }
}

/// property の既定選択肢名。beatoraja と同じく `def` が item name と一致する
/// ときだけ採用し、未指定/不一致なら先頭 item を使う。
fn property_default(prop: &SkinPropertyDef) -> String {
    prop.item
        .iter()
        .find(|item| !prop.def.is_empty() && item.name == prop.def)
        .or_else(|| prop.item.first())
        .map(|item| item.name.clone())
        .unwrap_or_default()
}

fn property_selection_is_valid(prop: &SkinPropertyDef, selected: &str) -> bool {
    if let Ok(op) = selected.parse::<i32>() {
        return prop.item.iter().any(|item| item.op == op);
    }
    prop.item.iter().any(|item| item.name == selected)
}

fn filepath_default(filepath: &SkinFilepathDef, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    // def が "Random" のときは具体ファイルへ固定せず、ランダム番兵を既定にする
    // (beatoraja の def="Random" 相当)。
    if filepath.def.eq_ignore_ascii_case(RANDOM_FILE_SELECTION) {
        return Some(RANDOM_FILE_SELECTION.to_string());
    }
    if !filepath.def.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, &filepath.def))
    {
        return Some(candidate.clone());
    }
    if filepath.def.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, "default"))
    {
        return Some(candidate.clone());
    }
    candidates.first().cloned()
}

fn filename_matches_def(candidate: &str, def: &str) -> bool {
    let file_name = Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.eq_ignore_ascii_case(def) {
        return true;
    }
    let stem = Path::new(file_name).file_stem().and_then(|stem| stem.to_str()).unwrap_or(file_name);
    if stem.eq_ignore_ascii_case(def) {
        return true;
    }
    filepath_def_acronym(def).is_some_and(|acronym| {
        let stem_lower = stem.to_ascii_lowercase();
        let acronym_lower = acronym.to_ascii_lowercase();
        stem_lower == acronym_lower || stem_lower.starts_with(&acronym_lower)
    })
}

fn filepath_def_acronym(def: &str) -> Option<String> {
    if !def.contains('-') {
        return None;
    }
    let acronym = def
        .split('-')
        .filter_map(|part| part.chars().find(|ch| ch.is_ascii_alphanumeric()))
        .collect::<String>();
    (!acronym.is_empty()).then_some(acronym)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_and_play_restrict_settings_panels() {
        assert!(!scene_restricts_settings("Select"));
        assert!(scene_restricts_settings("Decide"));
        assert!(scene_restricts_settings("Play"));
        assert!(!scene_restricts_settings("Result"));
    }

    #[test]
    fn restricted_profile_settings_keep_only_realtime_categories() {
        let baseline = ProfileConfig::new_default("default", "Default", 1);
        let mut edited = baseline.clone();
        edited.display_name = "Changed".to_string();
        edited.play.rule_mode = RuleMode::Dx;
        edited.audio_mix.master_volume = 23;
        edited.judge.input_offset_us = 4_000;
        edited.lane.hispeed = 3.25;
        edited.input.analog_scratch_threshold = 321;

        restore_restricted_profile_settings(&mut edited, baseline.clone());

        assert_eq!(edited.display_name, baseline.display_name);
        assert_eq!(edited.play.rule_mode, baseline.play.rule_mode);
        assert_eq!(edited.audio_mix.master_volume, 23);
        assert_eq!(edited.judge.input_offset_us, 4_000);
        assert_eq!(edited.lane.hispeed, 3.25);
        assert_eq!(edited.input.analog_scratch_threshold, 321);
    }
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
    }

    #[test]
    fn sanitize_profile_id_input_keeps_portable_path_chars_only() {
        let mut value = "abc_日本語-_.012/\\: xyz".to_string();

        sanitize_profile_id_input(&mut value);

        assert_eq!(value, "abc_-_012xyz");
    }

    #[test]
    fn sanitize_profile_id_input_truncates_to_profile_id_limit() {
        let mut value = "a".repeat(80);

        sanitize_profile_id_input(&mut value);

        assert_eq!(value.len(), 64);
    }

    #[test]
    fn skin_candidate_display_hides_bundled_origin_label_when_requested() {
        let candidate = SkinCandidate {
            name: "Default".to_string(),
            path: "resource:skins/default/select.json".to_string(),
            origin: SkinCandidateOrigin::Bundled,
        };

        assert_eq!(
            skin_candidate_display(&candidate, true),
            "[同梱] Default (resource:skins/default/select.json)"
        );
        assert_eq!(
            skin_candidate_display(&candidate, false),
            "Default (resource:skins/default/select.json)"
        );
    }

    #[test]
    fn skin_candidate_display_keeps_user_origin_label() {
        let candidate = SkinCandidate {
            name: "Custom".to_string(),
            path: "data:skins/custom/play7.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        };

        assert_eq!(
            skin_candidate_display(&candidate, false),
            "[ユーザー] Custom (data:skins/custom/play7.luaskin)"
        );
    }

    #[test]
    fn bundled_skin_origin_is_hidden_for_development_or_portable_layout() {
        let app_paths = AppPaths::from_dirs(
            PathBuf::from("data"),
            PathBuf::from("data"),
            PathBuf::from("data/cache"),
            PathBuf::from("data/logs"),
        );
        let mut catalog = SkinCatalog::default();
        catalog.select.push(SkinCandidate {
            name: "Default".to_string(),
            path: "resource:skins/default/select.json".to_string(),
            origin: SkinCandidateOrigin::Bundled,
        });
        catalog.select.push(SkinCandidate {
            name: "Custom".to_string(),
            path: "data:skins/custom/select.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        });

        assert!(!show_bundled_skin_origin(&app_paths, &catalog));
    }

    #[test]
    fn bundled_skin_origin_is_shown_when_user_candidates_share_a_regular_layout() {
        let app_paths = AppPaths::from_dirs(
            PathBuf::from("resources"),
            PathBuf::from("profile-data"),
            PathBuf::from("profile-data/cache"),
            PathBuf::from("profile-data/logs"),
        );
        let mut catalog = SkinCatalog::default();
        catalog.select.push(SkinCandidate {
            name: "Default".to_string(),
            path: "resource:skins/default/select.json".to_string(),
            origin: SkinCandidateOrigin::Bundled,
        });
        catalog.select.push(SkinCandidate {
            name: "Custom".to_string(),
            path: "data:skins/custom/select.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        });

        assert!(show_bundled_skin_origin(&app_paths, &catalog));
    }

    #[test]
    fn bundled_skin_origin_is_hidden_when_catalog_has_no_user_candidates() {
        let app_paths = AppPaths::from_dirs(
            PathBuf::from("resources"),
            PathBuf::from("profile-data"),
            PathBuf::from("profile-data/cache"),
            PathBuf::from("profile-data/logs"),
        );
        let mut catalog = SkinCatalog::default();
        catalog.select.push(SkinCandidate {
            name: "Default".to_string(),
            path: "resource:skins/default/select.json".to_string(),
            origin: SkinCandidateOrigin::Bundled,
        });

        assert!(!show_bundled_skin_origin(&app_paths, &catalog));
    }

    #[test]
    fn sync_ir_provider_roles_keeps_only_primary_role() {
        let mut ir_config = IrConfig {
            primary_provider: "bmz-dev".to_string(),
            providers: vec![
                IrProviderConfig {
                    provider: "bmz".to_string(),
                    provider_key: "bmz".to_string(),
                    base_url: "https://bmz-player.hyrorre.workers.dev".to_string(),
                    enabled: true,
                    account_display_name: String::new(),
                    account_id: String::new(),
                    send_policy: IrSendPolicyConfig::default(),
                    role: IrProviderRoleConfig::Primary,
                    last_login_at: None,
                    last_success_at: None,
                },
                IrProviderConfig {
                    provider: "bmz".to_string(),
                    provider_key: "bmz-dev".to_string(),
                    base_url: "http://localhost:3000".to_string(),
                    enabled: true,
                    account_display_name: String::new(),
                    account_id: String::new(),
                    send_policy: IrSendPolicyConfig::default(),
                    role: IrProviderRoleConfig::SubmitOnly,
                    last_login_at: None,
                    last_success_at: None,
                },
            ],
            ..IrConfig::default()
        };

        assert!(sync_ir_provider_roles(&mut ir_config));
        assert_eq!(ir_config.providers[0].role, IrProviderRoleConfig::SubmitOnly);
        assert_eq!(ir_config.providers[1].role, IrProviderRoleConfig::Primary);

        ir_config.primary_provider.clear();
        assert!(sync_ir_provider_roles(&mut ir_config));
        assert_eq!(ir_config.providers[0].role, IrProviderRoleConfig::SubmitOnly);
        assert_eq!(ir_config.providers[1].role, IrProviderRoleConfig::SubmitOnly);
    }

    #[test]
    fn clamp_panel_layout_fits_high_dpi_1920x1080_logical_viewport() {
        // 1920x1080 物理ウィンドウ @ 2x → egui 論理 960x540 相当。
        let constrain = egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(928.0, 508.0));
        // egui 0.34 既定 style 付近の chrome 高さ (frame + title bar)。
        let chrome = egui::vec2(12.0, 58.0);
        let (default_inner, max_inner, pos) =
            clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

        let outer = default_inner + chrome;
        assert!(outer.x <= constrain.width() + 0.01);
        assert!(outer.y <= constrain.height() + 0.01);
        assert!(pos.x + outer.x <= constrain.max.x + 0.01);
        assert!(pos.y + outer.y <= constrain.max.y + 0.01);
        assert_eq!(pos, egui::pos2(16.0, 16.0));
        assert!(default_inner.y < 560.0);
        assert_eq!(max_inner, egui::vec2(916.0, 450.0));
    }

    #[test]
    fn clamp_panel_layout_keeps_preferred_size_on_large_viewport() {
        let constrain =
            egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(1888.0, 1048.0));
        let chrome = egui::vec2(12.0, 58.0);
        let (default_inner, max_inner, pos) =
            clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

        assert_eq!(default_inner, egui::vec2(440.0, 560.0));
        assert_eq!(max_inner, egui::vec2(1876.0, 990.0));
        // outer 高さ 618 のため y=480 では下端がはみ出す → 446 へクランプ。
        assert_eq!(pos, egui::pos2(16.0, 446.0));
    }

    #[test]
    fn apply_settings_list_action_moves_and_removes_entries() {
        let mut items = vec!["a", "b", "c"];

        apply_settings_list_action(&mut items, SettingsListAction::MoveDown(0));
        assert_eq!(items, vec!["b", "a", "c"]);

        apply_settings_list_action(&mut items, SettingsListAction::MoveUp(2));
        assert_eq!(items, vec!["b", "c", "a"]);

        apply_settings_list_action(&mut items, SettingsListAction::Remove(1));
        assert_eq!(items, vec!["b", "a"]);
    }

    #[test]
    fn apply_settings_list_action_moves_entry_to_index() {
        let mut items = vec!["a", "b", "c", "d"];

        apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 0, to: 2 });
        assert_eq!(items, vec!["b", "c", "a", "d"]);

        apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 3, to: 1 });
        assert_eq!(items, vec!["b", "d", "c", "a"]);
    }

    #[test]
    fn apply_settings_list_action_ignores_invalid_moves() {
        let mut items = vec!["a", "b"];

        apply_settings_list_action(&mut items, SettingsListAction::MoveUp(0));
        apply_settings_list_action(&mut items, SettingsListAction::MoveDown(1));
        apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 0, to: 2 });
        apply_settings_list_action(&mut items, SettingsListAction::MoveTo { from: 2, to: 0 });
        apply_settings_list_action(&mut items, SettingsListAction::Remove(2));

        assert_eq!(items, vec!["a", "b"]);
    }

    #[test]
    fn directory_open_targets_expose_only_app_path_roots() {
        let root = unique_test_dir("bmz-ui-directory-targets");
        let app_paths = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        let targets = directory_open_targets(&app_paths);
        let labels = targets.iter().map(|target| target.label).collect::<Vec<_>>();
        let paths = targets.iter().map(|target| target.path).collect::<Vec<_>>();

        assert_eq!(labels, vec!["resource_dir", "data_dir", "cache_dir", "logs_dir"]);
        assert_eq!(
            paths,
            vec![
                app_paths.resource_dir.as_path(),
                app_paths.data_dir.as_path(),
                app_paths.cache_dir.as_path(),
                app_paths.logs_dir.as_path(),
            ]
        );
    }

    #[test]
    fn combined_license_notice_uses_packaged_notice_files() {
        let root = unique_test_dir("bmz-ui-license-packaged");
        let resource_dir = root.join("resources");
        let license_dir = resource_dir.join("licenses");
        fs::create_dir_all(&license_dir).unwrap();
        fs::write(license_dir.join("third-party-notices.txt"), "packaged third party").unwrap();
        fs::write(license_dir.join("rust-dependency-licenses.txt"), "packaged rust report")
            .unwrap();
        let app_paths = AppPaths::from_dirs(
            resource_dir,
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

        assert!(notice.contains("packaged third party"));
        assert!(notice.contains("packaged rust report"));
        assert!(!notice.contains("The generated Rust dependency license report was not found."));
    }

    #[test]
    fn combined_license_notice_uses_local_rust_report_for_development() {
        let root = unique_test_dir("bmz-ui-license-local");
        let resource_dir = root.join("resources");
        let license_dir = resource_dir.join("licenses");
        fs::create_dir_all(&license_dir).unwrap();
        fs::write(license_dir.join("third-party-notices.txt"), "packaged third party").unwrap();
        fs::write(root.join("rust-dependency-licenses.txt"), "local rust report").unwrap();
        let app_paths = AppPaths::from_dirs(
            resource_dir,
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

        assert!(notice.contains("packaged third party"));
        assert!(notice.contains("local rust report"));
        assert!(!notice.contains("The generated Rust dependency license report was not found."));
    }

    #[test]
    fn combined_license_notice_explains_missing_rust_report() {
        let root = unique_test_dir("bmz-ui-license-missing");
        let app_paths = AppPaths::from_dirs(
            root.join("resources"),
            root.join("data"),
            root.join("cache"),
            root.join("logs"),
        );

        let notice = combined_license_notice_text_with_repo_root(&app_paths, &root);

        assert!(notice.contains("BMZ Player Third-Party Notices"));
        assert!(notice.contains("The generated Rust dependency license report was not found."));
        assert!(notice.contains("cargo-about generate --workspace --locked --fail"));
    }

    #[test]
    fn glob_candidates_lists_files_matching_simple_pattern() {
        let root = unique_test_dir("bmz-ui-glob");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/a.png"), []).unwrap();
        fs::write(root.join("parts/b.png"), []).unwrap();
        fs::write(root.join("parts/c.txt"), []).unwrap();

        let candidates = glob_candidates(&root, "parts/*.png");

        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&"parts/a.png".to_string()));
        assert!(candidates.contains(&"parts/b.png".to_string()));
    }

    #[test]
    fn glob_candidates_strips_beatoraja_filter_suffix() {
        let root = unique_test_dir("bmz-ui-glob");
        fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
        fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
        fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();

        let candidates = glob_candidates(&root, "parts/lanecover_lift/*.png|lanecover|");

        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&"parts/lanecover_lift/TYPE-M.png".to_string()));
        assert!(candidates.contains(&"parts/lanecover_lift/default.png".to_string()));
    }

    #[test]
    fn normalize_filepath_selection_maps_legacy_basename_to_relative_candidate() {
        let candidates =
            vec!["parts/gauge/default.png".to_string(), "parts/gauge/blue.png".to_string()];

        assert_eq!(
            normalize_filepath_selection("blue.png", &candidates).as_deref(),
            Some("parts/gauge/blue.png")
        );
        assert_eq!(normalize_filepath_selection("old/blue.png", &candidates), None);
    }

    #[test]
    fn property_default_uses_matching_def_name_or_first_item() {
        let prop = SkinPropertyDef {
            category: String::new(),
            name: "Notes".to_string(),
            item: vec![
                bmz_render::skin::SkinPropertyItemDef { name: "Light".to_string(), op: 1 },
                bmz_render::skin::SkinPropertyItemDef { name: "Dark".to_string(), op: 2 },
            ],
            def: "Dark".to_string(),
        };
        assert_eq!(property_default(&prop), "Dark");

        let prop = SkinPropertyDef { def: "Missing".to_string(), ..prop };
        assert_eq!(property_default(&prop), "Light");
    }

    #[test]
    fn filepath_default_matches_def_with_or_without_extension_case_insensitive() {
        let filepath = SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "default".to_string(),
        };
        let candidates = vec!["aaa.png".to_string(), "Default.PNG".to_string()];

        assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("Default.PNG"));

        let filepath = SkinFilepathDef { def: "missing".to_string(), ..filepath };
        assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("aaa.png"));
    }

    #[test]
    fn filepath_default_uses_random_sentinel_for_random_def() {
        // def="Random" は具体ファイルへ固定せず、ランダム番兵を既定にする。
        let filepath = SkinFilepathDef {
            category: String::new(),
            name: "BG".to_string(),
            path: "bg/*.mp4".to_string(),
            def: "Random".to_string(),
        };
        let candidates = vec!["bg/one.mp4".to_string(), "bg/two.mp4".to_string()];
        assert_eq!(
            filepath_default(&filepath, &candidates).as_deref(),
            Some(RANDOM_FILE_SELECTION)
        );
    }

    #[test]
    fn filepath_default_prefers_default_stem_when_def_missing() {
        let filepath = SkinFilepathDef {
            category: String::new(),
            name: "Note".to_string(),
            path: "notes/*.png".to_string(),
            def: String::new(),
        };
        let candidates = vec!["pastel.png".to_string(), "default.png".to_string()];

        assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("default.png"));
    }

    #[test]
    fn fill_missing_skin_defaults_keeps_saved_values_and_fills_new_items() {
        let root = unique_test_dir("bmz-ui-defaults");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/aaa.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let defs = SceneSkinDefs {
            property: vec![
                SkinPropertyDef {
                    category: String::new(),
                    name: "Lane".to_string(),
                    item: vec![
                        bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                        bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
                    ],
                    def: "On".to_string(),
                },
                SkinPropertyDef {
                    category: String::new(),
                    name: "Saved".to_string(),
                    item: vec![
                        bmz_render::skin::SkinPropertyItemDef { name: "A".to_string(), op: 0 },
                        bmz_render::skin::SkinPropertyItemDef { name: "B".to_string(), op: 1 },
                    ],
                    def: "A".to_string(),
                },
            ],
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".to_string(),
                path: "notes/*.png".to_string(),
                def: "default".to_string(),
            }],
            offset: Vec::new(),
        };
        let mut options = BTreeMap::from([("Saved".to_string(), "B".to_string())]);
        let mut files = BTreeMap::new();

        assert!(fill_missing_skin_defaults(&defs, Some(&root), &mut options, &mut files));

        assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
        assert_eq!(options.get("Saved").map(String::as_str), Some("B"));
        assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
    }

    #[test]
    fn fill_missing_skin_defaults_replaces_stale_option_selection() {
        let defs = SceneSkinDefs {
            property: vec![SkinPropertyDef {
                category: String::new(),
                name: "Graph".to_string(),
                item: vec![
                    bmz_render::skin::SkinPropertyItemDef { name: "AC".to_string(), op: 922 },
                    bmz_render::skin::SkinPropertyItemDef { name: "TYPE-M".to_string(), op: 923 },
                ],
                def: "AC".to_string(),
            }],
            filepath: Vec::new(),
            offset: Vec::new(),
        };
        let mut options = BTreeMap::from([("Graph".to_string(), "999".to_string())]);
        let mut files = BTreeMap::new();

        assert!(fill_missing_skin_defaults(&defs, None, &mut options, &mut files));

        assert_eq!(options.get("Graph").map(String::as_str), Some("AC"));
    }

    #[test]
    fn fill_missing_skin_defaults_keeps_stale_file_selection_like_beatoraja() {
        let root = unique_test_dir("bmz-ui-defaults-stale");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/aaa.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let defs = SceneSkinDefs {
            property: Vec::new(),
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".to_string(),
                path: "notes/*.png".to_string(),
                def: "default".to_string(),
            }],
            offset: Vec::new(),
        };
        let mut options = BTreeMap::new();
        let mut files = BTreeMap::from([("Notes".to_string(), "../old/default.png".to_string())]);

        assert!(!fill_missing_skin_defaults(&defs, Some(&root), &mut options, &mut files));

        assert_eq!(files.get("Notes").map(String::as_str), Some("../old/default.png"));
    }

    #[test]
    fn play_skin_defs_include_beatoraja_common_offsets() {
        let defs = SceneSkinDefs::from_play_document(None);

        let offsets: Vec<_> =
            defs.offset.iter().map(|offset| (offset.id, offset.name.as_str())).collect();
        assert!(offsets.contains(&(10, "All offset(%)")));
        assert!(offsets.contains(&(30, "Notes offset")));
        assert!(offsets.contains(&(32, "Judge offset")));
        assert!(offsets.contains(&(33, "Judge Detail offset")));
        assert!(offsets.contains(&(SKIN_OFFSET_BAR_LINE, "Bar Line offset")));
    }

    #[test]
    fn play_skin_defs_do_not_duplicate_existing_common_offset_ids() {
        let mut defs = SceneSkinDefs::default();
        defs.offset.push(SkinOffsetDef {
            category: "custom".to_string(),
            name: "Custom all".to_string(),
            id: 10,
            x: true,
            y: true,
            w: false,
            h: false,
            r: false,
            a: false,
        });

        defs.append_missing_beatoraja_play_offsets();

        assert_eq!(defs.offset.iter().filter(|offset| offset.id == 10).count(), 1);
        assert_eq!(defs.offset.len(), 5);
    }

    #[test]
    fn play_skin_defs_enable_bar_line_alpha_when_skin_def_disables_it() {
        let mut defs = SceneSkinDefs::default();
        defs.offset.push(SkinOffsetDef {
            category: "custom".to_string(),
            name: "Custom bar".to_string(),
            id: SKIN_OFFSET_BAR_LINE,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: false,
        });

        defs.append_missing_beatoraja_play_offsets();

        let bar_line = defs
            .offset
            .iter()
            .find(|offset| offset.id == SKIN_OFFSET_BAR_LINE)
            .expect("bar line offset def");
        assert!(bar_line.a);
    }

    #[test]
    fn reset_scene_skin_to_defaults_clears_saved_values_and_restores_factory_defaults() {
        let root = unique_test_dir("bmz-ui-reset-scene");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/aaa.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let defs = SceneSkinDefs {
            property: vec![SkinPropertyDef {
                category: String::new(),
                name: "Lane".to_string(),
                item: vec![
                    bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                    bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
                ],
                def: "On".to_string(),
            }],
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".to_string(),
                path: "notes/*.png".to_string(),
                def: "default".to_string(),
            }],
            offset: vec![SkinOffsetDef {
                category: "test".to_string(),
                name: "Judge".to_string(),
                id: 32,
                x: true,
                y: true,
                w: false,
                h: false,
                r: false,
                a: false,
            }],
        };
        let mut options = BTreeMap::from([("Lane".to_string(), "Off".to_string())]);
        let mut files = BTreeMap::from([("Notes".to_string(), "aaa.png".to_string())]);
        let mut offsets = vec![SkinOffsetConfig { id: 32, x: 99, ..Default::default() }];

        assert!(reset_scene_skin_to_defaults(
            &defs,
            Some(&root),
            &mut options,
            &mut files,
            &mut offsets
        ));

        assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
        assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
        assert!(offsets.is_empty());
    }

    #[test]
    fn skin_slot_history_restores_options_files_and_offsets_by_path() {
        let mut skin = SkinConfig {
            play7: "data/skins/ECFN/play/play7.luaskin".to_string(),
            offsets: vec![SkinOffsetConfig { id: 32, x: 12, ..Default::default() }],
            ..SkinConfig::default()
        };
        skin.play7_options.insert("Judge".to_string(), "On".to_string());
        skin.play7_files.insert("Notes".to_string(), "default.png".to_string());

        save_skin_slot_history(&mut skin, SkinSlot::Play7);
        skin.play7 = "data/skins/Starseeker/play/play7.luaskin".to_string();
        skin.play7_options.insert("Judge".to_string(), "Off".to_string());
        skin.play7_files.insert("Notes".to_string(), "other.png".to_string());
        skin.offsets = vec![SkinOffsetConfig { id: 32, x: -4, ..Default::default() }];
        save_skin_slot_history(&mut skin, SkinSlot::Play7);

        skin.play7 = "data/skins/ECFN/play/play7.luaskin".to_string();
        restore_skin_slot_history(&mut skin, SkinSlot::Play7);

        assert_eq!(skin.play7_options.get("Judge").map(String::as_str), Some("On"));
        assert_eq!(skin.play7_files.get("Notes").map(String::as_str), Some("default.png"));
        assert_eq!(skin.offsets, vec![SkinOffsetConfig { id: 32, x: 12, ..Default::default() }]);
    }

    #[test]
    fn skin_reload_diff_scopes_play_slot_without_select_reload() {
        let before = SkinConfig::default();
        let mut after = before.clone();
        after.play7_files.insert("Notes".to_string(), "blue.png".to_string());

        let request = skin_reload_request_from_diff(&before, &after);

        assert!(request.play7);
        assert!(!request.select);
        assert!(!request.play5);
        assert!(!request.result);
        assert!(request.any_reload());
    }

    #[test]
    fn skin_reload_diff_separates_result_and_course_result_slots() {
        let before = SkinConfig::default();
        let mut after = before.clone();
        after.course_result = "data/skins/course/result.luaskin".to_string();
        after.course_result_options.insert("Layout".to_string(), "Course".to_string());

        let request = skin_reload_request_from_diff(&before, &after);

        assert!(request.course_result);
        assert!(!request.result);

        let mut after = before.clone();
        after.result_files.insert("Background".to_string(), "normal.png".to_string());

        let request = skin_reload_request_from_diff(&before, &after);

        assert!(request.result);
        assert!(!request.course_result);
    }

    #[test]
    fn skin_reload_diff_marks_offsets_without_texture_reload() {
        let before = SkinConfig::default();
        let mut after = before.clone();
        after.offsets.push(SkinOffsetConfig { id: 32, x: 1, ..Default::default() });

        let request = skin_reload_request_from_diff(&before, &after);

        assert!(request.offsets);
        assert!(!request.any_reload());
        assert!(request.any());
    }
}
