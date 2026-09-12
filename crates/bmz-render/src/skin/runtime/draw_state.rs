use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SkinDrawState {
    pub elapsed_ms: i32,
    /// Lua skin のruntime draw callbackにだけ渡すsidecar context。
    /// JSON skinと通常のcompiled条件では常にNone。
    #[doc(hidden)]
    pub lua_runtime: Option<SkinLuaRuntimeContext>,
    /// Lua callback をロード時に変換した runtime flag。DynamicTimerRuntime が注入する。
    pub runtime_flags: HashMap<i32, bool>,
    /// BMZ logical inputs in E1/E2/E3/E4/UI Left/Right/Up/Down order.
    pub logical_input_held: [bool; SKIN_BMZ_INPUT_COUNT],
    /// Elapsed milliseconds since the latest aggregate false-to-true press edge.
    pub logical_input_press_ms: [Option<i32>; SKIN_BMZ_INPUT_COUNT],
    /// E1/E2 aggregate timer while either input remains held.
    pub e1_e2_press_ms: Option<i32>,
    /// E1/E2 aggregate timer after the last held input is released.
    pub e1_e2_release_ms: Option<i32>,
    /// beatoraja TIMER_STARTINPUT (1)。skin.input 待機後からの経過 ms。
    pub start_input_ms: Option<i32>,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    /// アプリ起動後の経過時間 ms。
    /// beatoraja の NUMBER_OPERATING_TIME_HOUR/MINUTE/SECOND (27..29) に使う。
    pub operating_time_ms: i32,
    pub ready_timer_ms: Option<i32>,
    pub play_timer_ms: Option<i32>,
    pub rhythm_timer_ms: Option<i32>,
    pub quarter_note_elapsed_ms: Option<i32>,
    pub key_mode: KeyMode,
    pub skin_attempt: SkinAttemptState,
    pub select_bar_elapsed_ms: i32,
    pub select_option_panel_elapsed_ms: i32,
    pub select_option_panel_off_elapsed_ms: [Option<i32>; 6],
    pub select_option_panel: u8,
    pub select_arrange_index: usize,
    pub select_arrange_2p_index: usize,
    pub select_extended_arrange_index: usize,
    pub select_extended_arrange_2p_index: usize,
    pub select_double_option_index: usize,
    pub select_hs_fix_index: usize,
    pub result_arrange_index: usize,
    pub result_arrange_2p_index: usize,
    pub result_extended_arrange_index: usize,
    pub result_extended_arrange_2p_index: usize,
    /// beatoraja RANDOM lane refs 450..469.
    ///
    /// Resultでは互換値、Play/SelectではBMZ拡張として、現在または開始予定の
    /// 固定レーン配置を画面共通で公開する。
    pub random_lane_refs: [u8; SKIN_RANDOM_LANE_REF_COUNT],
    /// 現在対象の譜面にLNが含まれるか。Noneは譜面未選択・未確定。
    pub chart_has_long_notes: Option<bool>,
    /// Resultの実効LN種別imageset index (0=LN, 1=CN, 2=HCN)。
    pub result_ln_mode_index: Option<usize>,
    /// BMZ extension: frozen scoring rule mode (0=BEATORAJA, 1=LR2ORAJA, 2=DX).
    pub rule_mode_index: usize,
    /// BMZ extension: Select profile LN setting before chart normalization.
    pub ln_policy_setting_index: Option<usize>,
    /// BMZ extension: chart/attempt score-key LN policy after normalization.
    pub ln_score_policy_index: Option<usize>,
    pub select_gauge_index: usize,
    pub select_gauge_auto_shift_index: usize,
    pub select_bottom_shiftable_gauge_index: usize,
    pub select_target_index: usize,
    pub select_bga_index: usize,
    pub select_assist_index: usize,
    pub assist_flags: [bool; 7],
    pub assist_extra_note_depth: u8,
    pub assist_mine_mode: i64,
    pub assist_scroll_mode: i64,
    pub assist_long_note_mode: i64,
    pub guide_se_enabled: bool,
    pub constant_enabled: bool,
    /// BMZ extension: exact select session mode index.
    /// 0=NORMAL, 1=PRACTICE, 2=AUTOPLAY, 3=AUTO BATTLE, 4=G-BATTLE.
    pub select_session_mode_index: usize,
    pub select_mode_index: usize,
    pub select_difficulty_filter_index: usize,
    /// LR2 RANDOM MIX text refs 190..196.
    pub random_mix_options: [u32; 7],
    pub select_sort_index: usize,
    pub select_ln_mode_index: usize,
    pub select_judge_algorithm_index: usize,
    pub mouse_x: Option<f32>,
    pub mouse_y: Option<f32>,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub player_stats: PlayerStatsSnapshot,
    pub course_result: CourseResultSkinSnapshot,
    pub gauge: f32,
    pub gauge_type: i32,
    pub opponent_gauge: Option<f32>,
    pub opponent_gauge_type: Option<i32>,
    pub gauge_auto_shift: bool,
    pub gauge_max: f32,
    pub gauge_border: f32,
    pub play_progress: f32,
    pub end_of_note: bool,
    pub end_of_note_ms: Option<i32>,
    /// 各レーンのボムタイマー経過ms。Noneなら非アクティブ。
    pub bomb_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyon(押下中ビーム)タイマー経過ms。Noneなら非アクティブ。
    pub keyon_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyoff(離した直後の演出)タイマー経過ms。Noneなら非アクティブ。
    /// beatoraja の TIMER_KEYOFF_1P_KEY1..7 (121..127) / SCRATCH (120) に対応。
    pub keyoff_ms: [Option<i32>; LANE_COUNT],
    /// PeacefulPlay互換キービームの押下中表示状態。renderer runtimeが更新する。
    pub keybeam_hold_active: [bool; LANE_COUNT],
    /// PeacefulPlay互換キービームの解放フェード表示状態。renderer runtimeが更新する。
    pub keybeam_fade_active: [bool; LANE_COUNT],
    /// 各レーンの LN ホールドタイマー経過ms。ホールド中のみ Some。
    /// beatoraja の TIMER_HOLD_1P (70..=77) / TIMER_HOLD_2P (80..=87) に対応。
    pub hold_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの HCN ACTIVE(回復中) タイマー経過ms。
    /// beatoraja の TIMER_HCN_ACTIVE_1P (250..=257) / 2P (260..=267) に対応。
    pub hcn_active_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの HCN DAMAGE(減衰中) タイマー経過ms。
    /// beatoraja の TIMER_HCN_DAMAGE_1P (270..=277) / 2P (280..=287) に対応。
    pub hcn_damage_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの直近判定の画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR,5=MISS)。
    /// imageset (ボム・キービーム) の画像選択に使う。Noneなら判定なし。
    pub lane_judge: [Option<usize>; LANE_COUNT],
    /// 判定タイマー経過ms。index 0/1/2 = TIMER_JUDGE_1P/2P/3P (46/47/247)。Noneなら非アクティブ。
    pub judge_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    /// Full combo timer elapsed ms (TIMER_FULLCOMBO_1P/2P=48/49)。Noneなら非アクティブ。
    pub full_combo_ms: Option<i32>,
    pub full_combo_2p_ms: Option<i32>,
    pub music_end_ms: Option<i32>,
    /// Gauge increase timer elapsed ms (TIMER_GAUGE_INCLEASE_1P/2P=42/43)。
    pub gauge_increase_ms: Option<i32>,
    pub gauge_increase_2p_ms: Option<i32>,
    /// Gauge max timer elapsed ms (TIMER_GAUGE_MAX_1P/2P=44/45)。
    pub gauge_max_ms: Option<i32>,
    pub gauge_max_2p_ms: Option<i32>,
    pub end_of_note_2p_ms: Option<i32>,
    /// 領域別の判定画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR,5=MISS)。
    pub judge_index: [Option<usize>; MAX_JUDGE_REGIONS],
    /// 領域別の判定表示用 combo。beatoraja `JudgeManager.judgecombo` 相当。
    pub judge_combo: [u32; MAX_JUDGE_REGIONS],
    /// 領域別の判定タイミング符号。1=EARLY/FAST, -1=LATE/SLOW。
    pub judge_timing_sign: [Option<i8>; MAX_JUDGE_REGIONS],
    /// BMZ拡張: 判定領域×Scratch/鍵盤別の判定タイマー経過ms。
    /// index は region 0 Scratch/Keys, region 1 Scratch/Keys, region 2 Scratch/Keys。
    pub judge_lane_ms: [Option<i32>; SKIN_BMZ_JUDGE_LANE_COUNT],
    /// BMZ拡張: 判定領域×Scratch/鍵盤別の最新判定画像インデックス。
    pub judge_lane_index: [Option<usize>; SKIN_BMZ_JUDGE_LANE_COUNT],
    /// BMZ拡張: 判定領域×Scratch/鍵盤別のタイミング符号。
    /// 1=EARLY/FAST, -1=LATE/SLOW, None=表示フィルタで非表示。
    pub judge_lane_timing_sign: [Option<i8>; SKIN_BMZ_JUDGE_LANE_COUNT],
    /// BMZ拡張: 判定領域×Scratch/鍵盤別のタイミング差ms。
    /// 符号は 押下時刻 - note時刻 (FAST=負)。Noneなら非表示。
    pub judge_lane_timing_ms: [Option<i32>; SKIN_BMZ_JUDGE_LANE_COUNT],
    /// OFFSET_LIFT (id=3) の y 値 (skin canvas pixel 単位)。リフト量に応じて要素をシフトする。
    pub offset_lift_px: i32,
    /// OFFSET_LANECOVER (id=4) の y 値 (skin canvas pixel 単位)。レーンカバー位置インジケータのシフト。
    pub offset_lanecover_px: i32,
    /// OFFSET_HIDDEN_COVER (id=5) の y 値 (skin canvas pixel 単位)。HIDDEN カバー位置のシフト。
    pub offset_hidden_cover_px: i32,
    /// ユーザーまたは profile で指定された任意の skin offset 値。
    pub skin_offsets: SkinOffsetValues,
    /// 現在のハイスピード倍率 (NUMBER_HISPEED=310, NUMBER_HISPEED_AFTERDOT=311 に使用)。
    pub hispeed: f32,
    /// BMZ extension: current hispeed mode. 0=base, 1=Floating.
    pub hispeed_mode_index: i32,
    /// BMZ extension: configured base hispeed. 0=Classic, 1=Normal.
    pub base_hispeed_index: i32,
    /// BMZ extension: selected Normal hispeed level (1..=20).
    pub normal_hispeed_level: u8,
    /// BMZ extension: five-choice hispeed configuration index.
    pub hispeed_config_index: i32,
    /// BMZ extension: target green number used by Floating.
    pub target_green_number: u32,
    /// 曲残り時間 ms (NUMBER_TIMELEFT_MINUTE=163, NUMBER_TIMELEFT_SECOND=164 に使用)。
    pub timeleft_ms: i32,
    /// ノーツ表示時間 ms (NUMBER_DURATION=312 に使用)。
    pub total_duration_ms: i32,
    /// 緑数字 ms (NUMBER_DURATION_GREEN=313 に使用)。
    /// 指定されている場合は NUMBER_DURATION=312 をこの値の 5/3 倍として返す。
    pub duration_green_ms: Option<i32>,
    /// Result の曲長 ms (NUMBER_PLAYTIME_MINUTE/SECOND=1163/1164 に使用)。
    pub result_duration_ms: i32,
    /// レーンカバー割合 0.0-1.0 (NUMBER_LANECOVER1=14 は 0..=1000 で返す)。
    pub lane_cover: f32,
    /// リフト量 0.0-1.0 (NUMBER_LIFT1=314 に使用)。
    pub lift: f32,
    /// HIDDEN カバー割合 0.0-1.0。未対応の間は 0.0 で hiddenCover を描画しない。
    pub hidden_cover: f32,
    /// OPTION_LANECOVER1_CHANGING (270)。Start/Select 押下中に true。
    pub lane_cover_changing: bool,
    /// OPTION_LANECOVER1_ON (271)。
    pub lanecover_enabled: bool,
    /// OPTION_LIFT1_ON (272)。
    pub lift_enabled: bool,
    /// OPTION_HIDDEN1_ON (273)。
    pub hidden_enabled: bool,
    /// beatoraja image/index ref 342。
    pub hispeed_auto_adjust: bool,
    /// 現在 BPM (NUMBER_NOWBPM=160 に使用)。
    pub now_bpm: f32,
    /// 最小 BPM (NUMBER_MINBPM=91 に使用)。
    pub min_bpm: f32,
    /// 最大 BPM (NUMBER_MAXBPM=90 に使用)。
    pub max_bpm: f32,
    /// 現在の曲にBGAイベントが含まれるかどうか (OPTION_NO_BGA=170 / OPTION_BGA=171)。
    pub has_bga: bool,
    /// 現在の曲に STOP イベントが含まれるかどうか (OPTION_BPMSTOP=1177)。
    pub has_bpm_stop: bool,
    /// BGA表示設定がONかどうか。曲の有無とは分けて扱う。
    pub bga_enabled: bool,
    /// `#STAGEFILE` 相当の曲画像があるか (OPTION_NO_STAGEFILE=190 / OPTION_STAGEFILE=191)。
    pub has_stagefile: bool,
    /// runtime image 100 (`#STAGEFILE`) のロード済み画像サイズ。
    pub stagefile_image_size: Option<SkinImageSize>,
    /// `#BACKBMP` 相当の背景画像がロード済みか (OPTION_NO_BACKBMP=194 / OPTION_BACKBMP=195)。
    pub has_backbmp: bool,
    /// 現在表示するBGA本体画像。
    pub bga_base: Option<SkinBgaFrame>,
    /// 現在表示するBGAレイヤー画像。
    pub bga_layer: Option<SkinBgaFrame>,
    /// 現在表示するBGAレイヤー2画像 (ch 0A)。
    pub bga_layer2: Option<SkinBgaFrame>,
    /// 直近のBAD/POORで一時表示するミスレイヤー画像。
    pub bga_poor: Option<SkinBgaFrame>,
    /// BGA destination に stretch 指定が無い場合に使う拡大設定。
    pub bga_stretch: i32,
    /// 判定領域別の最後の判定タイミングずれ ms (VALUE_JUDGE_1P/2P/3P_DURATION=525/526/527 に使用)。
    /// 符号は 押下時刻 - note時刻 (FAST=負)。Noneなら非表示。
    pub judge_timing_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    /// DB 上のベスト ex スコア。
    /// Result では保存前ベスト (`previous_best_ex_score`) を MYBEST 表示に優先する。
    pub best_ex_score: Option<u32>,
    /// ghost から現在進行度まで積算した過去ベスト EX。None の場合は final score の線形投影を使う。
    pub projected_best_ex_score: Option<u32>,
    /// 過去ベストのクリアタイプ index (ref 371)。
    pub best_clear_index: Option<i64>,
    /// ターゲットスコアのexスコア (NUMBER_TARGET_SCORE=121, BARGRAPH_TARGETSCORERATE=115 に使用)。
    pub target_ex_score: Option<u32>,
    /// 判定タイミングオフセット設定値 ms (NUMBER_JUDGETIMING=12 に使用、beatoraja の judgetiming 設定)。
    pub judge_timing_offset_ms: i32,
    /// beatoraja IndexType.notesdisplaytimingautoadjust (number/event id 75).
    pub judge_timing_auto_adjust: bool,
    /// 選択中 DirectoryBar に含まれる譜面数 (NUMBER_FOLDER_TOTALSONGS=300)。
    /// 非 DirectoryBar では None。これは現在の表示フォルダー全体の行数ではなく、カーソル行の値。
    pub select_folder_song_count: Option<u32>,
    /// 現在の描画状態が選曲画面かどうか。番号 ref の一部は scene ごとに意味が違う。
    pub select_screen: bool,
    /// 選曲バーのスクロール位置 0.0-1.0。
    pub select_scroll_progress: f32,
    /// 選曲画面の master/key/bgm 音量 0.0-1.0。
    pub select_master_volume: f32,
    pub select_key_volume: f32,
    pub select_bgm_volume: f32,
    /// 選択中バーにバナー画像があるか (OPTION_NO_BANNER=192 / OPTION_BANNER=193)。
    pub select_has_banner: bool,
    /// 選択中 SongData と同じフォルダに `.txt` があるか (OPTION_NO_TEXT/TEXT=174/175)。
    pub select_has_document: bool,
    /// 選択中曲のレベル表記から取り出した数値。
    pub select_play_level: i64,
    /// 現在の曲のレベル表記から取り出した数値 (NUMBER_PLAYLEVEL=96)。
    pub play_level: i64,
    /// beatoraja OPTION_TABLE_SONG (1008).
    pub table_song: bool,
    /// 現在の曲の #DIFFICULTY code。0=OTHER, 1=BEGINNER, 2=NORMAL, 3=HYPER, 4=ANOTHER, 5=INSANE。
    pub difficulty: i64,
    /// 現在の曲の #RANK / 判定ランク。0..4 は VERYHARD..VERYEASY、10 以上は直接倍率。
    pub judge_rank: Option<i32>,
    /// 選択中曲のベストEXスコア。
    pub select_ex_score: Option<u32>,
    /// 選択中曲のリプレイスロット有無。
    pub select_replay_slots: [bool; 4],
    /// 選択中リプレイスロット。Noneなら未選択。
    pub select_replay_index: Option<usize>,
    /// 選択中曲のクリアランプ番号。
    pub select_clear_index: i64,
    /// 選曲画面のお気に入り状態。beatoraja IndexType favorite_song(89) / favorite_chart(90) 用。
    /// value ref 89/90 とは別名前空間。BMZ は invisible を持たず 0/1。
    pub select_favorite_song: bool,
    pub select_favorite_chart: bool,
    /// Result の favorite_chart (image ref 90)。None は Result 以外。
    /// BMZ は invisible を持たないため Some(false/true) の2状態だけを返す。
    pub result_favorite_chart: Option<bool>,
    /// beatoraja IndexType autosave_replay1..4 (321..324) image row indices。
    pub select_replay_slot_rule_indices: [i64; 4],
    /// beatoraja ValueType folder_noplay..folder_max (320..330) 用のランプ別曲数。
    pub select_folder_lamp_counts: [u32; 11],
    /// 選択中バー種別。OPTION_FOLDERBAR / SONGBAR / GRADEBAR の判定に使う。
    pub select_row_kind: SelectRowKind,
    /// 選択中 GradeBar の制約。OPTION_GRADEBAR_* (1002..1017) の判定に使う。
    pub select_course_constraints: CourseConstraintFlags,
    /// 選択中バーがフォルダかどうか。
    pub select_is_folder: bool,
    /// 選択中 SongBar / GradeBar 相当が library.db に登録済みかどうか。
    /// OPTION_PLAYABLEBAR=5 と no-songs SkinBar 表示に使う。
    pub select_in_library: bool,
    /// 選択中曲のノーツ数。
    pub select_total_notes: u32,
    /// beatoraja SongInformation-derived selected chart detail numbers.
    pub select_chart_normal_notes: u32,
    pub select_chart_long_notes: u32,
    pub select_chart_scratch_notes: u32,
    pub select_chart_long_scratch_notes: u32,
    pub select_chart_mine_notes: u32,
    pub select_chart_density: f32,
    pub select_chart_peak_density: f32,
    pub select_chart_end_density: f32,
    pub select_chart_total_gauge: f32,
    pub select_chart_main_bpm: f32,
    /// 選択中曲の代表BPM。
    pub select_bpm: f32,
    /// 選択中曲の最小BPM。
    pub select_min_bpm: f32,
    /// 選択中曲の最大BPM。
    pub select_max_bpm: f32,
    /// 選択中曲の長さ ms。
    pub select_length_ms: i64,
    /// 選択中曲のプレイ回数 / クリア回数 / ミスカウント。
    pub select_play_count: u32,
    pub select_clear_count: u32,
    pub select_bp: Option<u32>,
    pub select_cb: Option<u32>,
    /// Fast/Slow 内訳 (ref 410-419/421-424)。
    /// Play/Result 中は Some、それ以外は None。
    pub fast_slow_counts: Option<crate::snapshot::FastSlowJudgeCounts>,
    /// PeacefulPlay key logger: 直近1秒のPress数。
    pub keylogger_nps: u32,
    /// display lane別の COOL/GREAT/GOOD/BAD 累積数。
    pub keylogger_judge_counts: [[u32; 4]; LANE_COUNT],
    /// display lane別の COOL/FAST/SLOW 累積数。
    pub keylogger_fast_slow_counts: [[u32; 3]; LANE_COUNT],
    /// display lane別、直近16 Pressの開始時刻からの経過ms。
    pub keylogger_event_ms: [[Option<i32>; 16]; LANE_COUNT],
    pub keylogger_event_judge: [[u8; 16]; LANE_COUNT],
    pub keylogger_event_fast_slow: [[u8; 16]; LANE_COUNT],
    /// display lane別のチャタリング警告開始時刻からの経過ms。
    pub keylogger_chattering_ms: [Option<i32>; LANE_COUNT],
    pub keylogger_exclude_cool: bool,
    /// 過去ベスト max combo (ref 172)。
    pub best_max_combo: Option<u32>,
    /// ターゲット max combo (ref 173, 175 で使用)。
    pub target_max_combo: Option<u32>,
    /// 過去ベスト min_bp (ref 176, 178 で使用)。
    pub best_bp: Option<u32>,
    /// Result 画面で表示する今回 BP/CB。Failed では未処理ノーツを含む記録用値。
    pub result_bp: Option<u32>,
    pub result_cb: Option<u32>,
    /// Result 画面の IR ランキング状態 (NUMBER_IR_* / OPTION_IR_*)。
    pub ir_ranking: crate::scene::ResultIrSnapshot,
    /// Selectでライバルが選択されているか。譜面スコアの有無とは分離する。
    pub rival_selected: bool,
    /// 選曲カーソル譜面の IR ライバルベスト EX (NUMBER_RIVAL_SCORE=271)。
    pub rival_ex_score: Option<i64>,
    /// 同 max combo (NUMBER_RIVAL_MAXCOMBO=275)。
    pub rival_max_combo: Option<i64>,
    /// 同 BP (NUMBER_RIVAL_MISSCOUNT=276)。
    pub rival_bp: Option<i64>,
    /// EXスコア元プレイの PGREAT/GREAT/GOOD/BAD/POOR
    /// (NUMBER/FLOAT_RIVAL_*=280..289)。
    pub rival_judge_counts: Option<[u32; 5]>,
    /// Result update/draw ops 用の保存前ベスト。
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_clear_index: Option<i64>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_bp: Option<u32>,
    /// ターゲット min_bp (ref 176, 178 で使用)。
    pub target_bp: Option<u32>,
    /// ターゲットクリアタイプの index (ref 371)。
    pub target_clear_index: Option<i64>,
    /// リザルト画面でクリアしたか (op 90=CLEAR, op 91=FAIL)。
    /// Play 中は None、Result 中は Some(true)=Fail / Some(false)=Clear。
    pub result_failed: Option<bool>,
    /// シーン終了フェードアウトのタイマー経過 ms (TIMER_FADEOUT=2)。
    /// None ならフェードアウト中でない。`timer: 2` の destination はこの値が
    /// Some のときだけ描画され、リザルト画面終了時のアニメーションを駆動する。
    pub fadeout_ms: Option<i32>,
    /// RESULT graph begin/end timers (150/151) and update score timer (152)。
    pub result_graph_begin_ms: Option<i32>,
    pub result_graph_end_ms: Option<i32>,
    pub result_update_score_ms: Option<i32>,
    /// Result gaugegraph display type selected by result key CHANGE_GRAPH.
    pub result_gauge_graph_type: Option<i32>,
    /// Lua Result スキンの展開パネル (0=非表示、1=IR、2=グラフ)。
    pub result_panel: Option<i32>,
    /// RESULT replay slot status for OPTION_REPLAYDATA* / *_SAVED.
    pub result_replay_slots: [bool; 4],
    pub result_saved_replay_slots: [bool; 4],
    /// 閉店/FAILED 演出のタイマー経過 ms (TIMER_FAILED=3)。
    pub failed_ms: Option<i32>,
    /// Result timing distribution average (NUMBER_AVERAGE_TIMING=374).
    pub average_timing_ms: Option<f32>,
    /// Result全ノートの平均絶対判定ずれ us (NUMBER_AVERAGE_DURATION=372/373)。
    /// 未判定ノートは beatoraja と同じく 1,000,000us として集計する。
    pub average_duration_us: Option<i64>,
    /// Result timing distribution standard deviation (NUMBER_STDDEV_TIMING=376).
    pub stddev_timing_ms: Option<f32>,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// BMSPlayer のプレイ画面か。プレイ専用 op が他 scene で true にならないために使う。
    pub play_screen: bool,
    /// BMSPlayer が replay モードか。
    pub replay_playback: bool,
    /// BMSPlayer が practice モードか。
    pub practice_mode: bool,
    /// beatoraja PlayerResource.updateScore。None は対象外 scene。
    pub score_save_enabled: Option<bool>,
    /// OPTION_NOW_LOADING (80) / OPTION_LOADED (81) 用。
    pub skin_loaded: bool,
    /// NUMBER_LOADING_PROGRESS (165) / RateType load_progress (102) 用。
    pub resource_load_progress: f32,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    /// beatoraja `event_index(SKIN_EVENT_HSFIX)`。0=OFF, 1=START, 2=MAX, 3=MAIN, 4=MIN。
    pub hsfix_index: i32,
    /// beatoraja `NUMBER_MAINBPM` (92) 用の代表 BPM (プレイ中)。
    pub main_bpm: f32,
    /// Rm-skin F/S threshold 表示 (ms)。
    pub fs_threshold_ms: i32,
    /// HSFIX 連動の adjusted hidden cover (0..1)。
    pub adjusted_cover_progress: Option<f32>,
    /// HSFIX 連動の BPM 比率 (0..1)。
    pub adjusted_rate: Option<f32>,
    /// HSFIX 連動の BPM 比率 ×100 整数部。
    pub adjusted_rate_adot: Option<i32>,
    /// HitErrorVisualizer 用の直近判定タイミング (ms)。
    pub hit_error_ring: [i64; bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
    pub hit_error_ring_index: usize,
    /// `dynamicTimer` で定義された observe タイマーの経過 ms。None は timer_off。
    pub dynamic_timer_ms: [Option<i32>; SKIN_DYNAMIC_TIMER_COUNT],
    /// Lua custom timerのうち固定delayとしてIR化できたタイマーの経過ms。
    pub fixed_delay_timer_ms: HashMap<i32, i32>,
    /// 選曲画面の設定フォルダ内。曲メタデータ用の op / text / number を抑制する。
    pub in_settings: bool,
    /// 設定項目の編集モード中 (`in_settings` と併用)。
    pub settings_editing: bool,
    /// 選曲中の曲行キーモード。beatoraja OPTION_MODE_* (160..164) 用。
    pub select_chart_key_mode: Option<KeyMode>,
}

impl Default for SkinDrawState {
    fn default() -> Self {
        Self {
            elapsed_ms: 0,
            lua_runtime: None,
            runtime_flags: HashMap::new(),
            logical_input_held: [false; SKIN_BMZ_INPUT_COUNT],
            logical_input_press_ms: [None; SKIN_BMZ_INPUT_COUNT],
            e1_e2_press_ms: None,
            e1_e2_release_ms: None,
            start_input_ms: None,
            current_fps: 0,
            operating_time_ms: 0,
            ready_timer_ms: None,
            play_timer_ms: None,
            rhythm_timer_ms: None,
            quarter_note_elapsed_ms: None,
            key_mode: KeyMode::default(),
            skin_attempt: SkinAttemptState::default(),
            select_bar_elapsed_ms: 0,
            select_option_panel_elapsed_ms: 0,
            select_option_panel_off_elapsed_ms: [None; 6],
            select_option_panel: 0,
            select_arrange_index: 0,
            select_arrange_2p_index: 0,
            select_extended_arrange_index: 0,
            select_extended_arrange_2p_index: 0,
            select_double_option_index: 0,
            select_hs_fix_index: 0,
            result_arrange_index: 0,
            result_arrange_2p_index: 0,
            result_extended_arrange_index: 0,
            result_extended_arrange_2p_index: 0,
            random_lane_refs: [0; SKIN_RANDOM_LANE_REF_COUNT],
            chart_has_long_notes: None,
            result_ln_mode_index: None,
            rule_mode_index: 0,
            ln_policy_setting_index: None,
            ln_score_policy_index: None,
            select_gauge_index: 2,
            select_gauge_auto_shift_index: 0,
            select_bottom_shiftable_gauge_index: 0,
            select_target_index: 0,
            select_bga_index: 0,
            select_assist_index: 0,
            assist_flags: [false; 7],
            assist_extra_note_depth: 0,
            assist_mine_mode: 0,
            assist_scroll_mode: 0,
            assist_long_note_mode: 0,
            guide_se_enabled: false,
            constant_enabled: false,
            select_session_mode_index: 0,
            select_mode_index: 0,
            select_difficulty_filter_index: 0,
            random_mix_options: [0, 0, 0, 10, 0, 0, 5],
            select_sort_index: 0,
            select_ln_mode_index: 0,
            select_judge_algorithm_index: 0,
            mouse_x: None,
            mouse_y: None,
            combo: 0,
            max_combo: 0,
            ex_score: 0,
            total_notes: 0,
            past_notes: 0,
            judge_counts: DisplayJudgeCounts::default(),
            player_stats: PlayerStatsSnapshot::default(),
            course_result: CourseResultSkinSnapshot::default(),
            gauge: 0.0,
            gauge_type: 2,
            opponent_gauge: None,
            opponent_gauge_type: None,
            gauge_auto_shift: false,
            gauge_max: 100.0,
            gauge_border: 80.0,
            play_progress: 0.0,
            end_of_note: false,
            end_of_note_ms: None,
            bomb_ms: [None; LANE_COUNT],
            keyon_ms: [None; LANE_COUNT],
            keyoff_ms: [None; LANE_COUNT],
            keybeam_hold_active: [false; LANE_COUNT],
            keybeam_fade_active: [false; LANE_COUNT],
            hold_ms: [None; LANE_COUNT],
            hcn_active_ms: [None; LANE_COUNT],
            hcn_damage_ms: [None; LANE_COUNT],
            lane_judge: [None; LANE_COUNT],
            judge_ms: [None; MAX_JUDGE_REGIONS],
            full_combo_ms: None,
            full_combo_2p_ms: None,
            music_end_ms: None,
            gauge_increase_ms: None,
            gauge_increase_2p_ms: None,
            gauge_max_ms: None,
            gauge_max_2p_ms: None,
            end_of_note_2p_ms: None,
            judge_index: [None; MAX_JUDGE_REGIONS],
            judge_combo: [0; MAX_JUDGE_REGIONS],
            judge_timing_sign: [None; MAX_JUDGE_REGIONS],
            judge_lane_ms: [None; SKIN_BMZ_JUDGE_LANE_COUNT],
            judge_lane_index: [None; SKIN_BMZ_JUDGE_LANE_COUNT],
            judge_lane_timing_sign: [None; SKIN_BMZ_JUDGE_LANE_COUNT],
            judge_lane_timing_ms: [None; SKIN_BMZ_JUDGE_LANE_COUNT],
            offset_lift_px: 0,
            offset_lanecover_px: 0,
            offset_hidden_cover_px: 0,
            skin_offsets: SkinOffsetValues::default(),
            hispeed: 0.0,
            hispeed_mode_index: 0,
            base_hispeed_index: 0,
            normal_hispeed_level: 18,
            hispeed_config_index: 4,
            target_green_number: 0,
            timeleft_ms: 0,
            total_duration_ms: 0,
            duration_green_ms: None,
            result_duration_ms: 0,
            lane_cover: 0.0,
            lift: 0.0,
            hidden_cover: 0.0,
            lane_cover_changing: false,
            lanecover_enabled: true,
            lift_enabled: true,
            hidden_enabled: false,
            hispeed_auto_adjust: false,
            now_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            has_bga: false,
            has_bpm_stop: false,
            bga_enabled: true,
            has_stagefile: false,
            stagefile_image_size: None,
            has_backbmp: false,
            bga_base: None,
            bga_layer: None,
            bga_layer2: None,
            bga_poor: None,
            bga_stretch: 1,
            judge_timing_ms: [None; MAX_JUDGE_REGIONS],
            best_ex_score: None,
            projected_best_ex_score: None,
            best_clear_index: None,
            target_ex_score: None,
            judge_timing_offset_ms: 0,
            judge_timing_auto_adjust: false,
            select_folder_song_count: None,
            select_screen: false,
            select_scroll_progress: 0.0,
            select_master_volume: 1.0,
            select_key_volume: 1.0,
            select_bgm_volume: 1.0,
            select_has_banner: false,
            select_has_document: false,
            select_play_level: 0,
            play_level: 0,
            table_song: false,
            difficulty: 0,
            judge_rank: None,
            select_ex_score: None,
            select_replay_slots: [false; 4],
            select_replay_index: None,
            select_clear_index: 0,
            select_favorite_song: false,
            select_favorite_chart: false,
            result_favorite_chart: None,
            select_replay_slot_rule_indices: [0; 4],
            select_folder_lamp_counts: [0; 11],
            select_row_kind: SelectRowKind::Song,
            select_course_constraints: CourseConstraintFlags::default(),
            select_is_folder: false,
            select_in_library: true,
            select_total_notes: 0,
            select_chart_normal_notes: 0,
            select_chart_long_notes: 0,
            select_chart_scratch_notes: 0,
            select_chart_long_scratch_notes: 0,
            select_chart_mine_notes: 0,
            select_chart_density: 0.0,
            select_chart_peak_density: 0.0,
            select_chart_end_density: 0.0,
            select_chart_total_gauge: 0.0,
            select_chart_main_bpm: 0.0,
            select_bpm: 0.0,
            select_min_bpm: 0.0,
            select_max_bpm: 0.0,
            select_length_ms: 0,
            select_play_count: 0,
            select_clear_count: 0,
            select_bp: None,
            select_cb: None,
            fast_slow_counts: None,
            keylogger_nps: 0,
            keylogger_judge_counts: [[0; 4]; LANE_COUNT],
            keylogger_fast_slow_counts: [[0; 3]; LANE_COUNT],
            keylogger_event_ms: [[None; 16]; LANE_COUNT],
            keylogger_event_judge: [[0; 16]; LANE_COUNT],
            keylogger_event_fast_slow: [[0; 16]; LANE_COUNT],
            keylogger_chattering_ms: [None; LANE_COUNT],
            keylogger_exclude_cool: false,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            result_bp: None,
            result_cb: None,
            ir_ranking: crate::scene::ResultIrSnapshot::default(),
            rival_selected: false,
            rival_ex_score: None,
            rival_max_combo: None,
            rival_bp: None,
            rival_judge_counts: None,
            previous_best_ex_score: None,
            previous_best_clear_index: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_bp: None,
            target_clear_index: None,
            result_failed: None,
            fadeout_ms: None,
            result_graph_begin_ms: None,
            result_graph_end_ms: None,
            result_update_score_ms: None,
            result_gauge_graph_type: None,
            result_panel: None,
            result_replay_slots: [false; 4],
            result_saved_replay_slots: [false; 4],
            failed_ms: None,
            average_timing_ms: None,
            average_duration_us: None,
            stddev_timing_ms: None,
            autoplay: false,
            play_screen: false,
            replay_playback: false,
            practice_mode: false,
            score_save_enabled: None,
            skin_loaded: true,
            resource_load_progress: 1.0,
            course_stage: None,
            hsfix_index: 0,
            main_bpm: 0.0,
            fs_threshold_ms: 25,
            adjusted_cover_progress: None,
            adjusted_rate: None,
            adjusted_rate_adot: None,
            hit_error_ring: [bmz_gameplay::hit_error::HIT_ERROR_EMPTY;
                bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
            hit_error_ring_index: 0,
            dynamic_timer_ms: [None; SKIN_DYNAMIC_TIMER_COUNT],
            fixed_delay_timer_ms: HashMap::new(),
            in_settings: false,
            settings_editing: false,
            select_chart_key_mode: None,
        }
    }
}
