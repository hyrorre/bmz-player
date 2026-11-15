use super::schema_visualizers::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinTextureId(pub u32);

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinDocument {
    /// Adapter-only, immutable layout recipe. Live edits reuse installed assets.
    #[serde(skip)]
    pub qwilight_layout: Option<std::sync::Arc<QwilightLayout>>,
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
    /// BMZ play extension: screen-space black-to-clear entry fade in milliseconds.
    /// Scene elapsed time is used; seamless retries skip the fade.
    #[serde(default, rename = "bmzPlayFadein")]
    pub bmz_play_fadein: i32,
    /// Screen-space exit fade when the skin has no timer 2/3 animation.
    /// The host still uses `fadeout` / `close` for the transition wait.
    #[serde(default, rename = "bmzPlayFadeout")]
    pub bmz_play_fadeout: i32,
    /// BMZ play extension: a cleared finish without a full combo keeps Play until
    /// the BMZ result-settled timer animation ends, then starts the exit fade.
    /// Other skins start the fade after `finishmargin`, as in beatoraja.
    #[serde(default, rename = "bmzPlayClearCompletion")]
    pub bmz_play_clear_completion: bool,
    /// BMZ play extension: preview window for upcoming BPM changes.
    #[serde(default, rename = "bmzBpmChangePreview")]
    pub bmz_bpm_change_preview: Option<SkinBpmChangePreviewDef>,
    /// BMZ select extension: how the skin presents the ESC hold-to-exit progress.
    #[serde(default, rename = "bmzExitHold")]
    pub bmz_exit_hold: Option<SkinExitHoldDef>,
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
    /// BMZ extension: display text by UI locale code, keyed by property name.
    /// Saved selections keep using the language-independent names.
    #[serde(default, rename = "propertyLabels")]
    pub property_labels: HashMap<String, SkinPropertyLabels>,
    #[serde(default)]
    pub filepath: Vec<SkinFilepathDef>,
    #[serde(default)]
    pub offset: Vec<SkinOffsetDef>,
    #[serde(default)]
    pub source: Vec<SkinSourceDef>,
    /// Image sequences assembled by format adapters, keyed by source ID.
    #[serde(default, rename = "sourceFrames")]
    pub source_frames: HashMap<String, SkinSourceFramesDef>,
    /// Format-adapter frames kept compressed on the CPU and uploaded on demand.
    #[serde(default, rename = "deferredSources")]
    pub deferred_sources: Vec<String>,
    #[serde(default)]
    pub font: Vec<SkinFontDef>,
    #[serde(default, rename = "lr2SystemFonts")]
    pub lr2_system_fonts: Vec<SkinLr2SystemFontDef>,
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
    /// Qwilight play overlays driven by BMZ timing, judgment history and output PCM.
    #[serde(default, rename = "qwilightVisualizer")]
    pub qwilight_visualizer: Vec<SkinQwilightVisualizerDef>,
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
    /// キーロガーの表示列と key-on timer の対応。空ならキーモードのレーン順を使う。
    #[serde(default, rename = "keyloggerLane")]
    pub keylogger_lanes: Vec<SkinKeyloggerLaneDef>,
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

/// BMZ play extension declared by skins that preview upcoming BPM changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinBpmChangePreviewDef {
    /// Real-time milliseconds before a BPM change when the preview starts.
    #[serde(default = "default_bpm_change_preview_lead_ms", rename = "leadMs")]
    pub lead_ms: i32,
}

impl SkinBpmChangePreviewDef {
    pub fn clamped_lead_ms(self) -> i32 {
        self.lead_ms.clamp(0, SKIN_BMZ_BPM_CHANGE_PREVIEW_MAX_LEAD_MS)
    }
}

fn default_bpm_change_preview_lead_ms() -> i32 {
    2_000
}

/// BMZ select extension declared by skins that draw their own ESC hold-to-exit progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinExitHoldDef {
    /// Whether BMZ still draws its built-in indicator over the skin.
    #[serde(default = "default_exit_hold_builtin")]
    pub builtin: bool,
}

fn default_exit_hold_builtin() -> bool {
    true
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinSongListDef {
    /// LR2 child coordinates are relative to the bottom-left of each bar.
    pub lr2_bottom_origin: bool,
    pub flash: Vec<DestinationListEntry>,
    pub id: String,
    pub center: i32,
    pub clickable: Vec<i32>,
    pub listoff: Vec<DestinationListEntry>,
    pub liston: Vec<DestinationListEntry>,
    pub text: Vec<DestinationListEntry>,
    pub level: Vec<DestinationListEntry>,
    pub lamp: Vec<DestinationListEntry>,
    pub playerlamp: Vec<DestinationListEntry>,
    pub rivallamp: Vec<DestinationListEntry>,
    /// Native LR2 BAR_RIVAL slots; 0..3 are win, lose, draw, and no player score.
    pub lr2rival: Vec<DestinationListEntry>,
    pub trophy: Vec<DestinationListEntry>,
    pub graph: Vec<DestinationListEntry>,
    pub label: Vec<DestinationListEntry>,
    pub judgegraph: Vec<DestinationListEntry>,
    pub bpmgraph: Vec<DestinationListEntry>,
    pub scorerank: Vec<SkinSongListFieldDef>,
    pub exscore: Vec<SkinSongListFieldDef>,
    pub scorerate: Vec<SkinSongListFieldDef>,
    pub minbp: Vec<SkinSongListFieldDef>,
    pub mincb: Vec<SkinSongListFieldDef>,
    pub maxcombo: Vec<SkinSongListFieldDef>,
    pub nextrank: Vec<SkinSongListFieldDef>,
    pub nextrankdiff: Vec<SkinSongListFieldDef>,
    pub nearestrank: Vec<SkinSongListFieldDef>,
    pub nearestrankdiff: Vec<SkinSongListFieldDef>,
    pub rivalscorerank: Vec<SkinSongListFieldDef>,
    pub rivaldiff: Vec<SkinSongListFieldDef>,
    pub favorite: Vec<SkinSongListFieldDef>,
    /// Source declaration order. Lua skins can supply this explicitly.
    pub score_order: Vec<SongListField>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinCategoryDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SongListField {
    #[serde(rename = "scorerank")]
    Scorerank,
    #[serde(rename = "exscore")]
    Exscore,
    #[serde(rename = "scorerate")]
    Scorerate,
    #[serde(rename = "minbp")]
    Minbp,
    #[serde(rename = "mincb")]
    Mincb,
    #[serde(rename = "maxcombo")]
    Maxcombo,
    #[serde(rename = "nextrank")]
    Nextrank,
    #[serde(rename = "nextrankdiff")]
    Nextrankdiff,
    #[serde(rename = "nearestrank")]
    Nearestrank,
    #[serde(rename = "nearestrankdiff")]
    Nearestrankdiff,
    #[serde(rename = "rivalscorerank")]
    Rivalscorerank,
    #[serde(rename = "rivaldiff")]
    Rivaldiff,
    #[serde(rename = "favorite")]
    Favorite,
}

impl SkinSongListDef {
    pub fn all_score_fields(&self) -> impl Iterator<Item = &SkinSongListFieldDef> {
        [
            &self.scorerank,
            &self.exscore,
            &self.scorerate,
            &self.minbp,
            &self.mincb,
            &self.maxcombo,
            &self.nextrank,
            &self.nextrankdiff,
            &self.nearestrank,
            &self.nearestrankdiff,
            &self.rivalscorerank,
            &self.rivaldiff,
            &self.favorite,
        ]
        .into_iter()
        .flatten()
    }

    pub fn all_score_fields_mut(&mut self) -> impl Iterator<Item = &mut SkinSongListFieldDef> {
        [
            &mut self.scorerank,
            &mut self.exscore,
            &mut self.scorerate,
            &mut self.minbp,
            &mut self.mincb,
            &mut self.maxcombo,
            &mut self.nextrank,
            &mut self.nextrankdiff,
            &mut self.nearestrank,
            &mut self.nearestrankdiff,
            &mut self.rivalscorerank,
            &mut self.rivaldiff,
            &mut self.favorite,
        ]
        .into_iter()
        .flatten()
    }

    pub fn score_fields(&self, field: SongListField) -> &[SkinSongListFieldDef] {
        match field {
            SongListField::Scorerank => &self.scorerank,
            SongListField::Exscore => &self.exscore,
            SongListField::Scorerate => &self.scorerate,
            SongListField::Minbp => &self.minbp,
            SongListField::Mincb => &self.mincb,
            SongListField::Maxcombo => &self.maxcombo,
            SongListField::Nextrank => &self.nextrank,
            SongListField::Nextrankdiff => &self.nextrankdiff,
            SongListField::Nearestrank => &self.nearestrank,
            SongListField::Nearestrankdiff => &self.nearestrankdiff,
            SongListField::Rivalscorerank => &self.rivalscorerank,
            SongListField::Rivaldiff => &self.rivaldiff,
            SongListField::Favorite => &self.favorite,
        }
    }
}

impl<'de> Deserialize<'de> for SkinSongListDef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{MapAccess, Visitor};
        struct SongListVisitor;
        impl<'de> Visitor<'de> for SongListVisitor {
            type Value = SkinSongListDef;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a songlist object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                struct Id(#[serde(deserialize_with = "deserialize_skin_id")] String);
                #[derive(Deserialize)]
                struct Entries(
                    #[serde(deserialize_with = "deserialize_destination_entries")]
                    Vec<DestinationListEntry>,
                );
                let mut list = SkinSongListDef::default();
                let mut explicit_order: Option<Vec<SongListField>> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => list.id = map.next_value::<Id>()?.0,
                        "lr2BottomOrigin" => list.lr2_bottom_origin = map.next_value()?,
                        "flash" => list.flash = map.next_value()?,
                        "center" => list.center = map.next_value()?,
                        "clickable" => list.clickable = map.next_value()?,
                        "listoff" => list.listoff = map.next_value()?,
                        "liston" => list.liston = map.next_value()?,
                        "text" => list.text = map.next_value()?,
                        "level" => list.level = map.next_value()?,
                        "lamp" => list.lamp = map.next_value()?,
                        "playerlamp" => list.playerlamp = map.next_value()?,
                        "rivallamp" => list.rivallamp = map.next_value()?,
                        "lr2rival" => list.lr2rival = map.next_value()?,
                        "judgegraph" => list.judgegraph = map.next_value()?,
                        "bpmgraph" => list.bpmgraph = map.next_value()?,
                        "trophy" => list.trophy = map.next_value::<Entries>()?.0,
                        "graph" => list.graph = map.next_value::<Entries>()?.0,
                        "label" => list.label = map.next_value::<Entries>()?.0,
                        "scoreOrder" => {
                            explicit_order = Some(deserialize_songlist_score_order::<M::Error>(
                                map.next_value()?,
                            )?);
                        }
                        "scorerank" => {
                            list.score_order.push(SongListField::Scorerank);
                            list.scorerank =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "exscore" => {
                            list.score_order.push(SongListField::Exscore);
                            list.exscore =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "scorerate" => {
                            list.score_order.push(SongListField::Scorerate);
                            list.scorerate =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "minbp" => {
                            list.score_order.push(SongListField::Minbp);
                            list.minbp =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "mincb" => {
                            list.score_order.push(SongListField::Mincb);
                            list.mincb =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "maxcombo" => {
                            list.score_order.push(SongListField::Maxcombo);
                            list.maxcombo =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "nextrank" => {
                            list.score_order.push(SongListField::Nextrank);
                            list.nextrank =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "nextrankdiff" => {
                            list.score_order.push(SongListField::Nextrankdiff);
                            list.nextrankdiff =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "nearestrank" => {
                            list.score_order.push(SongListField::Nearestrank);
                            list.nearestrank =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "nearestrankdiff" => {
                            list.score_order.push(SongListField::Nearestrankdiff);
                            list.nearestrankdiff =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "rivalscorerank" => {
                            list.score_order.push(SongListField::Rivalscorerank);
                            list.rivalscorerank =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "rivaldiff" => {
                            list.score_order.push(SongListField::Rivaldiff);
                            list.rivaldiff =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        "favorite" => {
                            list.score_order.push(SongListField::Favorite);
                            list.favorite =
                                deserialize_songlist_fields::<M::Error>(map.next_value()?, &key)?;
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                if let Some(order) = explicit_order {
                    let mut complete = Vec::new();
                    for field in order.into_iter().chain(list.score_order) {
                        if !complete.contains(&field) {
                            complete.push(field);
                        }
                    }
                    list.score_order = complete;
                }
                Ok(list)
            }
        }
        deserializer.deserialize_map(SongListVisitor)
    }
}

fn deserialize_songlist_entry<T: serde::de::DeserializeOwned, E: serde::de::Error>(
    value: JsonValue,
    path: &str,
) -> Result<T, E> {
    serde_json::from_value(value).map_err(|error| E::custom(format!("{path}: {error}")))
}

fn deserialize_songlist_fields<E: serde::de::Error>(
    value: JsonValue,
    field: &str,
) -> Result<Vec<SkinSongListFieldDef>, E> {
    let path = format!("songlist.{field}");
    match value {
        JsonValue::Object(_) => deserialize_songlist_entry(value, &path).map(|entry| vec![entry]),
        JsonValue::Array(entries) => entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| deserialize_songlist_entry(entry, &format!("{path}[{index}]")))
            .collect(),
        _ => Err(E::custom(format!("{path}: expected an object or array"))),
    }
}

fn deserialize_songlist_score_order<E: serde::de::Error>(
    value: JsonValue,
) -> Result<Vec<SongListField>, E> {
    let JsonValue::Array(entries) = value else {
        return Err(E::custom("songlist.scoreOrder: expected an array"));
    };
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            deserialize_songlist_entry(entry, &format!("songlist.scoreOrder[{index}]"))
        })
        .collect()
}

/// Inline materials use the same definitions as root text/value objects.
/// The destination is flattened so op/draw/timer/dst retain their usual meaning.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinSongListFieldDef {
    #[serde(flatten)]
    pub render: SongListFieldRender,
    #[serde(flatten)]
    pub destination: SkinDestinationDef,
    #[serde(default, deserialize_with = "deserialize_songlist_table")]
    pub format: SongListFieldFormat,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum SongListFieldRender {
    Image {
        #[serde(deserialize_with = "deserialize_skin_id_vec")]
        images: Vec<String>,
    },
    Text {
        #[serde(default, deserialize_with = "deserialize_songlist_table")]
        text: SkinTextDef,
        #[serde(default)]
        labels: Vec<String>,
    },
    Digits {
        #[serde(deserialize_with = "deserialize_songlist_table")]
        digits: SkinValueDef,
        #[serde(default, deserialize_with = "deserialize_songlist_table")]
        glyphs: SongListDigitGlyphs,
    },
}

/// Lua's empty table is represented as an empty JSON array by the converter.
fn deserialize_songlist_table<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = JsonValue::deserialize(deserializer)?;
    if value.as_array().is_some_and(Vec::is_empty) {
        return Ok(T::default());
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SongListFieldFormat {
    /// None chooses 2 for scorerate, 0 otherwise; bounded to 6 at evaluation.
    #[serde(default)]
    pub decimals: Option<u8>,
    #[serde(default, rename = "showPlus")]
    pub show_plus: bool,
    /// None chooses true for scorerate, false otherwise.
    #[serde(default)]
    pub percent: Option<bool>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
}

/// Cell indices inside one number frame. Missing required glyph hides the
/// entire value, never just the sign or decimal point.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SongListDigitGlyphs {
    #[serde(default)]
    pub plus: Option<u32>,
    #[serde(default)]
    pub minus: Option<u32>,
    #[serde(default)]
    pub decimal: Option<u32>,
    #[serde(default)]
    pub percent: Option<u32>,
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

/// Locale code (`ja`, `en`, ...) to display text for one property and its items.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SkinPropertyLabels {
    #[serde(default)]
    pub name: HashMap<String, String>,
    /// Item name to its localized display text.
    #[serde(default)]
    pub items: HashMap<String, HashMap<String, String>>,
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
pub struct SkinSourceFramesDef {
    pub paths: Vec<String>,
    pub columns: u32,
}

/// Qwilight animation timing; sources may repeat to hold sparse frames.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct SkinQwilightAnimation {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub fps: u32,
    #[serde(default, rename = "cycleMs")]
    pub cycle_ms: i32,
    #[serde(default)]
    pub lane: usize,
    #[serde(default)]
    pub looping: i32,
    #[serde(default, rename = "loopFrame")]
    pub loop_frame: usize,
    #[serde(default)]
    pub enlarge: i32,
    #[serde(default)]
    pub system: i32,
    #[serde(default)]
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinFontDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "type")]
    pub font_type: i32,
    /// beatoraja `JsonSkin.Font.fallback`。主フォントに無い文字を先頭から順に探す。
    #[serde(default, deserialize_with = "deserialize_font_fallbacks")]
    pub fallback: Vec<SkinFontFallbackDef>,
}

/// beatoraja `JsonSkin.FontFallback`。パスはフォント定義と同じ規則で解決する。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkinFontFallbackDef {
    pub path: String,
    pub font_type: i32,
}

/// beatoraja `FontFallbackSerializer` と同じく、文字列・`{path,type}`・`{value}` を受け付ける。
/// Lua変換で単一の値になった場合も1件として扱い、パスの無い要素は無視する。
fn deserialize_font_fallbacks<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<SkinFontFallbackDef>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Path(String),
        Object {
            #[serde(default)]
            path: Option<String>,
            #[serde(default)]
            value: Option<String>,
            #[serde(default, rename = "type")]
            font_type: Option<f64>,
        },
        Other(serde::de::IgnoredAny),
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entries {
        Many(Vec<Entry>),
        One(Entry),
    }

    let entries = match Entries::deserialize(deserializer)? {
        Entries::Many(entries) => entries,
        Entries::One(entry) => vec![entry],
    };
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Path(path) => Some(SkinFontFallbackDef { path, font_type: 0 }),
            Entry::Object { path, value, font_type } => path.or(value).map(|path| {
                SkinFontFallbackDef { path, font_type: font_type.unwrap_or(0.0) as i32 }
            }),
            Entry::Other(_) => None,
        })
        .filter(|fallback| !fallback.path.is_empty())
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinImageDef {
    #[serde(default, rename = "qwilightAnimation")]
    pub qwilight_animation: Option<SkinQwilightAnimation>,
    /// Qwilight conversion: atlas cell of each logical frame. Sparse frames hold
    /// the previous image, so each distinct image is stored once. Empty means
    /// frame `i` is cell `i`.
    #[serde(default, rename = "qwilightFrameCells")]
    pub qwilight_frame_cells: Vec<u16>,
    /// LR2 buttons select a flat cell, independent of atlas orientation.
    #[serde(default, rename = "lr2Button")]
    pub lr2_button: bool,
    #[serde(default, rename = "lr2Panel")]
    pub lr2_panel: Option<i32>,
    /// LR2 SRC: LR2 shortens a rectangle that leaves the image to the image,
    /// and drawing stretches the rest. beatoraja keeps the requested rectangle and samples the
    /// edge pixels beyond the image instead.
    #[serde(default, rename = "lr2Clip")]
    pub lr2_clip: bool,
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
    #[serde(default, rename = "qwilightAnimation")]
    pub qwilight_animation: Option<SkinQwilightAnimation>,
    /// LR2 CSV numbers use consecutive 10/11/24-cell frames rather than rows.
    #[serde(default, rename = "lr2Number")]
    pub lr2_number: bool,
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
    #[serde(default, deserialize_with = "deserialize_skin_bool_or_string")]
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
    /// BMZ extension: gradient start color of the text body in RRGGBB or RRGGBBAA form.
    /// With `colorBottom`, the body changes from this color to that color.
    #[serde(default, rename = "colorTop")]
    pub color_top: String,
    /// BMZ extension: gradient end color of the text body in RRGGBB or RRGGBBAA form.
    #[serde(default, rename = "colorBottom")]
    pub color_bottom: String,
    /// BMZ extension: gradient direction in degrees. 0 runs from top to bottom and positive
    /// values turn it counter-clockwise like destination `angle` (90 runs left to right).
    #[serde(default, rename = "gradientAngle")]
    pub gradient_angle: f32,
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
    /// FloatWriter ID, or an encoded Lua slider event callback.
    #[serde(default)]
    pub event: Option<i32>,
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
    #[serde(default = "default_judgegraph_delay")]
    pub delay: i32,
    #[serde(default, rename = "orderReverse")]
    pub order_reverse: i32,
    #[serde(default, rename = "noGap")]
    pub no_gap: i32,
    #[serde(default, rename = "noGapX")]
    pub no_gap_x: i32,
}

/// beatoraja `JsonSkin.JudgeGraph.delay`.
fn default_judgegraph_delay() -> i32 {
    500
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
