use super::*;

pub(super) struct SystemSoundLoadWorkerResult {
    pub(super) generation: u64,
    pub(super) prepared: crate::system_sound_manager::PreparedSystemSoundSet,
}

pub(super) struct PendingSystemSoundLoad {
    pub(super) generation: u64,
    pub(super) started_at: Instant,
    pub(super) finished: Receiver<SystemSoundLoadWorkerResult>,
}

pub(super) struct PendingSongScan {
    pub(super) finished: Receiver<Result<ScanReport>>,
    pub(super) progress: Arc<AtomicU64>,
    pub(super) library_roots: Option<Vec<PathEntry>>,
}

pub(super) struct PendingReplayImport {
    pub(super) finished: Receiver<Result<ReplayImportReport>>,
    pub(super) done: Arc<AtomicU32>,
    pub(super) total: Arc<AtomicU32>,
    pub(super) cancel: Arc<AtomicBool>,
}

pub(super) enum UpdateCheckWorkerResult {
    Available(Box<UpdateCandidate>),
    UpToDate,
    Failed(anyhow::Error),
    Paused,
}

pub(super) struct PracticeChartDefaults {
    pub(super) property: crate::screens::practice::PracticeProperty,
    pub(super) title: String,
    pub(super) sha256: [u8; 32],
    pub(super) graph: std::sync::Arc<bmz_render::snapshot::ResultGraphSnapshot>,
    pub(super) max_end_time_ms: u32,
    pub(super) is_double: bool,
}

pub(super) struct PlayEndingTransition {
    pub(super) started_at: Instant,
    /// beatoraja の TIMER_MUSIC_END (timer 143) を開始した時刻。
    ///
    /// 最終ノーツ後の手動終了や Practice 設定画面からの退出は timer 2 だけを
    /// 開始するため、終了状態であってもここは `None` になる。
    pub(super) music_end_started_at: Option<Instant>,
    pub(super) fadeout_started_at: Option<Instant>,
    pub(super) finished: Option<FinishedPlaySession>,
    pub(super) failed: bool,
    pub(super) completion: PlayEndingCompletion,
    pub(super) full_combo_elapsed_at_finish_ms: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayEndingCompletion {
    Result,
    ViewerWait,
    ViewerExit,
    Select,
    PracticeConfig,
    PracticeLeave,
}

pub(super) fn viewer_exit_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: None,
        fadeout_started_at: Some(started_at),
        finished: None,
        failed: false,
        completion: PlayEndingCompletion::ViewerExit,
        full_combo_elapsed_at_finish_ms: None,
    }
}

pub(super) fn pre_play_abort_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: None,
        fadeout_started_at: Some(started_at),
        finished: None,
        failed: false,
        completion: PlayEndingCompletion::Select,
        full_combo_elapsed_at_finish_ms: None,
    }
}

pub(super) fn practice_natural_finish_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: Some(started_at),
        fadeout_started_at: None,
        finished: None,
        failed: false,
        completion: PlayEndingCompletion::PracticeConfig,
        full_combo_elapsed_at_finish_ms: None,
    }
}

pub(super) fn practice_requested_finish_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: None,
        fadeout_started_at: Some(started_at),
        finished: None,
        failed: false,
        completion: PlayEndingCompletion::PracticeConfig,
        full_combo_elapsed_at_finish_ms: None,
    }
}

pub(super) fn practice_failed_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: None,
        fadeout_started_at: None,
        finished: None,
        failed: true,
        completion: PlayEndingCompletion::PracticeConfig,
        full_combo_elapsed_at_finish_ms: None,
    }
}

pub(super) fn practice_leave_ending(started_at: Instant) -> PlayEndingTransition {
    PlayEndingTransition {
        started_at,
        music_end_started_at: None,
        fadeout_started_at: Some(started_at),
        finished: None,
        failed: false,
        completion: PlayEndingCompletion::PracticeLeave,
        full_combo_elapsed_at_finish_ms: None,
    }
}

/// リザルト画面終了フェードアウトの進行状態。
/// 通常はフェードアウト時間が経過したら、スキップ要求時は実アニメーションの
/// 最終フレームを1フレーム保持してから `action` を実行して画面を切り替える。
pub(super) struct ResultExit {
    pub(super) started_at: Instant,
    pub(super) action: ResultExitAction,
    pub(super) skip_requested: bool,
    pub(super) skip_final_frame_held: bool,
}

/// F10 で開始したフォルダ内 Autoplay の進行状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AutoplayFolderSession {
    pub(super) chart_ids: Vec<i64>,
    pub(super) next_index: usize,
}

/// リザルト画面を抜けたあとに実行する遷移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResultExitAction {
    /// 選曲画面へ戻る。
    Leave,
    /// 直前と同じ譜面を、指定した arrange でもう一度プレイする。
    Retry(ResultRetryMode),
    /// レーンキー (Key1-4 / Key5 / Key7) 押下で開始した遷移。
    /// フェードアウト終了時の Key5/Key7 押下状態で、retry(arrange) か
    /// 選曲へ戻るかを決める (beatoraja の REPLAY_SAME / REPLAY_DIFFERENT / OK 相当)。
    HeldLanes,
    /// コース（段位）リザルトから、コース全体を同配置で再プレイする。
    RetryCourseSameArrange,
    /// コース（段位）リザルトから、Key5/Key7 の押下状態で arrange を決める。
    HeldCourseLanes,
    /// コース曲間の中間リザルトを閉じて、コースの次の曲を開始する。
    /// リトライは発生させず次譜面へ進むだけ (beatoraja の MusicResult コース分岐相当)。
    AdvanceCourse,
    /// コース途中落ちの単曲リザルトを閉じて、コース最終リザルトへ進む。
    FinishCourse,
    /// フォルダ内 Autoplay の次の譜面を開始する。
    AdvanceAutoplayFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultRetryMode {
    SameArrange,
    DifferentArrange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetryPreloadKind {
    CachedChartWithFreshAudio,
    ReimportedChartWithFreshAudio,
}

pub(super) const SELECT_EXIT_HOLD_DURATION: Duration = Duration::from_millis(1_200);
pub(super) const FALLBACK_RESULT_SCENE_DURATION: Duration = Duration::from_secs(10);
/// プレイ中の Start ボタンを「2回連続押し」と判定する間隔上限。
pub(super) const PLAY_START_DOUBLE_PRESS_WINDOW: Duration = Duration::from_millis(400);
/// リザルト退出時にプレイ残響(draining_audio)を絞り切るまでの上限時間。
/// スキンの終了アニメーション (`fadeout`) が長くても (例: Starseeker は 3000ms)、
/// 音声はこの時間内でフェードし切る。スキンの fadeout がこれより短ければそちらを優先。
pub(super) const RESULT_EXIT_AUDIO_FADE: Duration = Duration::from_millis(1_500);
pub(super) const AUDIO_DIAGNOSTICS_LOG_INTERVAL: Duration = Duration::from_secs(1);
/// beatoraja PreviewMusicProcessor fades select BGM over 10 * 15ms steps.
/// beatoraja MusicSelector waits this long after a song-bar change before preview starts.
pub(super) const SELECT_PREVIEW_START_DELAY: Duration = Duration::from_millis(400);
/// レーンカバー / LIFT を上下キーで動かす際のステップ幅。
pub(super) const LANE_COVER_STEP: f32 = 0.001;
pub(super) const LANE_COVER_REPEAT_STEP: f32 = 0.01;
/// アナログスクラッチの tick が途切れたとみなし、端数バッファを捨てるまでの時間 (ms)。
/// beatoraja の `getAnalogDiffAndReset(i, 200)` の tolerance に相当。
pub(super) const SELECT_ANALOG_SCROLL_TOLERANCE_MS: u64 = 200;
pub(super) const SKIN_RELOAD_REDRAW_PROFILE_THRESHOLD: Duration = Duration::from_millis(8);
/// GPU texture の登録を伴う完了結果は、通常描画を止めないよう少量ずつ処理する。
/// BGA worker 側も同じ数で backpressure を掛け、先行した `Queue::write_texture` が
/// GPU queue を埋め続けないようにする。
pub(super) const MAX_PENDING_BGA_TEXTURE_UPLOADS: usize = 2;
pub(super) const MAX_BGA_TEXTURE_RESULTS_PER_REDRAW: usize = 2;
pub(super) const MAX_SKIN_UPLOADS_PER_REDRAW: usize = 1;

pub(super) fn bounded_gpu_upload_channel<T>(capacity: usize) -> (mpsc::SyncSender<T>, Receiver<T>) {
    debug_assert!(capacity > 0);
    mpsc::sync_channel(capacity)
}

pub(super) struct PendingSkinResult {
    pub(super) generation: u64,
    pub(super) path: PathBuf,
    pub(super) kind: SkinKind,
    pub(super) queued_at: Instant,
    pub(super) decode_started_at: Instant,
    pub(super) decode_finished_at: Instant,
    pub(super) result: Result<DecodedSkin>,
}

/// upload worker が GPU アップロードまで終えた結果を main へ返すメッセージ。
/// `UploadedSkin` 内の `PreparedTexture` は `Send` なのでスレッド間で渡せる。
/// main は受信後、テクスチャを差し込んで `SkinContext` を組むだけ (軽量)。
pub(super) struct PendingUploadResult {
    pub(super) generation: u64,
    pub(super) path: PathBuf,
    pub(super) kind: SkinKind,
    pub(super) queued_at: Instant,
    pub(super) decode_started_at: Instant,
    pub(super) decode_finished_at: Instant,
    pub(super) upload_started_at: Instant,
    pub(super) upload_finished_at: Instant,
    pub(super) uploaded: Result<UploadedSkin>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct SkinDrainStats {
    pub(super) received_count: usize,
    pub(super) applied_count: usize,
    pub(super) max_upload_wait_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DeferredBoot {
    Chart {
        chart_id: i64,
        replay_slot: Option<u8>,
        skip_decide: bool,
        score_save_disabled: bool,
        start_time_us: Option<i64>,
        bms_random_seed: Option<u64>,
    },
    Practice {
        chart_id: i64,
        start_time_ms: Option<u32>,
        end_time_ms: Option<u32>,
    },
    /// `--boot-replay-file <PATH>`: リプレイファイル直接指定の再生。
    ReplayFile {
        path: String,
    },
    CourseReplay {
        course_id: i64,
    },
    Course {
        course_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AppViewState {
    Select,
    Decide,
    Play,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppSceneKind {
    Select,
    Decide,
    Play,
    Result,
}
