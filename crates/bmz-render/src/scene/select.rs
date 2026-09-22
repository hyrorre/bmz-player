use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SelectSnapshot {
    pub time: TimeUs,
    /// beatoraja STRING_PLAYER (2) に渡す現在プロフィール名。
    pub player_name: String,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    /// アプリ起動後の経過時間 ms。
    /// beatoraja の NUMBER_OPERATING_TIME_HOUR/MINUTE/SECOND (27..29) に使う。
    pub operating_time_ms: i32,
    pub skin_input: SkinLogicalInputSnapshot,
    pub skin_attempt: SkinAttemptState,
    /// 現在の選曲スキンスロットに設定された destination offset。
    pub skin_offsets: SkinOffsetValues,
    pub selection_time: TimeUs,
    pub option_panel_time: TimeUs,
    /// TIMER_PANEL1_OFF..6_OFF (31..36) の経過時間。None は対応タイマーOFF。
    pub option_panel_off_times: [Option<TimeUs>; 6],
    pub option_panel: u8,
    pub chart_count: u32,
    pub selected_index: u32,
    /// beatoraja-style song bar movement direction. `1` means the new bars start
    /// from the next slot, `-1` from the previous slot, `0` disables movement.
    pub bar_scroll_direction: i32,
    /// Remaining song bar movement progress (1.0 at movement start, 0.0 at rest).
    pub bar_scroll_progress: f32,
    pub selected_chart_id: Option<i64>,
    /// Currently selected replay slot (0..=3), normalized to an existing slot.
    pub selected_replay_slot: Option<u8>,
    pub selected_title: String,
    /// Current profile hispeed shown to select skins (NUMBER_HISPEED=310/311).
    pub hispeed: f32,
    pub hispeed_mode_index: i32,
    pub base_hispeed_index: i32,
    pub normal_hispeed_level: u8,
    pub hispeed_config_index: i32,
    /// Effective target green number for the selected play mode. `None` when a
    /// mixed/unresolved course has no single mode whose value can be shown.
    pub note_display_duration_ms: Option<i32>,
    pub rows: Vec<SelectRowSnapshot>,
    pub arrange: String,
    pub arrange_2p: String,
    /// BMZ extension: このままプレイを開始した場合に適用する予定の固定レーン配置。
    ///
    /// 通常の RANDOM は選曲中には未抽選なので空。リプレイや将来のライバル配置コピーなど、
    /// 選曲中に配置が確定している場合だけ `pattern[表示先レーン] = 元レーン` を格納する。
    pub lane_shuffle_pattern: Vec<u8>,
    pub target: String,
    pub resolved_target_name: Option<String>,
    /// beatoraja STRING_CHARTREPLICATION (86)。
    pub chart_replication_mode: String,
    pub gauge: String,
    pub gauge_auto_shift: String,
    pub bottom_shiftable_gauge: String,
    pub double_option: String,
    pub hs_fix: String,
    pub assist: String,
    pub assist_flags: [bool; 7],
    pub assist_extra_note_depth: u8,
    pub assist_mine_mode: i64,
    pub assist_scroll_mode: i64,
    pub assist_long_note_mode: i64,
    pub guide_se_enabled: bool,
    pub constant_enabled: bool,
    pub select_mode: String,
    /// LR2-style difficulty filter: 0=ALL, 1=BEGINNER .. 5=INSANE.
    pub select_difficulty_filter: u8,
    /// LR2 text refs 190..196: target/max/min level, BPM range/max/min, stages.
    pub random_mix_options: [u32; 7],
    pub select_sort: String,
    pub select_ln_mode: String,
    /// BMZ extension: current profile scoring rule mode index.
    pub rule_mode_index: usize,
    /// BMZ extension: current profile LN setting index before normalization.
    pub ln_policy_setting_index: usize,
    /// BMZ extension: selected chart/course score-key LN policy index.
    pub ln_score_policy_index: Option<usize>,
    pub judge_algorithm: String,
    pub bga: String,
    /// Select detail option panelで表示する判定表示オフセット(ms)。
    pub judge_timing_offset_ms: i32,
    pub judge_timing_auto_adjust: bool,
    /// Select skin image refs/events 330..332 の表示状態。
    pub lanecover_enabled: bool,
    pub lift_enabled: bool,
    pub hidden_enabled: bool,
    /// beatoraja image/index ref 342。
    pub hispeed_auto_adjust: bool,
    pub master_volume: f32,
    pub key_volume: f32,
    pub bgm_volume: f32,
    pub current_folder: String,
    pub key_hint: String,
    pub option_hint: String,
    /// ESC 長押しによるアプリ終了の進捗 (0.0..=1.0)。0.0 のときは未押下。
    pub exit_hold_progress: f32,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// `#STAGEFILE` テクスチャがロード済みなら true。
    pub stage_background: bool,
    /// ロード済み `#STAGEFILE` の画像サイズ。
    pub stage_image_size: Option<SkinImageSize>,
    /// `#BACKBMP` テクスチャがロード済みなら true。
    pub backbmp_image: bool,
    /// ロード済み `#BACKBMP` の画像サイズ。
    pub backbmp_image_size: Option<SkinImageSize>,
    /// `#BANNER` テクスチャがロード済みなら true。
    pub banner_image: bool,
    /// ロード済み `#BANNER` の画像サイズ。
    pub banner_image_size: Option<SkinImageSize>,
    /// 設定フォルダ内にいるとき true。
    pub in_settings: bool,
    /// 設定項目の編集モード中。
    pub settings_editing: bool,
    /// 楽曲検索バー (beatoraja `STRING_SEARCHWORD`, ref=30) の入力オーバーレイに
    /// 表示する文字列。
    /// 検索モード中は入力中クエリ、非モード中は空 or 直前のメッセージ
    /// ("no song found" 等)。
    pub search_word: String,
    /// `search_word` に乗せる不透明度倍率 (0.0..=1.0)。placeholder /
    /// メッセージ表示時は薄く (< 1.0)、実入力中は 1.0。
    pub search_word_alpha: f32,
    /// `search_word` 内に重ねる検索 caret の UTF-8 byte index。
    pub search_caret_byte_index: Option<usize>,
    /// 検索入力モード中なら true。
    ///
    /// 非入力時の placeholder / feedback はスキン本来の destination 順で描画し、
    /// 入力中の文字と caret だけを TextField 相当の最前面オーバーレイにする。
    pub search_input_active: bool,
    /// Select skin mouse position in normalized skin-canvas coordinates.
    /// The origin is the top-left corner.
    pub mouse_position: Option<(f32, f32)>,
    /// 選曲カーソル譜面の IR ランキング状態 (NUMBER_IR_* / OPTION_IR_*)。
    pub ir: ResultIrSnapshot,
    /// 選曲カーソル譜面の IR ライバルベスト
    /// (NUMBER_RIVAL_*=271,275,276)。
    pub rival: Option<SelectRivalSnapshot>,
    /// beatoraja の `MusicSelector.getRival() != null` に相当する選択状態。
    /// 対象譜面にライバルスコアがなくても true のままにする。
    pub rival_selected: bool,
    /// 選択ライバル名。未プレイ譜面でも STRING_RIVAL (1) へ表示する。
    pub rival_name: String,
    /// beatoraja IndexType autosave_replay1..4 (321..324) image row indices.
    pub replay_slot_rule_indices: [i64; 4],
    pub player_stats: PlayerStatsSnapshot,
    pub selected_score_date_sec: i64,
}

/// 選曲カーソル譜面に対する IR ライバル (最上位 1 名) のベストスコア。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectRivalSnapshot {
    pub display_name: String,
    pub ex_score: u32,
    /// beatoraja ClearType index (0=NO PLAY .. 10=MAX)。
    pub clear_index: i64,
    pub max_combo: u32,
    pub bp: u32,
    /// EXスコア元プレイの PGREAT/GREAT/GOOD/BAD/POOR 内訳。
    /// legacy IR 応答では取得できないため None。
    pub judge_counts: Option<SelectRivalJudgeCounts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectRivalJudgeCounts {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
}

impl Default for SelectSnapshot {
    fn default() -> Self {
        Self {
            time: TimeUs::default(),
            player_name: String::new(),
            current_fps: 0,
            operating_time_ms: 0,
            skin_input: SkinLogicalInputSnapshot::default(),
            skin_attempt: SkinAttemptState::default(),
            skin_offsets: SkinOffsetValues::default(),
            selection_time: TimeUs::default(),
            option_panel_time: TimeUs::default(),
            option_panel_off_times: [None; 6],
            option_panel: 0,
            chart_count: 0,
            selected_index: 0,
            bar_scroll_direction: 0,
            bar_scroll_progress: 0.0,
            selected_chart_id: None,
            selected_replay_slot: None,
            selected_title: String::new(),
            hispeed: 0.0,
            hispeed_mode_index: 0,
            base_hispeed_index: 0,
            normal_hispeed_level: 18,
            hispeed_config_index: 4,
            note_display_duration_ms: None,
            rows: Vec::new(),
            arrange: String::new(),
            arrange_2p: String::new(),
            lane_shuffle_pattern: Vec::new(),
            target: String::new(),
            resolved_target_name: None,
            chart_replication_mode: String::new(),
            gauge: String::new(),
            gauge_auto_shift: String::new(),
            bottom_shiftable_gauge: String::new(),
            double_option: String::new(),
            hs_fix: String::new(),
            assist: String::new(),
            assist_flags: [false; 7],
            assist_extra_note_depth: 0,
            assist_mine_mode: 0,
            assist_scroll_mode: 0,
            assist_long_note_mode: 0,
            guide_se_enabled: false,
            constant_enabled: false,
            select_mode: String::new(),
            select_difficulty_filter: 0,
            random_mix_options: [0, 0, 0, 10, 0, 0, 5],
            select_sort: String::new(),
            select_ln_mode: String::new(),
            rule_mode_index: 0,
            ln_policy_setting_index: 0,
            ln_score_policy_index: None,
            judge_algorithm: String::new(),
            bga: String::new(),
            judge_timing_offset_ms: 0,
            judge_timing_auto_adjust: false,
            lanecover_enabled: false,
            lift_enabled: true,
            hidden_enabled: false,
            hispeed_auto_adjust: false,
            master_volume: 0.0,
            key_volume: 0.0,
            bgm_volume: 0.0,
            current_folder: String::new(),
            key_hint: String::new(),
            option_hint: String::new(),
            exit_hold_progress: 0.0,
            overlay: OverlaySnapshot::default(),
            stage_background: false,
            stage_image_size: None,
            backbmp_image: false,
            backbmp_image_size: None,
            banner_image: false,
            banner_image_size: None,
            in_settings: false,
            settings_editing: false,
            search_word: String::new(),
            search_word_alpha: 1.0,
            search_caret_byte_index: None,
            search_input_active: false,
            mouse_position: None,
            ir: ResultIrSnapshot::default(),
            rival: None,
            rival_selected: false,
            rival_name: String::new(),
            replay_slot_rule_indices: [0; 4],
            player_stats: PlayerStatsSnapshot::default(),
            selected_score_date_sec: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRowSnapshot {
    pub index: u32,
    pub title: String,
    /// 選曲一覧のバー内だけに表示する文字列。空の場合は `title` を使う。
    pub bar_text: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    /// BMZ's level display setting inside a difficulty-table level folder.
    /// None preserves the skin's normal behavior; the value has no table symbol.
    pub level_display_override: Option<String>,
    pub table_level: String,
    pub table_text_primary: String,
    pub table_text_secondary: String,
    pub table_text_fallback: String,
    /// songlist のレベル装飾を表示するか。G-BATTLEの固定行など、
    /// Song 行の描画を使いつつレベル欄だけ隠す場合に false。
    pub show_level: bool,
    /// 現在の曲の #RANK / 判定ランク。0..4 は VERYHARD..VERYEASY、10 以上は直接倍率。
    pub judge_rank: Option<i32>,
    pub total_notes: u32,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub length_ms: i64,
    pub clear_type: String,
    /// 選択中ライバルの beatoraja ClearType index (0=NO PLAY .. 10=MAX)。
    /// ライバル未プレイ譜面も 0。
    pub rival_clear_index: usize,
    pub ex_score: Option<u32>,
    pub max_combo: Option<u32>,
    pub gauge_value: Option<f32>,
    pub bp: Option<u32>,
    pub cb: Option<u32>,
    pub judge_counts: crate::snapshot::DisplayJudgeCounts,
    pub fast_slow_counts: Option<crate::snapshot::FastSlowJudgeCounts>,
    pub play_count: u32,
    pub clear_count: u32,
    pub replay_slots: [bool; 4],
    pub favorite_chart: bool,
    pub favorite_song: bool,
    /// Same-folder `.txt` presence for OPTION_NO_TEXT / OPTION_TEXT (174/175).
    pub has_document: bool,
    pub has_bga: bool,
    pub has_long_notes: bool,
    pub has_mines: bool,
    pub has_random: bool,
    /// BMZ source LN profile bit mask. None for non-chart rows.
    pub source_ln_profile_bits: Option<u8>,
    /// beatoraja SongInformation-derived chart details for selected song rows.
    pub chart_normal_notes: u32,
    pub chart_long_notes: u32,
    pub chart_scratch_notes: u32,
    pub chart_long_scratch_notes: u32,
    pub chart_mine_notes: u32,
    pub chart_density: f32,
    pub chart_peak_density: f32,
    pub chart_end_density: f32,
    pub chart_total_gauge: f32,
    pub chart_main_bpm: f32,
    pub chart_distribution: Vec<SelectChartDistributionSecond>,
    pub chart_bpm_graph_segments: Vec<BpmGraphSegment>,
    /// beatoraja DirectoryBar-style lamp distribution for folder rows.
    /// Indexes match SkinBar BARLAMP IDs: 0 no play, 1 failed, ... 10 max.
    pub folder_lamp_counts: [u32; 11],
    pub is_folder: bool,
    pub kind: SelectRowKind,
    /// library.db に登録済みかどうか。未登録の難易度表エントリは false。
    pub in_library: bool,
    /// コース行の場合のみ、これまでに達成したトロフィー名のリスト
    /// （`course_trophy_achievements` の DISTINCT、アルファ順）。
    /// それ以外の行 (Song / Folder / TableFolder) では常に空。
    ///
    /// `songlist.trophy` の描画判定で `SelectRowSnapshot` から直接参照する。
    /// `SkinDrawState` には載せない (Copy であるため Vec を抱えられない)。
    pub achieved_trophy_names: Vec<String>,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) for course rows.
    /// Empty for non-course rows.
    pub course_titles: [String; 10],
    /// beatoraja OPTION_GRADEBAR_* (1002..1017) for course rows.
    pub course_constraints: CourseConstraintFlags,
    /// 曲行のみ。beatoraja OPTION_MODE_* (160..164, 1160..1161) 用。
    pub chart_key_mode: Option<bmz_core::lane::KeyMode>,
}

impl SelectRowSnapshot {
    pub fn display_bar_text(&self) -> &str {
        if self.bar_text.is_empty() { &self.title } else { &self.bar_text }
    }
}

impl Default for SelectRowSnapshot {
    fn default() -> Self {
        Self {
            index: 0,
            title: String::new(),
            bar_text: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            level_display_override: None,
            table_level: String::new(),
            table_text_primary: String::new(),
            table_text_secondary: String::new(),
            table_text_fallback: String::new(),
            show_level: true,
            judge_rank: None,
            total_notes: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            length_ms: 0,
            clear_type: String::new(),
            rival_clear_index: 0,
            ex_score: None,
            max_combo: None,
            gauge_value: None,
            bp: None,
            cb: None,
            judge_counts: crate::snapshot::DisplayJudgeCounts::default(),
            fast_slow_counts: None,
            play_count: 0,
            clear_count: 0,
            replay_slots: [false; 4],
            favorite_chart: false,
            favorite_song: false,
            has_document: false,
            has_bga: false,
            has_long_notes: false,
            has_mines: false,
            has_random: false,
            source_ln_profile_bits: None,
            chart_normal_notes: 0,
            chart_long_notes: 0,
            chart_scratch_notes: 0,
            chart_long_scratch_notes: 0,
            chart_mine_notes: 0,
            chart_density: 0.0,
            chart_peak_density: 0.0,
            chart_end_density: 0.0,
            chart_total_gauge: 0.0,
            chart_main_bpm: 0.0,
            chart_distribution: Vec::new(),
            chart_bpm_graph_segments: Vec::new(),
            folder_lamp_counts: [0; 11],
            is_folder: false,
            kind: SelectRowKind::default(),
            in_library: true,
            achieved_trophy_names: Vec::new(),
            course_titles: Default::default(),
            course_constraints: CourseConstraintFlags::default(),
            chart_key_mode: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectChartDistributionSecond {
    pub scratch_long_heads: u16,
    pub scratch_long_bodies: u16,
    pub scratch_taps: u16,
    pub key_long_heads: u16,
    pub key_long_bodies: u16,
    pub key_taps: u16,
    pub mines: u16,
}

impl SelectChartDistributionSecond {
    pub fn total(self) -> u32 {
        u32::from(self.scratch_long_heads)
            + u32::from(self.scratch_long_bodies)
            + u32::from(self.scratch_taps)
            + u32::from(self.key_long_heads)
            + u32::from(self.key_long_bodies)
            + u32::from(self.key_taps)
            + u32::from(self.mines)
    }

    pub fn values(self) -> [u16; 7] {
        [
            self.scratch_long_heads,
            self.scratch_long_bodies,
            self.scratch_taps,
            self.key_long_heads,
            self.key_long_bodies,
            self.key_taps,
            self.mines,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CourseConstraintFlags {
    pub class: bool,
    pub mirror: bool,
    pub random: bool,
    pub no_speed: bool,
    pub no_good: bool,
    pub no_great: bool,
    pub gauge_lr2: bool,
    pub gauge_5k: bool,
    pub gauge_7k: bool,
    pub gauge_9k: bool,
    pub gauge_24k: bool,
    pub ln: bool,
    pub cn: bool,
    pub hcn: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectRowKind {
    #[default]
    Song,
    Folder,
    TableFolder,
    SearchFolder,
    Course,
    Executable,
    RandomCourse,
    Command,
    Container,
    NoSong,
    SettingsRoot,
    SettingsFolder,
    SettingsBack,
    SettingsClose,
    Config,
}
