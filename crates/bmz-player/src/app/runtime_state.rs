use super::*;

#[derive(Default)]
pub(super) struct SelectScoreRefreshState {
    dirty: bool,
}

impl SelectScoreRefreshState {
    pub(super) fn mark_stored_result(&mut self, score_history_id: i64) {
        self.dirty |= score_history_id > 0;
    }

    pub(super) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

pub(super) struct SelectRuntimeState {
    /// 選曲画面の F10 で開始したフォルダ内 Autoplay。
    pub(super) autoplay_folder: Option<AutoplayFolderSession>,
    /// 選曲カーソル譜面の IR ランキングキャッシュ。
    pub(super) select_ir: crate::screens::select_ir::SelectIrRanking,
    /// profile 全体の player statistics。Select / Result skin の NUMBER_TOTAL* 系に渡す。
    pub(super) player_stats: PlayerStatsSnapshot,
    /// Result 保存後、選曲リストが score DB を再取得するまで保持する。
    /// Retry で `finished_play` が破棄されても更新必要性を失わないための状態。
    pub(super) score_refresh: SelectScoreRefreshState,
    pub(super) select_items: Vec<SelectItem>,
    pub(super) select_distribution_cache: RefCell<HashMap<i64, Vec<ChartDistributionSecond>>>,
    pub(super) difficulty_tables: Vec<DifficultyTableRecord>,
    pub(super) table_breadcrumb_cache: RefCell<HashMap<String, TableBreadcrumb>>,
    pub(super) select_folder_summaries: SelectFolderSummaryRuntime,
    pub(super) folder_stack: Vec<String>,
    /// `folder_stack` の各階層に入る直前の `selected_index`。
    /// フォルダから出た時にカーソル位置を復元するために使う。長さは `folder_stack` と一致。
    pub(super) selected_index_stack: Vec<usize>,
    pub(super) selected_index: usize,
    pub(super) arrange_option: ArrangeOption,
    pub(super) arrange_option_2p: ArrangeOption,
    /// Endless Dream 互換の7K RANDOM固定配置。profileへは保存しない。
    pub(super) random_trainer: RandomTrainerState,
    pub(super) target_option: TargetOption,
    pub(super) gauge_option: GaugeTypeConfig,
    pub(super) gauge_auto_shift_option: GaugeAutoShiftConfig,
    pub(super) bottom_shiftable_gauge_option: BottomShiftableGaugeConfig,
    pub(super) double_option: DoubleOption,
    pub(super) hs_fix_option: HsFixOption,
    pub(super) session_mode: SessionMode,
    pub(super) select_mode_filter: SelectModeFilter,
    pub(super) select_sort: SelectSort,
    pub(super) select_keys: SelectKeyBindings,
    pub(super) select_bar_scroll_direction: i32,
    pub(super) select_bar_scroll_duration: Duration,
    pub(super) select_hold_move: Option<SelectMove>,
    pub(super) select_hold_started_at: Option<Instant>,
    pub(super) select_hold_last_trigger_at: Option<Instant>,
    pub(super) select_hold_control: Option<String>,
    pub(super) select_analog_scroll_buffer: i32,
    pub(super) select_analog_last_tick_at: Option<Instant>,
    /// キーコンフィグ確定/キャンセル直後、スクラッチが止まるまでアナログスクロールを抑止する。
    pub(super) select_analog_suppress_until_idle: bool,
    pub(super) select_scene_started_at: Instant,
    pub(super) select_bar_started_at: Instant,
    pub(super) option_panel_started_at: Instant,
    pub(super) option_panel_off_started_at: [Option<Instant>; 6],
    pub(super) select_option_panel: u8,
    /// 選曲画面でESCを長押し中の開始時刻。離されたり画面を抜けると None になる。
    pub(super) select_exit_hold_started_at: Option<Instant>,
    /// 選曲画面のメタ画像・試聴音源のキャッシュと非同期ロード状態。
    pub(super) select_assets: SelectAssetRuntime,
    /// 設定画面で編集中の項目。`None` なら一覧操作モード。
    pub(super) settings_edit: Option<SettingsEditSession>,
    /// キー設定の待ち受け状態。
    pub(super) key_config_edit: Option<KeyConfigEditSession>,
    /// 選曲画面の検索文字列、IME、cursor、履歴、feedback状態。
    pub(super) search: SelectSearchRuntime,
    /// 直近のマウスカーソル位置。select skin のクリック hit-test に使う。
    pub(super) last_cursor_position: Option<PhysicalPosition<f64>>,
    /// ドラッグ中の select skin slider type。
    pub(super) select_slider_dragging_type: Option<i32>,
}

pub(super) struct PlayRuntimeState {
    pub(super) active_play: Option<StartedInputPlaySession>,
    /// コースプレイ中のセッション。単曲プレイ時は None。
    pub(super) active_course: Option<ActiveCourseSession>,
    pub(super) last_play_snapshot: Option<RenderSnapshot>,
    pub(super) pending_decide: Option<DecideTransition>,
    pub(super) pending_play_start: Option<PendingPlayStart>,
    pub(super) pending_play_preload: Option<PendingPlayPreload>,
    /// 中間リザルト表示中に開始した次コース譜面の先読み要求。
    pub(super) pending_course_stage_launch: Option<PendingCourseStageLaunch>,
    /// Decide中に先頭譜面の変換結果を再利用して計算する、厳密なコース全体metrics。
    pub(super) pending_course_metrics: Option<PendingCourseMetrics>,
    /// Decide 演出中に preload worker から受け取った結果を退避し、
    /// `start_chart_with_options` で再利用するためのバッファ。
    /// 既に裏で完了している譜面/音源ロードを main で再度同期実行するのを避ける。
    pub(super) preloaded_play_session: Option<PreloadedInputPlaySession>,
    pub(super) play_preload_generation: u64,
    pub(super) course_metrics_generation: u64,
    /// 同曲リトライ用に残すキー音 / 静止画 BGA / 動画デコーダ。
    pub(super) play_media_cache: Option<PlayMediaCache>,
    pub(super) play_ending: Option<PlayEndingTransition>,
    pub(super) last_started_chart_id: Option<i64>,
    /// プレイ開始時点の難易度表テキスト (beatoraja TEXT_TABLE1..3)。
    pub(super) play_table_text_primary: String,
    pub(super) play_table_text_secondary: String,
    pub(super) play_table_text_fallback: String,
    /// 現在のプレイ譜面の KEY MODE と device-aware lane binding。
    /// 選曲用 binding と分離し、10K/14K の同名 gamepad control も側別に解決する。
    pub(super) play_option_input: Option<PlayOptionInput>,
    pub(super) play_analog_scroll_buffer: i32,
    pub(super) play_analog_last_tick_at: Option<Instant>,
    pub(super) play_scene_started_at: Instant,
    pub(super) play_ready_sound_started_at: Option<Instant>,
    /// READY 前に E1/E2 が最後に押されていた時刻。
    /// beatoraja と同様、解放後 1 秒間は PRELOAD を維持する。
    pub(super) play_ready_last_control_hold_at: Option<Instant>,
    pub(super) decide_sound_stopped_for_chart_start: bool,
    pub(super) bga_preload: BgaPreloadRuntime,
    /// プレイ開始時にロードした `#STAGEFILE` のキャッシュキー。
    /// Result でも同じ runtime image 100 を使うため、Result 終了まで保持する。
    pub(super) play_stagefile_source: Option<String>,
    pub(super) play_stagefile_loaded: bool,
    pub(super) play_stagefile_size: Option<SkinImageSize>,
    /// プレイ `#BACKBMP` のロード済みキャッシュキー。
    pub(super) play_backbmp_source: Option<String>,
    pub(super) play_backbmp_loaded: bool,
    /// プレイ中の Start キー直近の押下時刻。連続押し判定で使用。
    pub(super) last_play_start_press_at: Option<Instant>,
    /// Decide 中の E1 押下状態。E1+E2 キャンセルに使う。
    pub(super) decide_e1_held: bool,
    /// プレイ開始待ち/プレイ中の E1 押下状態。READY 前の緑数字表示にも使う。
    pub(super) play_e1_held: bool,
    /// プレイ中の E2 押下状態。E2+E3 即終了 / E1+E2 長押し終了に使う。
    pub(super) play_e2_held: bool,
    /// プレイ中の E3 押下状態。E2+E3 即終了に使う。
    pub(super) play_e3_held: bool,
    /// E1+E2 が押され続けている開始時刻。beatoraja 既定 1000ms で途中終了。
    pub(super) play_exit_hold_started_at: Option<Instant>,
    /// CLI から入ったプラクティスセッション。選曲 UI からは未対応。
    pub(super) practice_session: Option<PracticeSession>,
    /// 次の `RunningPlaySession::start` で使う chart zero（区間先頭の 1 秒前）。
    pub(super) practice_chart_zero_time: Option<TimeUs>,
}

pub(super) struct ResultRuntimeState {
    /// 最終曲の判定確定時に保存・IR enqueue まで済ませたコース結果。
    /// 単曲リザルトを表示し終えるまでは `finished_course` に昇格させず、従来の
    /// 「最終曲リザルト → コースリザルト」の表示順を維持する。
    pub(super) prepared_course_finish: Option<PreparedCourseFinish>,
    /// コース全体完了時のリザルト。リザルト画面から抜けるまで保持する。
    pub(super) finished_course: Option<CourseResultSummary>,
    /// `finished_course` から Result skin 用に集約した結果。
    ///
    /// コースの graph は全ステージ分を連結するため構築コストが高い。コース完了時に
    /// 一度だけ生成し、リザルト表示中はこの値を参照する。
    pub(super) finished_course_skin_summary: Option<ResultSummary>,
    /// 完了したコースの canonical hash。IR ranking の起動や replay slot 保存で
    /// course 定義を DB から再走査しないため、identity 解決時に保持する。
    pub(super) finished_course_hash: Option<String>,
    /// 完了したcourseのrianIR/beatoraja connector互換hash。
    pub(super) finished_course_rian_hash_v1: Option<String>,
    /// IR 無効時も course ranking task の起動判定を毎フレーム繰り返さないための印。
    pub(super) finished_course_ir_attempted: bool,
    pub(super) finished_play: Option<FinishedPlaySession>,
    /// Result skin の favorite_chart (image ref 90) に渡す現在譜面の状態。
    /// BMZ は beatoraja の invisible を持たないため false/true の2状態だけを使う。
    pub(super) result_favorite_chart: bool,
    /// リザルト画面の IR 送信・ランキング表示状態。
    /// 通常プレイでは play ending 中に早期起動し、Result 画面まで保持する。
    pub(super) result_ir: Option<crate::screens::result_ir::ResultIrState>,
    /// 直近に開始したプレイのsession mode。Play / Resultの常時表示に使う。
    pub(super) last_play_session_mode: SessionMode,
    pub(super) result_scene_started_at: Instant,
    /// 現在インストール済みの Result skin が宣言した BGM / SE ランタイム。
    pub(super) result_skin_audio: Option<crate::skin_audio::SkinAudioRuntime>,
    /// リザルト画面終了アニメーションの進行状態。
    /// Some のあいだは終了フェードアウト中で、入力は受け付けない。
    pub(super) result_exit: Option<ResultExit>,
    /// リザルト画面で Key5 が現在押されているか。
    /// 終了アニメーション終了時に retry arrange を決める判定に使う。
    pub(super) result_key5_held: bool,
    /// リザルト画面で Key7 が現在押されているか。
    pub(super) result_key7_held: bool,
    pub(super) result_gauge_graph_type: i32,
    /// Lua Result スキンの展開パネル (0=非表示、1=IR、2=グラフ)。
    pub(super) result_panel: i32,
    /// Result IR のキー長押し・アナログスクラッチ用入力状態。
    pub(super) result_ir_scroll: ResultIrScrollRuntime,
}

pub(super) struct PreparedCourseFinish {
    pub(super) course_id: i64,
    pub(super) course_result: CourseResultSummary,
    pub(super) course_hash: Option<String>,
    pub(super) rian_course_hash_v1: Option<String>,
    pub(super) last_finished: Option<FinishedPlaySession>,
}

#[derive(Default)]
pub(super) struct ResultIrScrollRuntime {
    pub(super) hold_rows: i32,
    pub(super) hold_started_at: Option<Instant>,
    pub(super) hold_last_trigger_at: Option<Instant>,
    pub(super) hold_control: Option<String>,
    pub(super) analog_buffer: i32,
    pub(super) analog_last_tick_at: Option<Instant>,
}

pub(super) struct AppJobs {
    /// 通常表・rianIR表の取得channel、queue、progress、世代状態。
    pub(super) table_fetch: TableFetchRuntime,
    pub(super) pending_song_scan: Option<PendingSongScan>,
    /// Select外で要求されたscanを開始せず、次のSelectまでFIFOで保持する。
    pub(super) queued_song_scans: VecDeque<(Vec<PathEntry>, bool, String)>,
    pub(super) pending_chart_download: Option<Receiver<Result<ChartDownloadResult>>>,
    pub(super) queued_download_scan: Option<(PathBuf, String)>,
    pub(super) song_scan_progress: Option<ScanProgress>,
    pub(super) pending_update_check: Option<Receiver<UpdateCheckWorkerResult>>,
    pub(super) pending_update_check_reports_up_to_date: bool,
    /// 自動・手動update checkをSelectまで保留する。手動要求を優先して結果を表示する。
    pub(super) queued_update_check: Option<(&'static str, bool)>,
    pub(super) pending_update_download: Option<Receiver<Result<DownloadedUpdate>>>,
    pub(super) update_prompt: Option<UpdatePrompt>,
    pub(super) update_dismissed_session_version: Option<String>,
    /// worker群へ、Selectかつ直接起動待ちでない期間だけ実行許可を通知する。
    pub(super) maintenance_select_tx: tokio::sync::watch::Sender<bool>,
}

pub(super) struct IntegrationRuntimeState {
    pub(super) obs_controller: Option<crate::obs::ObsController>,
    pub(super) applied_obs_config: ObsConfig,
    pub(super) play_overlay_controller: crate::play_overlay::PlayOverlayController,
    pub(super) play_overlay_state: crate::play_overlay::PlayOverlayState,
    pub(super) applied_play_overlay_config: crate::play_overlay::PlayOverlayServerConfig,
    pub(super) exit_configs_saved: bool,
    pub(super) last_scene_kind: Option<AppSceneKind>,
    pub(super) discord_presence: Option<DiscordPresenceHandle>,
    pub(super) discord_presence_config: Option<DiscordPresenceConfig>,
    pub(super) last_obs_event_key: Option<crate::obs::ObsEventKey>,
}

pub(super) struct SmokeRuntime {
    pub(super) smoke_exit_after_frames: Option<u32>,
    pub(super) smoke_exit_after_play_frames: Option<u32>,
    pub(super) smoke_exit_after_result_frames: Option<u32>,
    pub(super) smoke_exit_on_result: bool,
    pub(super) smoke_screenshot_path: Option<PathBuf>,
    /// 左上へ出す一時メッセージ。
    pub(super) left_overlay_toast: Option<LeftOverlayToast>,
    pub(super) rendered_frames: u32,
    pub(super) rendered_play_frames: u32,
    pub(super) rendered_result_frames: u32,
    pub(super) app_started_at: Instant,
}

pub(super) struct SkinRuntimeState {
    pub(super) skin_catalog: SkinCatalog,
    pub(super) skin_defs_cache: BTreeMap<String, SceneSkinDefs>,
    pub(super) default_skin_manifest: Option<SkinManifest>,
    /// skin decode/upload channel、共有cache、pending世代をまとめた非同期pipeline。
    pub(super) skin_pipeline: SkinPipelineRuntime,
    pub(super) skin_video_sources: HashMap<SkinKind, Vec<ActiveSkinVideoSource>>,
    pub(super) pending_skin_render_probe: Option<PendingSkinRenderProbe>,
    /// 直近 install をリクエストしたプレイスキンの key_mode と設定 fingerprint。
    /// 同じ mode かつ同じ path/options/files なら再 decode をスキップする。
    pub(super) last_play_skin_signature: Option<PlaySkinSignature>,
    /// 直近 install をリクエストした Result context の用途と設定 fingerprint。
    /// Renderer の Result context は 1 本だけなので、通常/コース最終結果で差し替える。
    pub(super) last_result_skin_signature: Option<ResultSkinSignature>,
}

pub(super) struct AppAudioRuntimeState {
    /// プレイ終了でリザルトへ移った後、曲の余韻を鳴らし切るために保持する音声出力。
    /// ドレインが完了するか、選曲復帰・次プレイ開始で解放される。
    pub(super) draining_audio: Option<AppAudioOutput>,
    pub(super) audio_runtime: Option<AudioRuntime>,
    pub(super) audio_output_open_attempted: bool,
    pub(super) audio_diagnostics_last_log_at: Instant,
    pub(super) audio_diagnostics_last: Option<AudioOutputDiagnostics>,
    pub(super) input_diagnostics_last_sequence: u64,
    /// システム SE / BGM を再生する cpal ストリーム。
    /// 開けない環境では `None` で、システム音はサイレント。
    pub(super) system_audio: Option<crate::audio::SystemAudio>,
    /// 起動時にスキャンしたシステム BGM / SE セット候補。
    /// 選曲画面へ戻る際の再抽選では再スキャンせず、この一覧を使う。
    pub(super) system_sound_catalog: crate::system_sound::SoundSetCatalog,
    /// `system_audio` 上にデコード済みサンプルを乗せて再生・停止する facade。
    /// `system_audio` が `None` の場合や、サウンドセット未指定の場合も `Some` で
    /// 構築されるが id_map が空なので各 play/stop は no-op になる。
    pub(super) system_sound: Option<crate::system_sound_manager::SystemSoundManager>,
}

pub(super) struct UiRuntimeState {
    /// 本体設定 / スキン設定 / デバッグ表示用の egui レイヤ。
    /// ウィンドウ生成時に初期化される。
    pub(super) egui: Option<EguiLayer>,
    /// デバッグ表示へ渡す bounded tracing ログバッファ。
    #[allow(dead_code)]
    pub(super) log_buffer: LogBuffer,
    /// 現在ウィンドウへ適用済みのウィンドウモード。
    /// config 側との差分検出でライブ反映の要否を判定する。
    pub(super) applied_window_mode: WindowMode,
    /// キーボード backend 変更後、次の about_to_wait で winit の
    /// DeviceEvent 購読を更新する。
    pub(super) device_events_reconfigure_pending: bool,
    /// ウィンドウがフォーカスを持っているか。フレームレート上限の切替に使う。
    pub(super) focused: bool,
    /// 直近のマウスカーソル移動 / 操作時刻。カーソル非表示判定に使う。
    pub(super) last_cursor_action_at: Instant,
    /// 現在マウスカーソルが表示されているか。
    pub(super) cursor_visible: bool,
}
