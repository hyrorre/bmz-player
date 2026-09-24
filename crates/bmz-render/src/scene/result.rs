use super::*;

pub const IR_RANKING_ENTRY_SLOTS: usize = 10;
pub const IR_RANKING_NAME_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultIrRankingName {
    bytes: [u8; IR_RANKING_NAME_BYTES],
    len: u8,
}

impl Default for ResultIrRankingName {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ResultIrRankingName {
    pub const EMPTY: Self = Self { bytes: [0; IR_RANKING_NAME_BYTES], len: 0 };

    pub fn from_display_name(name: &str) -> Self {
        let mut len = name.len().min(IR_RANKING_NAME_BYTES);
        while !name.is_char_boundary(len) {
            len -= 1;
        }
        let mut bytes = [0; IR_RANKING_NAME_BYTES];
        bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self { bytes, len: len as u8 }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultIrRankingEntrySnapshot {
    pub rank: Option<i64>,
    pub ex_score: Option<i64>,
    /// image/index property 390..399 で使う beatoraja clear type index。
    pub clear_index: Option<i64>,
    pub player_name: ResultIrRankingName,
}

impl ResultIrRankingEntrySnapshot {
    pub const EMPTY: Self = Self {
        rank: None,
        ex_score: None,
        clear_index: None,
        player_name: ResultIrRankingName::EMPTY,
    };
}

/// リザルト画面の IR ランキング表示状態。
///
/// beatoraja の `NUMBER_IR_*` / `OPTION_IR_*` skin property に対応する。
/// 接続状態は選択中譜面のランキング取得状態とは独立して保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultIrSnapshot {
    /// primary IR が設定済みか。OPTION_OFFLINE / OPTION_ONLINE (50/51) に使う。
    pub online: bool,
    pub state: ResultIrState,
    /// STRING_IR_NAME=1020。primary IRの表示名。
    pub provider_name: ResultIrRankingName,
    /// STRING_IR_USER_NAME=1021。自分のランキング行判定にも使う。
    pub user_name: ResultIrRankingName,
    /// IR connect/send/access begin timer elapsed ms (TIMER_IR_CONNECT_BEGIN=172).
    pub connect_begin_ms: Option<i32>,
    /// IR connect/send/access success timer elapsed ms (TIMER_IR_CONNECT_SUCCESS=173).
    pub connect_success_ms: Option<i32>,
    /// IR connect/send/access fail timer elapsed ms (TIMER_IR_CONNECT_FAIL=174).
    pub connect_fail_ms: Option<i32>,
    /// 全体ランキングでの自分の順位 (NUMBER_IR_RANK=179)。
    pub rank: Option<i64>,
    /// ランキング対象の総プレイヤー数 (NUMBER_IR_TOTALPLAYER=180/200)。
    pub total_player: Option<i64>,
    /// 全プレイヤー中のクリア率 % (NUMBER_IR_CLEARRATE=181)。
    pub clear_rate: Option<i64>,
    /// Exact population by beatoraja clear index (0..=10), when the complete
    /// ranking is available. A page of top scores must not be treated as a population.
    pub clear_counts: Option<[u32; 11]>,
    /// 更新前の順位 (NUMBER_IR_PREVRANK=182)。未対応なら None。
    pub previous_rank: Option<i64>,
    /// BMZ Result IR scope (`0=Ranking`, `1=Rival`) currently supplied to the skin.
    pub scope: ResultIrScope,
    /// Whether the global Ranking scope can be selected for this Result.
    pub global_scope_supported: bool,
    /// Whether the Self-and-Rivals scope can be selected for this Result.
    pub rival_scope_supported: bool,
    /// IRランキングの先頭表示行と最大スクロール位置。rate type 8の算出に使う。
    pub scroll_offset: usize,
    pub scroll_max: usize,
    /// 上位ランキング行 (STRING_RANKINGNAME1..10 / NUMBER_RANKING*_EXSCORE/INDEX)。
    pub entries: [ResultIrRankingEntrySnapshot; IR_RANKING_ENTRY_SLOTS],
}

impl ResultIrSnapshot {
    pub const EMPTY: Self = Self {
        online: false,
        state: ResultIrState::Offline,
        provider_name: ResultIrRankingName::EMPTY,
        user_name: ResultIrRankingName::EMPTY,
        connect_begin_ms: None,
        connect_success_ms: None,
        connect_fail_ms: None,
        rank: None,
        total_player: None,
        clear_rate: None,
        clear_counts: None,
        previous_rank: None,
        scope: ResultIrScope::Global,
        global_scope_supported: false,
        rival_scope_supported: false,
        scroll_offset: 0,
        scroll_max: 0,
        entries: [ResultIrRankingEntrySnapshot::EMPTY; IR_RANKING_ENTRY_SLOTS],
    };
}

/// BMZ Result IR scope exposed to compatible skins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResultIrScope {
    #[default]
    Global,
    Rival,
}

impl ResultIrScope {
    pub const fn index(self) -> i64 {
        match self {
            Self::Global => 0,
            Self::Rival => 1,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Global => "RANKING",
            Self::Rival => "RIVAL",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResultIrState {
    /// IR 未設定、または現在行にランキング対象がない。
    #[default]
    Offline,
    /// 送信・ランキング取得中 (OPTION_IR_LOADING=601)。
    Loading,
    /// 選曲カーソルがランキング取得デバウンス中 (OPTION_IR_WAITING=606)。
    Waiting,
    /// ランキング取得済み (OPTION_IR_LOADED=602)。
    Loaded,
    /// 取得失敗 (OPTION_IR_FAILED=604)。
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSnapshot {
    /// beatoraja STRING_PLAYER (2) に渡す現在プロフィール名。
    pub player_name: String,
    /// beatoraja STRING_RIVAL/STRING_TARGET (1/3)。
    pub target_name: String,
    /// プレイ開始時のターゲット設定ID (text ref 3)。
    pub target: String,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    pub skin_input: SkinLogicalInputSnapshot,
    pub skin_attempt: SkinAttemptState,
    /// 現在のリザルトスキンスロットに設定された destination offset。
    pub skin_offsets: SkinOffsetValues,
    /// Result skin mouse position in normalized skin-canvas coordinates.
    /// The origin is the top-left corner.
    pub mouse_position: Option<(f32, f32)>,
    /// beatoraja image/index ref 342。
    pub hispeed_auto_adjust: bool,
    pub clear_type: ClearType,
    /// OPTION_RESULT_CLEAR/FAILED (90/91) に渡す実際の成否。
    ///
    /// コース曲間リザルトでは clear lamp 表示用の `clear_type` を NoPlay に丸める一方、
    /// 背景や CLEAR/FAILED 演出は実プレイ結果に合わせるため分けて持つ。
    pub result_failed: bool,
    /// BMZ extension: result skin でも OPTION_AUTOPLAYON/OFF (33/32) を公開する。
    pub autoplay: bool,
    pub arrange: String,
    pub arrange_2p: String,
    pub double_option: String,
    pub lane_shuffle_pattern: Vec<u8>,
    pub ex_score: u32,
    pub ex_score_rate: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
    pub gauge_value: f32,
    pub gauge_type: i32,
    pub total_notes: u32,
    pub duration_ms: i32,
    /// NUMBER_DURATION に渡す設定済みノーツ表示時間 ms。
    /// NUMBER_DURATION_GREEN は描画state構築時にこの値から導出する。
    pub note_display_duration_ms: Option<i32>,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub main_bpm: f32,
    pub total_gauge: f32,
    pub judge_rank: Option<i32>,
    pub key_mode: KeyMode,
    /// 実効譜面にLNが含まれるか (OPTION_NO_LN/LN=172/173)。
    pub has_long_notes: bool,
    /// 実効LN種別のimageset index (0=LN, 1=CN, 2=HCN)。
    pub ln_mode_index: usize,
    /// BMZ extension: frozen scoring rule mode index for this attempt.
    pub rule_mode_index: usize,
    /// BMZ extension: normalized LN policy index stored in this attempt's score key.
    pub ln_score_policy_index: Option<usize>,
    pub result_gauge_graph_type: i32,
    /// Lua Result スキンの展開パネル (0=非表示、1=IR、2=グラフ)。
    pub result_panel: i32,
    /// 現在の譜面が favorite chart か。BMZ は invisible を持たないため2状態。
    pub favorite_chart: bool,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    /// 今回のリザルトがスコア保存対象か。
    pub score_save_enabled: bool,
    pub assist_flags: [bool; 7],
    pub assist_extra_note_depth: u8,
    pub assist_mine_mode: i64,
    pub assist_scroll_mode: i64,
    pub assist_long_note_mode: i64,
    pub score_history_id: i64,
    pub replay_saved: bool,
    pub replay_slots: [bool; 4],
    pub saved_replay_slots: [bool; 4],
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub target_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub best_bp: Option<u32>,
    pub target_bp: Option<u32>,
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_clear_type: Option<ClearType>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_bp: Option<u32>,
    pub target_clear_type: Option<ClearType>,
    /// リザルト画面を開いてからの経過時間。
    /// destination の timer/loop/keyframe アニメーション、image cycle に使われる。
    pub elapsed_time: TimeUs,
    /// リザルト画面終了フェードアウトの経過時間 (TIMER_FADEOUT=2)。
    /// None なら終了処理に入っていない。Some のあいだは `timer: 2` の
    /// destination が描画され、終了アニメーションが進行する。
    pub fadeout_elapsed: Option<TimeUs>,
    /// 曲名 (text ref 10/12 で表示)。
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub table_text_primary: String,
    pub table_text_secondary: String,
    pub table_text_fallback: String,
    /// 直前にプレイした譜面の `#STAGEFILE` テクスチャがロード済みなら true。
    pub stagefile_background: bool,
    /// ロード済み `#STAGEFILE` の画像サイズ。
    pub stagefile_image_size: Option<SkinImageSize>,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) for course results.
    pub course_titles: [String; 10],
    pub course_result: CourseResultSkinSnapshot,
    /// Result 画面の graph 系 skin object に渡すプレイ中の推移データ。
    pub graph: Arc<crate::snapshot::ResultGraphSnapshot>,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// IR ランキング表示状態 (NUMBER_IR_* / OPTION_IR_*)。
    pub ir: ResultIrSnapshot,
    pub player_stats: PlayerStatsSnapshot,
}

impl ResultSnapshot {
    pub fn is_full_combo(&self) -> bool {
        self.total_notes > 0 && self.max_combo >= self.total_notes
    }
}
