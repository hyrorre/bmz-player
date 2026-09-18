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
        defs.append_play_common_offsets();
        defs
    }

    pub(super) fn is_empty(&self) -> bool {
        self.property.is_empty() && self.filepath.is_empty() && self.offset.is_empty()
    }

    pub(super) fn append_play_common_offsets(&mut self) {
        // beatoraja はスキン定義との ID 重複を除外せず、共通 offset を定義列の
        // 末尾へ追加する。runtime の ID map では後勝ちになる一方、設定値は名前で
        // 独立して保持される。
        for offset in beatoraja_play_common_offsets() {
            self.offset.push(offset);
        }

        // Bar Line offset は BMZ 独自拡張で、beatoraja の共通 offset とは分けて
        // beatoraja の有効範囲と衝突しない BMZ 専用 ID の定義を補完する。
        let bar_line = bmz_play_bar_line_offset();
        if let Some(existing) =
            self.offset.iter_mut().find(|existing| existing.id == SKIN_OFFSET_BAR_LINE)
        {
            existing.x = true;
            existing.y = true;
            existing.w = true;
            existing.h = true;
            existing.a = true;
        } else {
            self.offset.push(bar_line);
        }
    }
}

pub(super) fn beatoraja_play_common_offsets() -> [SkinOffsetDef; 4] {
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
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: true,
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
    ]
}

pub(super) fn bmz_play_bar_line_offset() -> SkinOffsetDef {
    SkinOffsetDef {
        category: "bmz".to_string(),
        name: "Bar Line offset".to_string(),
        id: SKIN_OFFSET_BAR_LINE,
        x: true,
        y: true,
        w: true,
        h: true,
        r: false,
        a: true,
    }
}

/// 選曲 / プレイ / リザルト各スキンの設定可能項目。
#[derive(Default)]
pub struct SkinConfigMeta {
    pub load_error: Option<(String, String)>,
    pub loading: bool,
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
    pub battle5: SceneSkinDefs,
    pub battle7: SceneSkinDefs,
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
    pub battle5: Vec<SkinCandidate>,
    pub battle7: Vec<SkinCandidate>,
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
    /// 右上 FPS オーバーレイと同じ、1 秒間の実測 FPS。
    pub current_fps: u32,
    /// 描画サーフェスの幅 (px)。
    pub width: u32,
    /// 描画サーフェスの高さ (px)。
    pub height: u32,
    /// GPU/OS capabilityのfallbackを反映した実効present mode。
    pub effective_present_mode: Option<&'static str>,
    /// swapchainに許可している最大in-flight frame数。
    pub maximum_frame_latency: Option<u32>,
}

/// `EguiLayer::run` の 1 フレーム入力。
pub struct EguiRunContext<'a, 'practice> {
    pub info: &'a DebugInfo,
    pub log_buffer: &'a LogBuffer,
    pub app_config: &'a mut AppConfig,
    pub profile_config: &'a mut ProfileConfig,
    pub random_trainer: &'a mut RandomTrainerState,
    pub skin_meta: &'a SkinConfigMeta,
    pub skin_catalog: &'a SkinCatalog,
    pub course_result: Option<&'a CourseResultSummary>,
    pub course_preview: Option<&'a SelectCourseRow>,
    pub course_editor: &'a CourseEditorData,
    pub select_course_builder: Option<SelectCourseBuilderData<'a>>,
    pub practice: Option<&'a mut PracticePanelContext<'practice>>,
    pub result_ir: Option<&'a mut crate::screens::result_ir::ResultIrState>,
    pub profile_root: &'a Path,
    pub app_paths: &'a AppPaths,
    /// 取得済み難易度表のメタデータ。設定済み URL の表示名解決に使う。
    pub difficulty_tables: &'a [DifficultyTableRecord],
    pub update_dialog: Option<UpdateDialog<'a>>,
    pub obs_connection_status: &'a crate::obs::ObsConnectionStatus,
    /// 現在の backend が認識しているゲームパッド一覧。未初期化時は空。
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
    /// eguiキー設定から要求された割り当て変更。
    pub key_config_action: Option<EguiKeyConfigAction>,
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
    pub replay_import_request: Option<ImportBeatorajaReplaysRequest>,
    pub cancel_replay_import: bool,
    /// 現在の設定で音声出力(cpal ストリーム)を開き直す要求。
    pub apply_audio_output: bool,
    pub check_for_update: bool,
    pub update_dialog_action: Option<UpdateDialogAction>,
    pub practice_start: bool,
    pub practice_leave: bool,
    pub course_editor_action: Option<CourseEditorAction>,
    pub select_course_builder_action: Option<SelectCourseBuilderAction>,
}

pub struct SelectCourseBuilderData<'a> {
    pub definition: &'a mut bmz_core::course::CourseDefinition,
    pub max_entries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectCourseBuilderAction {
    Remove(usize),
    Move { from: usize, to: usize },
    Undo,
    Save,
    Cancel,
}

#[derive(Debug, Clone, Default)]
pub struct CourseEditorData {
    pub courses: Vec<crate::storage::library_db::StoredCourse>,
    pub charts: Vec<CourseEditorChart>,
}

#[derive(Debug, Clone)]
pub struct CourseEditorChart {
    pub chart_id: i64,
    pub title: String,
    pub artist: String,
    pub play_level: String,
    pub mode: String,
    pub md5: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub enum CourseEditorAction {
    Save(bmz_core::course::CourseDefinition),
    Delete(i64),
    Export { path: PathBuf, definition: bmz_core::course::CourseDefinition },
    Import { path: PathBuf },
}

#[derive(Default)]
pub(super) struct CourseEditorUiState {
    pub(super) selected_course_id: Option<i64>,
    pub(super) draft: Option<bmz_core::course::CourseDefinition>,
    pub(super) search_query: String,
    pub(super) status: String,
    pub(super) error: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum UpdateDialog<'a> {
    Available(&'a UpdateCandidate),
    Downloading(&'a UpdateCandidate, &'a crate::update::DownloadProgress),
    Ready(&'a UpdateCandidate),
    Preparing(&'a UpdateCandidate),
    Error { message: &'a str, candidate: Option<&'a UpdateCandidate> },
    UpToDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDialogAction {
    Update,
    Install,
    Cancel,
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
    pub(super) ctx: egui::Context,
    pub(super) state: egui_winit::State,
    /// egui の未指定テキストで最優先する地域別 CJK coverage。
    pub(super) font_coverage: bmz_render::FontCoverage,
    /// OS フォントに依存しない CJK fallback の検索先。
    pub(super) font_search_paths: Vec<PathBuf>,
    /// メニュー全体の表示状態。F1 でトグルする。
    pub(super) visible: bool,
    /// デバッグ表示パネルの開閉状態。
    pub(super) show_debug: bool,
    /// 7K RANDOM 固定配置パネルの開閉状態。
    pub(super) show_random_trainer: bool,
    /// デバッグ表示内のログ最低表示レベル。
    pub(super) debug_log_filter: DebugLogFilter,
    /// デバッグ表示内のログを末尾へ追従するか。
    pub(super) debug_log_autoscroll: bool,
    /// 右上 FPS オーバーレイの表示状態。
    pub(super) show_fps: bool,
    /// 本体設定パネルの開閉状態。
    pub(super) show_settings: bool,
    /// プロファイル設定内のキー設定UIと入力待受状態。
    pub(super) key_config: EguiKeyConfigUiState,
    /// ローカルのコース/段位作成・編集パネル。
    pub(super) show_course_editor: bool,
    pub(super) course_editor: CourseEditorUiState,
    /// Lua skin の canonicalize をスキン設定 UI の毎フレームで繰り返さないためのキャッシュ。
    pub(super) skin_ui_path_cache: SkinUiPathCache,
    /// ライセンス表示パネルに出す結合済み notice text。
    pub(super) license_notice_text: Option<String>,
    pub(super) update_dialog_active: bool,
    /// 本体設定パネル: 曲フォルダ追加用の入力欄。
    pub(super) settings_new_root_path: String,
    /// 本体設定パネル: 曲フォルダ追加の直近エラー。
    pub(super) settings_add_root_error: String,
    pub(super) settings_new_table_url: String,
    pub(super) settings_add_table_error: String,
    pub(super) score_import_path: String,
    pub(super) score_import_kind: ScoreImportKind,
    pub(super) score_import_device_type: InputDeviceKind,
    pub(super) score_import_status: String,
    pub(super) score_import_error: String,
    pub(super) replay_import_path: String,
    pub(super) replay_import_device_type: InputDeviceKind,
    pub(super) replay_import_overwrite: bool,
    pub(super) replay_import_status: String,
    pub(super) replay_import_error: String,
    pub(super) replay_import_progress: Option<ReplayImportProgress>,
    /// 本体設定パネル: 出力デバイス選択用の列挙キャッシュ。
    pub(super) audio_device_picker: AudioDevicePickerState,
    /// 本体設定パネル: OBS scene list 取得状態。
    pub(super) obs_scene_picker: ObsScenePickerState,
    /// プロファイル設定パネル: IR ログインフォームの状態。
    pub(super) ir_login: IrLoginUiState,
    /// プロファイル設定パネル: IR device key 操作用の状態。
    pub(super) ir_device_key: IrDeviceKeyUiState,
    /// プロファイル設定パネル: profile 作成 / 複製フォームの状態。
    pub(super) profile_manager: ProfileManagerUiState,
    /// BMZ メニュー: OS のファイルマネージャでディレクトリを開いた直近結果。
    pub(super) directory_open_status: Option<DirectoryOpenStatus>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EguiKeyConfigSection {
    #[default]
    Common,
    Select,
    Play,
    Result,
    KeyMode(KeyMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EguiKeyConfigListenTarget {
    pub(super) key_mode: KeyMode,
    pub(super) target: KeyBindingTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EguiKeyConfigStatus {
    pub(super) message: String,
    pub(super) error: bool,
}

#[derive(Debug)]
pub(super) struct EguiKeyConfigUiState {
    pub(super) section: EguiKeyConfigSection,
    pub(super) slot: KeyBindingSlot,
    pub(super) listening: Option<EguiKeyConfigListenTarget>,
    pub(super) status: Option<EguiKeyConfigStatus>,
}

impl Default for EguiKeyConfigUiState {
    fn default() -> Self {
        Self {
            section: EguiKeyConfigSection::Common,
            slot: KeyBindingSlot::KeyboardPrimary,
            listening: None,
            status: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EguiKeyConfigAction {
    Bind { key_mode: KeyMode, target: KeyBindingTarget, control: String },
    Clear { key_mode: KeyMode, target: KeyBindingTarget },
    ToggleEightKeyHispeed { entry_id: SettingsEntryId },
    RestoreDefaults { section: EguiKeyConfigSection, slot: KeyBindingSlot },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EguiKeyConfigInput {
    NotHandled,
    Consumed,
    Action(EguiKeyConfigAction),
}

#[derive(Debug, Clone)]
pub(super) struct DirectoryOpenStatus {
    pub(super) label: &'static str,
    pub(super) path: PathBuf,
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DirectoryOpenTarget<'a> {
    pub(super) label: &'static str,
    pub(super) path: &'a Path,
}
use super::*;
