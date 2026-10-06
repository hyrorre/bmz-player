use super::schema_visualizers::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinTextureId(pub u32);

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinDocument {
    /// LR2 CSV origin. Keeps compatibility behavior separate from JSON/Lua skins.
    #[serde(default)]
    pub lr2: bool,
    #[serde(default, rename = "lr2Result")]
    pub lr2_result: Option<SkinLr2ResultDef>,
    #[serde(default, rename = "lr2Charts")]
    pub lr2_charts: Vec<SkinLr2ChartDef>,
    #[serde(default, rename = "type")]
    pub skin_type: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    /// Explicit support version; only 1 enables the new E2 UI and input scheme.
    #[serde(default, rename = "bmzDetailOptions")]
    pub bmz_detail_options: u32,
    /// Supports numeric rows and the experimental two-panel interaction.
    #[serde(default, rename = "bmzDetailOptionsNumbers")]
    pub bmz_detail_options_numbers: bool,
    /// Opt in to a read-only 300 ms E2 exit snapshot, animated with timer 32.
    #[serde(default, rename = "bmzDetailOptionsClose")]
    pub bmz_detail_options_close: bool,
    #[serde(default = "default_skin_canvas_width")]
    pub w: u32,
    #[serde(default = "default_skin_canvas_height")]
    pub h: u32,
    #[serde(default)]
    pub fadeout: i32,
    #[serde(default)]
    pub input: i32,
    #[serde(default)]
    pub ranktime: i32,
    #[serde(default)]
    pub scene: i32,
    #[serde(default)]
    pub close: i32,
    #[serde(default)]
    pub loadstart: i32,
    #[serde(default)]
    pub loadend: i32,
    #[serde(default)]
    pub playstart: i32,
    #[serde(default = "default_judgetimer")]
    pub judgetimer: i32,
    #[serde(default)]
    pub finishmargin: i32,
    #[serde(default)]
    pub category: Vec<SkinCategoryDef>,
    #[serde(default)]
    pub property: Vec<SkinPropertyDef>,
    #[serde(default)]
    pub filepath: Vec<SkinFilepathDef>,
    #[serde(default)]
    pub offset: Vec<SkinOffsetDef>,
    #[serde(default)]
    pub source: Vec<SkinSourceDef>,
    #[serde(default)]
    pub font: Vec<SkinFontDef>,
    #[serde(default)]
    pub image: Vec<SkinImageDef>,
    #[serde(default)]
    pub imageset: Vec<SkinImageSetDef>,
    #[serde(default)]
    pub value: Vec<SkinValueDef>,
    #[serde(default)]
    pub text: Vec<SkinTextDef>,
    /// BMZ extension: texture-free solid-color panels.
    #[serde(default)]
    pub panel: Vec<SkinPanelDef>,
    #[serde(default)]
    pub slider: Vec<SkinSliderDef>,
    #[serde(default)]
    pub graph: Vec<SkinGraphDef>,
    #[serde(default, rename = "hiddenCover")]
    pub hidden_cover: Vec<SkinHiddenCoverDef>,
    #[serde(default, rename = "liftCover", deserialize_with = "deserialize_lift_cover_defs")]
    pub lift_cover: Vec<SkinHiddenCoverDef>,
    #[serde(default, rename = "hiterrorvisualizer")]
    pub hiterror_visualizer: Vec<SkinHitErrorVisualizerDef>,
    #[serde(default)]
    pub timingvisualizer: Vec<SkinTimingVisualizerDef>,
    #[serde(default)]
    pub timingdistributiongraph: Vec<SkinTimingDistributionGraphDef>,
    #[serde(default)]
    pub gaugegraph: Vec<SkinGaugeGraphDef>,
    #[serde(default)]
    pub judgegraph: Vec<SkinJudgeGraphDef>,
    #[serde(default)]
    pub bpmgraph: Vec<SkinBpmGraphDef>,
    pub note: Option<SkinNoteSetDef>,
    pub gauge: Option<SkinGaugeDef>,
    #[serde(default)]
    pub gauges: Vec<SkinGaugeDef>,
    #[serde(default)]
    pub judge: Vec<SkinJudgeDef>,
    #[serde(default)]
    pub pmchara: Vec<SkinPmCharaDef>,
    pub bga: Option<SkinBgaDef>,
    /// beatoraja play skin の Practice 設定表示領域。
    ///
    /// 表示内容自体は app 側 egui が担うが、同じ id の destination 座標を
    /// 初期ウィンドウ位置として利用する。
    pub practice: Option<SkinPracticeDef>,
    pub songlist: Option<SkinSongListDef>,
    #[serde(default)]
    pub destination: Vec<DestinationListEntry>,
    /// Lua `timer_util.timer_observe_boolean` から変換された動的タイマー定義。
    #[serde(default, rename = "dynamicTimer")]
    pub dynamic_timers: Vec<SkinDynamicTimerDef>,
    /// Lua `customTimers` のうち、既存タイマー開始時刻へ固定 delay を加える定義。
    #[serde(default, rename = "fixedDelayTimer")]
    pub fixed_delay_timers: Vec<SkinFixedDelayTimerDef>,
    /// Lua skin callback をロード時に変換した、初期値を持つ内部フラグ。
    #[serde(default, rename = "runtimeFlag")]
    pub runtime_flags: Vec<SkinRuntimeFlagDef>,
    /// Lua skin callback をロード時に変換した、内部フラグのトグルイベント。
    #[serde(default, rename = "runtimeEvent")]
    pub runtime_events: Vec<SkinRuntimeEventDef>,
    /// Lua のロード中に呼ばれた `main_state.audio_*` をシーン開始時の命令へ変換したもの。
    #[serde(default, rename = "sceneAudio")]
    pub scene_audio: Vec<SkinAudioActionDef>,
    /// Lua `customEvents` のうち、タイマー開始を条件とする宣言的な音声イベント。
    #[serde(default, rename = "customEvents")]
    pub custom_events: Vec<SkinCustomEventDef>,
    /// Lua Result スキンがロード時に選んだ展開パネル。
    ///
    /// Lua の `Expand_op` / `result_mode` を認識した場合に設定され、
    /// 0=非表示、1=IR、2=グラフとして Result 入力と描画状態を同期する。
    #[serde(default, rename = "resultPanelDefault")]
    pub result_panel_default: Option<i32>,
    /// BMZ Result IR の標準 ref を global / 現在選択中 scope のどちらへ束縛するか。
    #[serde(default, rename = "resultIrScopeBinding")]
    pub result_ir_scope_binding: IrScopeBinding,
    /// BMZ Result IR の scope を切り替える入力。未指定なら操作を追加しない。
    #[serde(default, rename = "resultIrScopeToggle")]
    pub result_ir_scope_toggle: ResultIrScopeToggle,
    /// BMZ Select IR の標準 ref を global / 現在選択中 scope のどちらへ束縛するか。
    #[serde(default, rename = "selectIrScopeBinding")]
    pub select_ir_scope_binding: IrScopeBinding,
    /// BMZ Select IR の scope を切り替える入力。未指定なら操作を追加しない。
    #[serde(default, rename = "selectIrScopeToggle")]
    pub select_ir_scope_toggle: SelectIrScopeToggle,
    /// ユーザがスキン設定パネルで選んだオプションから算出した有効 op コード列。
    /// `Some` のときレンダー時の `enabled_options()` はこれを返し、`None` の
    /// ときは従来通り `property.def` (または各 property の先頭 item) を既定として
    /// 計算する。
    #[serde(skip)]
    pub user_selected_options: Option<Vec<i32>>,
    /// LR2 `#SETOPTION` など、設定 UI に出さず内部的に有効化する op。
    #[serde(skip, default)]
    pub internal_enabled_options: Vec<i32>,
    /// プレイ描画時のみ plan 側が設定する judgegraph 密度。
    #[serde(skip, default)]
    pub play_judge_graph_density: Vec<u8>,
    /// プレイ描画時のみ plan 側が設定する bpmgraph 線分。
    #[serde(skip, default)]
    pub play_bpm_graph_segments: Vec<BpmGraphSegment>,
    /// リザルト描画時のみ plan 側が設定する gaugegraph 推移。
    #[serde(skip, default)]
    pub result_gauge_graph_points: Vec<ResultGaugeGraphPoint>,
    /// リザルト描画時のみ plan 側が設定する timing graph 推移。
    #[serde(skip, default)]
    pub result_timing_points: Vec<ResultTimingPoint>,
    /// リザルト描画時のみ plan 側が設定する judgegraph(type=1) 用の秒別 state 集計。
    #[serde(skip, default)]
    pub result_judge_graph_buckets: Vec<ResultJudgeGraphBucket>,
    /// リザルト描画時のみ judgegraph(type=0) 用のノーツ種別別集計。
    #[serde(skip, default)]
    pub result_note_graph_buckets: Vec<ResultNoteGraphBucket>,
    /// リザルト描画時のみ plan 側が設定する judgegraph(type=2) 用の FAST/SLOW 秒別集計。
    #[serde(skip, default)]
    pub result_early_late_graph_buckets: Vec<ResultEarlyLateGraphBucket>,
    /// リザルト描画時のみ plan 側が設定する timingdistributiongraph 用の固定分布。
    #[serde(skip, default)]
    pub result_timing_distribution: ResultTimingDistribution,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SkinPracticeDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default = "default_practice_visible_items", rename = "visibleItems")]
    pub visible_items: i32,
}

fn default_practice_visible_items() -> i32 {
    10
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinSongListDef {
    /// LR2 child coordinates are relative to the bottom-left of each bar.
    #[serde(default, rename = "lr2BottomOrigin")]
    pub lr2_bottom_origin: bool,
    #[serde(default)]
    pub flash: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub center: i32,
    #[serde(default)]
    pub clickable: Vec<i32>,
    #[serde(default)]
    pub listoff: Vec<DestinationListEntry>,
    #[serde(default)]
    pub liston: Vec<DestinationListEntry>,
    #[serde(default)]
    pub text: Vec<DestinationListEntry>,
    #[serde(default)]
    pub level: Vec<DestinationListEntry>,
    #[serde(default)]
    pub lamp: Vec<DestinationListEntry>,
    #[serde(default)]
    pub playerlamp: Vec<DestinationListEntry>,
    #[serde(default)]
    pub rivallamp: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub trophy: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub graph: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub label: Vec<DestinationListEntry>,
    #[serde(default)]
    pub judgegraph: Vec<DestinationListEntry>,
    #[serde(default)]
    pub bpmgraph: Vec<DestinationListEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinCategoryDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinPropertyDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<SkinPropertyItemDef>,
    #[serde(default)]
    pub def: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinPropertyItemDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub op: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinFilepathDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub def: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinOffsetDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub x: bool,
    #[serde(default)]
    pub y: bool,
    #[serde(default)]
    pub w: bool,
    #[serde(default)]
    pub h: bool,
    #[serde(default)]
    pub r: bool,
    #[serde(default)]
    pub a: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinSourceDef {
    #[serde(default, rename = "lr2ColorKey")]
    pub lr2_color_key: Option<[u8; 3]>,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinFontDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "type")]
    pub font_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinImageDef {
    #[serde(default, rename = "lr2Panel")]
    pub lr2_panel: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub len: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default)]
    pub click: i32,
    #[serde(default)]
    pub act: Option<i32>,
    /// `act` を状態参照に使いつつ、クリックイベントは無効にする画像向け。
    /// 未指定時は従来どおり `act` の有無をクリック可否として扱う。
    #[serde(default)]
    pub clickable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinImageSetDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    /// IntegerProperty; `ref` above belongs to the separate image-index family.
    #[serde(default)]
    pub value: Option<SkinPropertyValue>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub images: Vec<String>,
    #[serde(default)]
    pub click: i32,
    #[serde(default)]
    pub act: Option<i32>,
    #[serde(default)]
    pub clickable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct SkinValueDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub align: i32,
    /// LR2 CSV の judge combo だけが持つ alignment 解釈。
    /// 未指定の JSON/Lua judge は beatoraja と同じく中央寄せにする。
    #[serde(default, rename = "judgeAlign")]
    pub judge_align: Option<i32>,
    #[serde(default)]
    pub digit: i32,
    #[serde(default)]
    pub padding: i32,
    #[serde(default)]
    pub zeropadding: i32,
    #[serde(default)]
    pub space: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default)]
    pub value: Option<SkinPropertyValue>,
    #[serde(default)]
    pub expr: String,
    /// Lua `value = function()` から変換した浮動小数 digit 式。空なら `expr` / `ref` を使う。
    #[serde(default)]
    pub value_expr: String,
    #[serde(default)]
    pub offset: Vec<SkinValueDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinLr2SystemFontDef {
    pub size: i32,
    #[serde(default)]
    pub thickness: i32,
    #[serde(default, rename = "type")]
    pub font_type: i32,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct SkinTextDef {
    #[serde(default, rename = "lr2SystemFont")]
    pub lr2_system_font: Option<SkinLr2SystemFontDef>,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub font: String,
    #[serde(default)]
    pub size: i32,
    #[serde(default)]
    pub align: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default)]
    pub value: Option<SkinPropertyValue>,
    #[serde(default, rename = "constantText", deserialize_with = "deserialize_skin_string")]
    pub constant_text: String,
    /// BMZ extension: render a numeric skin ref with the text renderer.
    /// beatoraja-compatible value sprites remain supported; this is used by the bundled
    /// default JSON skin to avoid shipping a separate digit atlas.
    #[serde(default, rename = "numberRef")]
    pub number_ref: Option<i32>,
    /// BMZ extension: render the latest judgement text for a judge region.
    /// Region 0 corresponds to the normal 1P judgement area.
    #[serde(default, rename = "judgeRegion")]
    pub judge_region: Option<usize>,
    /// BMZ extension: color `judgeRegion` text by judgement category.
    #[serde(default, rename = "judgeColor")]
    pub judge_color: bool,
    /// BMZ extension: render FAST/SLOW text for a judge region.
    #[serde(default, rename = "judgeTimingRegion")]
    pub judge_timing_region: Option<usize>,
    /// BMZ extension: color `judgeTimingRegion` text by FAST/SLOW side.
    #[serde(default, rename = "judgeTimingColor")]
    pub judge_timing_color: bool,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub wrapping: bool,
    #[serde(default)]
    pub overflow: i32,
    /// BMZ extension: controls how `overflow = 1` shrinks text.
    /// 0 (default) preserves height and shrinks only width like beatoraja;
    /// 1 shrinks both axes uniformly like legacy BMZ rendering.
    #[serde(default, rename = "shrinkMode")]
    pub shrink_mode: i32,
    #[serde(default, rename = "outlineColor")]
    pub outline_color: String,
    #[serde(default, rename = "outlineWidth")]
    pub outline_width: f32,
    #[serde(default, rename = "shadowColor")]
    pub shadow_color: String,
    #[serde(default, rename = "shadowOffsetX")]
    pub shadow_offset_x: f32,
    #[serde(default, rename = "shadowOffsetY")]
    pub shadow_offset_y: f32,
    #[serde(default, rename = "shadowSmoothness")]
    pub shadow_smoothness: f32,
    /// Lua `value = function()` から変換したコース表テキスト式。空なら `ref` を使う。
    #[serde(default)]
    pub value_expr: String,
}

/// BMZ extension for building skin panels without raster assets.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct SkinPanelDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    /// Fill color in RRGGBB or RRGGBBAA form.
    #[serde(default)]
    pub color: String,
    /// Optional border color in RRGGBB or RRGGBBAA form.
    #[serde(default, rename = "borderColor")]
    pub border_color: String,
    /// Border width in skin-canvas pixels.
    #[serde(default, rename = "borderWidth")]
    pub border_width: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinSliderDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub angle: i32,
    #[serde(default)]
    pub range: i32,
    #[serde(default, rename = "type")]
    pub slider_type: i32,
    #[serde(default = "default_true")]
    pub changeable: bool,
    #[serde(default, rename = "isRefNum", deserialize_with = "deserialize_skin_bool")]
    pub is_ref_num: bool,
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
    /// Lua `value = function()` から変換した slider 進捗式 (0.0–1.0)。空なら `type` を使う。
    #[serde(default)]
    pub value_expr: String,
    /// beatoraja JSON FloatProperty uses the Rate family.
    #[serde(default)]
    pub value: Option<SkinPropertyValue>,
}

/// beatoraja `judgegraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinJudgeGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub graph_type: i32,
    #[serde(default, rename = "type")]
    pub type_alias: i32,
    #[serde(default, rename = "backTexOff")]
    pub back_tex_off: i32,
    #[serde(default)]
    pub delay: i32,
    #[serde(default, rename = "orderReverse")]
    pub order_reverse: i32,
    #[serde(default, rename = "noGap")]
    pub no_gap: i32,
    #[serde(default, rename = "noGapX")]
    pub no_gap_x: i32,
}

impl SkinJudgeGraphDef {
    pub fn graph_type(&self) -> i32 {
        if self.graph_type != 0 { self.graph_type } else { self.type_alias }
    }
}

/// beatoraja `gaugegraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinGaugeGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub color: Vec<String>,
    #[serde(default = "default_gaugegraph_assist_clear_bg_color", rename = "assistClearBGColor")]
    pub assist_clear_bg_color: String,
    #[serde(
        default = "default_gaugegraph_assist_easy_fail_bg_color",
        rename = "assistAndEasyFailBGColor"
    )]
    pub assist_and_easy_fail_bg_color: String,
    #[serde(default = "default_gaugegraph_groove_fail_bg_color", rename = "grooveFailBGColor")]
    pub groove_fail_bg_color: String,
    #[serde(
        default = "default_gaugegraph_groove_clear_hard_bg_color",
        rename = "grooveClearAndHardBGColor"
    )]
    pub groove_clear_and_hard_bg_color: String,
    #[serde(default = "default_gaugegraph_exhard_bg_color", rename = "exHardBGColor")]
    pub ex_hard_bg_color: String,
    #[serde(default = "default_gaugegraph_hazard_bg_color", rename = "hazardBGColor")]
    pub hazard_bg_color: String,
    #[serde(
        default = "default_gaugegraph_assist_clear_line_color",
        rename = "assistClearLineColor"
    )]
    pub assist_clear_line_color: String,
    #[serde(
        default = "default_gaugegraph_assist_easy_fail_line_color",
        rename = "assistAndEasyFailLineColor"
    )]
    pub assist_and_easy_fail_line_color: String,
    #[serde(default = "default_gaugegraph_groove_fail_line_color", rename = "grooveFailLineColor")]
    pub groove_fail_line_color: String,
    #[serde(
        default = "default_gaugegraph_groove_clear_hard_line_color",
        rename = "grooveClearAndHardLineColor"
    )]
    pub groove_clear_and_hard_line_color: String,
    #[serde(default = "default_gaugegraph_exhard_line_color", rename = "exHardLineColor")]
    pub ex_hard_line_color: String,
    #[serde(default = "default_gaugegraph_hazard_line_color", rename = "hazardLineColor")]
    pub hazard_line_color: String,
    #[serde(default = "default_gaugegraph_borderline_color", rename = "borderlineColor")]
    pub borderline_color: String,
    #[serde(default = "default_gaugegraph_border_color", rename = "borderColor")]
    pub border_color: String,
}

/// beatoraja `bpmgraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinBpmGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub delay: i32,
    #[serde(default, rename = "lineWidth")]
    pub line_width: i32,
    #[serde(default, rename = "mainBPMColor")]
    pub main_bpm_color: String,
    #[serde(default, rename = "minBPMColor")]
    pub min_bpm_color: String,
    #[serde(default, rename = "maxBPMColor")]
    pub max_bpm_color: String,
    #[serde(default, rename = "otherBPMColor")]
    pub other_bpm_color: String,
    #[serde(default, rename = "stopLineColor")]
    pub stop_line_color: String,
    #[serde(default, rename = "transitionLineColor")]
    pub transition_line_color: String,
}
