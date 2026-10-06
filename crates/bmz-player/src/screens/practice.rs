//! Practice mode configuration (beatoraja `PracticeProperty` subset).

use std::path::PathBuf;

use anyhow::Result;
use bmz_chart::model::{JudgeRankKind, JudgeRankSpec, PlayableChart};
use bmz_chart::practice::apply_practice_section;
use bmz_core::clear::GaugeType;
use bmz_core::lane::KeyMode;
use bmz_core::time::TimeUs;
use bmz_gameplay::gauge::{GaugeProperty, GaugeState};
use bmz_gameplay::judge::window::judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode;
use bmz_gameplay::rule::RuleMode;
use bmz_render::snapshot::ResultGraphSnapshot;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::profile_config::GaugeTypeConfig;
use crate::paths::ProfilePaths;
use crate::select_options::ArrangeOption;

mod persistence;

const PRACTICE_PROPERTY_FORMAT_VERSION: u32 = 1;
const PRACTICE_PLAYBACK_RATE_MIN: u16 = 50;
const PRACTICE_PLAYBACK_RATE_MAX: u16 = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PracticeGaugeType {
    AssistEasy,
    Easy,
    #[default]
    Normal,
    Hard,
    ExHard,
    Hazard,
    Class,
    ExClass,
    ExHardClass,
    /// Compatibility with a practice file written before full gauge support.
    AutoShift,
}

impl PracticeGaugeType {
    pub const VALUES: [Self; 9] = [
        Self::AssistEasy,
        Self::Easy,
        Self::Normal,
        Self::Hard,
        Self::ExHard,
        Self::Hazard,
        Self::Class,
        Self::ExClass,
        Self::ExHardClass,
    ];

    pub const fn gauge_type(self) -> GaugeType {
        match self {
            Self::AssistEasy => GaugeType::AssistEasy,
            Self::Easy => GaugeType::Easy,
            Self::Normal => GaugeType::Normal,
            Self::Hard => GaugeType::Hard,
            Self::ExHard => GaugeType::ExHard,
            Self::Hazard => GaugeType::Hazard,
            Self::Class => GaugeType::Class,
            Self::ExClass => GaugeType::ExClass,
            Self::ExHardClass => GaugeType::ExHardClass,
            Self::AutoShift => GaugeType::ExHard,
        }
    }

    pub const fn scales_section_total(self) -> bool {
        matches!(self, Self::AssistEasy | Self::Easy | Self::Normal)
    }
}

impl From<GaugeTypeConfig> for PracticeGaugeType {
    fn from(value: GaugeTypeConfig) -> Self {
        match value {
            GaugeTypeConfig::AssistEasy => Self::AssistEasy,
            GaugeTypeConfig::Easy => Self::Easy,
            GaugeTypeConfig::Normal => Self::Normal,
            GaugeTypeConfig::Hard => Self::Hard,
            GaugeTypeConfig::ExHard => Self::ExHard,
            GaugeTypeConfig::Hazard => Self::Hazard,
            GaugeTypeConfig::AutoShift => Self::AutoShift,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeGraphType {
    #[default]
    NoteType,
    Judge,
    EarlyLate,
}

/// Persisted / editable practice settings for one chart (SHA-256).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PracticeProperty {
    #[serde(default)]
    pub format_version: u32,
    pub start_time_ms: u32,
    pub end_time_ms: u32,
    pub gauge: PracticeGaugeType,
    #[serde(default)]
    pub gauge_category: Option<GaugeProperty>,
    pub start_gauge: u32,
    pub judgerank: i32,
    pub arrange: ArrangeOption,
    #[serde(default)]
    pub arrange_2p: ArrangeOption,
    #[serde(default)]
    pub dp_flip: bool,
    pub total: Option<f64>,
    #[serde(default = "default_playback_rate_percent")]
    pub playback_rate_percent: u16,
    #[serde(default)]
    pub graph_type: PracticeGraphType,
}

impl Default for PracticeProperty {
    fn default() -> Self {
        Self {
            format_version: PRACTICE_PROPERTY_FORMAT_VERSION,
            start_time_ms: 0,
            end_time_ms: 10_000,
            gauge: PracticeGaugeType::Normal,
            gauge_category: None,
            start_gauge: 20,
            judgerank: 100,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            dp_flip: false,
            total: None,
            playback_rate_percent: 100,
            graph_type: PracticeGraphType::NoteType,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PracticeRuleContext {
    pub rule_mode: RuleMode,
    pub key_mode: KeyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PracticeGaugeBounds {
    pub initial: u32,
    pub max: u32,
}

impl PracticeRuleContext {
    pub fn for_play_session(
        rule_mode: RuleMode,
        chart_key_mode: KeyMode,
        options: &crate::screens::play_session::PlaySessionOptions,
    ) -> Self {
        Self {
            rule_mode,
            key_mode: crate::screens::play_session::effective_primary_key_mode(
                chart_key_mode,
                options,
            ),
        }
    }

    pub fn is_dx(self) -> bool {
        self.rule_mode == RuleMode::Dx
    }

    pub fn gauge_bounds(self, property: &PracticeProperty) -> PracticeGaugeBounds {
        let category =
            property.gauge_category.unwrap_or_else(|| GaugeProperty::from_keymode(self.key_mode));
        let definition = bmz_gameplay::gauge::gauge_definitions_for_rule_mode_and_keymode(
            category,
            self.rule_mode,
            self.key_mode,
        )
        .into_iter()
        .find(|definition| definition.gauge_type == property.gauge.gauge_type())
        .expect("practice gauge must have a gameplay definition");
        let max = definition.max.round().max(1.0) as u32;
        PracticeGaugeBounds { initial: (definition.init.round() as u32).clamp(1, max), max }
    }

    pub fn clamp_start_gauge(self, property: &mut PracticeProperty) {
        property.start_gauge = property.start_gauge.clamp(1, self.gauge_bounds(property).max);
    }

    pub fn set_gauge(self, property: &mut PracticeProperty, gauge: PracticeGaugeType) {
        if property.gauge == gauge {
            return;
        }
        property.gauge = gauge;
        if self.is_dx() {
            property.start_gauge = self.gauge_bounds(property).initial;
        } else {
            self.clamp_start_gauge(property);
        }
    }

    pub fn set_gauge_category(self, property: &mut PracticeProperty, category: GaugeProperty) {
        if self.is_dx() || property.gauge_category == Some(category) {
            return;
        }
        property.gauge_category = Some(category);
        property.start_gauge = self.gauge_bounds(property).initial;
    }

    pub fn field_is_fixed(self, field: usize) -> bool {
        self.is_dx() && matches!(field, 3 | 5 | 6)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PracticePhase {
    /// Settings overlay; chart is preloaded but not playing.
    Config,
    /// Active section play.
    Playing,
}

#[derive(Debug, Clone)]
pub struct PracticeSession {
    pub chart_id: i64,
    pub chart_title: String,
    pub chart_sha256: [u8; 32],
    pub property: PracticeProperty,
    pub rules: PracticeRuleContext,
    pub phase: PracticePhase,
    pub max_end_time_ms: u32,
    pub last_graph: Arc<ResultGraphSnapshot>,
    /// Absolute chart time represented by graph bucket zero.
    pub graph_start_time_ms: u32,
    pub is_double: bool,
    pub cursor: usize,
    /// `last_play_snapshot` に反映済みの設定中プレビュー時刻。
    pub preview_time_ms: Option<u32>,
    /// KEY4 の相手選択から PRACTICE を開始した場合に、各練習ラウンドへ引き継ぐ相手。
    pub battle_target: Option<crate::screens::play_start::BattleTarget>,
}

/// CLI-only overrides applied when entering practice from the command line.
#[derive(Debug, Clone, Default)]
pub struct PracticeCliOverrides {
    pub start_time_ms: Option<u32>,
    pub end_time_ms: Option<u32>,
}

pub fn practice_property_path(profile_paths: &ProfilePaths, chart_sha256: &[u8; 32]) -> PathBuf {
    profile_paths.root_dir.join("practice").join(format!("{}.json", sha256_hex(chart_sha256)))
}

pub fn load_practice_property(
    profile_paths: &ProfilePaths,
    chart_sha256: &[u8; 32],
    chart: &PlayableChart,
    profile_gauge: GaugeTypeConfig,
    rules: PracticeRuleContext,
    cli: &PracticeCliOverrides,
) -> Result<PracticeProperty> {
    let path = practice_property_path(profile_paths, chart_sha256);
    let saved = persistence::load(&path);
    let loaded_from_file = saved.is_some();
    let mut property = saved.unwrap_or_default();

    if !loaded_from_file {
        property.end_time_ms = default_end_time_ms(chart);
        property.judgerank = practice_judgerank_percent(chart, rules.rule_mode);
        if profile_gauge != GaugeTypeConfig::AutoShift {
            property.gauge = profile_gauge.into();
        }
    } else {
        migrate_legacy_practice_property(&mut property, chart, rules.rule_mode);
    }
    property.gauge_category.get_or_insert_with(|| GaugeProperty::from_keymode(rules.key_mode));
    if !loaded_from_file && rules.is_dx() {
        property.start_gauge = rules.gauge_bounds(&property).initial;
    }
    if property.total.is_none() {
        property.total = chart.metadata.total;
    }

    if let Some(start) = cli.start_time_ms {
        property.start_time_ms = start;
    }
    if let Some(end) = cli.end_time_ms {
        property.end_time_ms = end;
    }
    clamp_practice_property(&mut property, chart, rules);

    Ok(property)
}

fn practice_judgerank_percent(chart: &PlayableChart, rule_mode: RuleMode) -> i32 {
    judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
        chart.metadata.judge_rank_spec,
        chart.metadata.key_mode,
        rule_mode,
    )
}

fn migrate_legacy_practice_property(
    property: &mut PracticeProperty,
    chart: &PlayableChart,
    rule_mode: RuleMode,
) {
    if property.format_version >= PRACTICE_PROPERTY_FORMAT_VERSION {
        return;
    }

    // BMZ の旧形式は初回値に BMS の生 #RANK / #DEFEXRANK を保存し、
    // 次回開始時に BMSON の倍率 (%) として再解釈していた。譜面由来の
    // 旧初期値と一致する場合だけ移行し、ユーザーが変更した値は保持する。
    if let Some(spec) = chart.metadata.judge_rank_spec
        && matches!(spec.kind, JudgeRankKind::BmsRank | JudgeRankKind::DefExRank)
        && property.judgerank == spec.value.clamp(1, 400)
    {
        property.judgerank = practice_judgerank_percent(chart, rule_mode);
    }
    property.format_version = PRACTICE_PROPERTY_FORMAT_VERSION;
}

pub fn save_practice_property(
    profile_paths: &ProfilePaths,
    chart_sha256: &[u8; 32],
    property: &PracticeProperty,
) -> Result<()> {
    let path = practice_property_path(profile_paths, chart_sha256);
    persistence::save(&path, property)
}

pub fn apply_practice_property(chart: &mut PlayableChart, property: &PracticeProperty) {
    let start_us = TimeUs(i64::from(property.start_time_ms) * 1000);
    let end_ms = property.end_time_ms.max(property.start_time_ms.saturating_add(1000));
    let end_us = TimeUs(i64::from(end_ms) * 1000);
    let audio_start_us = TimeUs(start_us.0.saturating_sub(1_000_000).max(0));
    if let Some(total) = property.total {
        chart.metadata.total = Some(total);
    }
    apply_practice_section(chart, start_us, end_us);
    // beatoraja starts background keysounds at `starttime - 1s` and skips
    // earlier events. Retaining them would make the scheduler catch up every
    // sound before the practice range when entering midway through a chart.
    chart.bgm_events.retain(|event| event.time >= audio_start_us);
    chart.metadata.judge_rank = Some(property.judgerank);
    chart.metadata.judge_rank_spec =
        Some(JudgeRankSpec { value: property.judgerank, kind: JudgeRankKind::BmsonJudgeRank });
    if !property.gauge.scales_section_total()
        && let Some(total) = property.total
    {
        chart.metadata.total = Some(total);
    }
}

pub fn apply_practice_start_gauge(gauge: &mut GaugeState, start_gauge: u32) {
    // GAS can start on Hazard while retaining a POP gauge with a 120 maximum.
    // GaugeState clamps each member to its own definition, not the selected one.
    gauge.set_initial_value(start_gauge.max(1) as f32);
}

pub fn practice_chart_zero_time(property: &PracticeProperty, skin_playstart_us: TimeUs) -> TimeUs {
    let lead_us = i64::from(property.start_time_ms.saturating_sub(1000)) * 1000;
    // `skin_playstart_us` is the normal negative READY offset. The audio clock
    // advances at the selected rate, so compensate the fixed wall-clock READY
    // duration and arrive at `lead_us` exactly when the play timer starts.
    let ready_wall_us = skin_playstart_us.0.saturating_neg().max(0);
    let ready_chart_us = ((i128::from(ready_wall_us) * i128::from(property.playback_rate_percent))
        / 100)
        .min(i128::from(i64::MAX)) as i64;
    TimeUs(lead_us.saturating_sub(ready_chart_us))
}

pub fn clamp_practice_property(
    property: &mut PracticeProperty,
    chart: &PlayableChart,
    rules: PracticeRuleContext,
) {
    let max_end = default_end_time_ms(chart);
    property.start_time_ms = property.start_time_ms.min(max_end.saturating_sub(3000));
    property.end_time_ms =
        property.end_time_ms.clamp(property.start_time_ms.saturating_add(1000), max_end);
    property.judgerank = property.judgerank.clamp(1, 400);
    rules.clamp_start_gauge(property);
    property.playback_rate_percent = property
        .playback_rate_percent
        .clamp(PRACTICE_PLAYBACK_RATE_MIN, PRACTICE_PLAYBACK_RATE_MAX);
    if let Some(total) = property.total.as_mut() {
        *total = total.clamp(10.0, 5000.0);
    }
}

pub fn practice_field_count(is_double: bool) -> usize {
    if is_double { 12 } else { 10 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PracticeCursorTarget {
    Field(usize),
    Start,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PracticeCursorAction {
    None,
    Start,
    Leave,
}

pub fn practice_cursor_count(is_double: bool) -> usize {
    practice_field_count(is_double) + 2
}

pub fn practice_start_cursor(is_double: bool) -> usize {
    practice_field_count(is_double)
}

pub fn practice_leave_cursor(is_double: bool) -> usize {
    practice_field_count(is_double) + 1
}

pub fn practice_cursor_target(cursor: usize, is_double: bool) -> PracticeCursorTarget {
    let field_count = practice_field_count(is_double);
    match cursor % practice_cursor_count(is_double) {
        index if index < field_count => PracticeCursorTarget::Field(index),
        index if index == field_count => PracticeCursorTarget::Start,
        _ => PracticeCursorTarget::Leave,
    }
}

pub fn move_practice_cursor(cursor: &mut usize, is_double: bool, forward: bool) {
    let count = practice_cursor_count(is_double);
    *cursor = (*cursor + if forward { 1 } else { count - 1 }) % count;
}

pub fn apply_practice_cursor_horizontal(
    property: &mut PracticeProperty,
    cursor: usize,
    is_double: bool,
    increment: bool,
    max_end_time_ms: u32,
    rules: PracticeRuleContext,
) -> PracticeCursorAction {
    match practice_cursor_target(cursor, is_double) {
        PracticeCursorTarget::Field(field) => {
            adjust_practice_selected_field(
                property,
                field,
                is_double,
                increment,
                max_end_time_ms,
                rules,
            );
            PracticeCursorAction::None
        }
        PracticeCursorTarget::Start if increment => PracticeCursorAction::Start,
        PracticeCursorTarget::Leave if increment => PracticeCursorAction::Leave,
        PracticeCursorTarget::Start | PracticeCursorTarget::Leave => PracticeCursorAction::None,
    }
}

pub fn adjust_practice_selected_field(
    property: &mut PracticeProperty,
    cursor: usize,
    is_double: bool,
    increment: bool,
    max_end_time_ms: u32,
    rules: PracticeRuleContext,
) {
    if rules.field_is_fixed(cursor) {
        return;
    }
    let direction = if increment { 1_i32 } else { -1 };
    match cursor {
        0 => {
            adjust_u32(
                &mut property.start_time_ms,
                direction * 100,
                0,
                max_end_time_ms.saturating_sub(3000),
            );
            property.end_time_ms =
                property.end_time_ms.max(property.start_time_ms.saturating_add(1000));
        }
        1 => adjust_u32(
            &mut property.end_time_ms,
            direction * 100,
            property.start_time_ms.saturating_add(1000),
            max_end_time_ms,
        ),
        2 => {
            let mut gauge = property.gauge;
            cycle_gauge(&mut gauge, increment);
            rules.set_gauge(property, gauge);
        }
        3 => {
            let mut category = property.gauge_category;
            cycle_gauge_category(&mut category, increment);
            rules.set_gauge_category(property, category.unwrap_or_default());
        }
        4 => {
            let max = rules.gauge_bounds(property).max;
            adjust_u32(&mut property.start_gauge, direction, 1, max);
        }
        5 => property.judgerank = (property.judgerank + direction).clamp(1, 400),
        6 => {
            if let Some(total) = property.total.as_mut() {
                *total = (*total + f64::from(direction) * 5.0).clamp(10.0, 5000.0);
            }
        }
        7 => {
            let value = i32::from(property.playback_rate_percent) + direction * 5;
            property.playback_rate_percent = value
                .clamp(i32::from(PRACTICE_PLAYBACK_RATE_MIN), i32::from(PRACTICE_PLAYBACK_RATE_MAX))
                as u16;
        }
        8 => cycle_graph_type(&mut property.graph_type, increment),
        9 => cycle_arrange(&mut property.arrange, increment),
        10 if is_double => cycle_arrange(&mut property.arrange_2p, increment),
        11 if is_double => property.dp_flip = !property.dp_flip,
        _ => {}
    }
}

fn adjust_u32(value: &mut u32, delta: i32, min: u32, max: u32) {
    *value = (i64::from(*value) + i64::from(delta)).clamp(i64::from(min), i64::from(max)) as u32;
}

fn cycle_gauge(value: &mut PracticeGaugeType, increment: bool) {
    let index = PracticeGaugeType::VALUES.iter().position(|item| item == value).unwrap_or(0);
    let len = PracticeGaugeType::VALUES.len();
    *value = PracticeGaugeType::VALUES[(index + if increment { 1 } else { len - 1 }) % len];
}

fn cycle_gauge_category(value: &mut Option<GaugeProperty>, increment: bool) {
    let values =
        [GaugeProperty::FiveKeys, GaugeProperty::SevenKeys, GaugeProperty::Pms, GaugeProperty::Lr2];
    let current = value.unwrap_or(GaugeProperty::SevenKeys);
    let index = values.iter().position(|item| *item == current).unwrap_or(0);
    *value = Some(values[(index + if increment { 1 } else { values.len() - 1 }) % values.len()]);
}

fn cycle_graph_type(value: &mut PracticeGraphType, increment: bool) {
    let values =
        [PracticeGraphType::NoteType, PracticeGraphType::Judge, PracticeGraphType::EarlyLate];
    let index = values.iter().position(|item| item == value).unwrap_or(0);
    *value = values[(index + if increment { 1 } else { values.len() - 1 }) % values.len()];
}

fn cycle_arrange(value: &mut ArrangeOption, increment: bool) {
    *value = if increment { value.cycle() } else { value.cycle_prev() };
}

const fn default_playback_rate_percent() -> u16 {
    100
}

pub fn default_end_time_ms(chart: &PlayableChart) -> u32 {
    let end_ms = (chart.end_time.0 / 1000).max(0);
    u32::try_from(end_ms).unwrap_or(u32::MAX).saturating_add(1000)
}

fn sha256_hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use bmz_core::chart::ChartIdentity;

    use super::*;
    use bmz_chart::model::ChartMetadata;

    fn empty_chart(end_ms: i64) -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [1; 32] },
            metadata: ChartMetadata {
                judge_rank: Some(150),
                judge_rank_spec: Some(JudgeRankSpec {
                    value: 150,
                    kind: JudgeRankKind::BmsonJudgeRank,
                }),
                total: Some(250.0),
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: Default::default(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(end_ms * 1000),
        }
    }

    #[test]
    fn load_practice_property_uses_chart_defaults() {
        let root = std::env::temp_dir().join(format!(
            "bmz-practice-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            collection_db: root.join("collection.db"),
            score_db: root.join("score.db"),
            network_db: root.join("network.db"),
            replay_dir: root.join("replay"),
        };
        let chart = empty_chart(120_000);
        let property = load_practice_property(
            &paths,
            &chart.identity.file_sha256,
            &chart,
            GaugeTypeConfig::Hard,
            PracticeRuleContext::default(),
            &PracticeCliOverrides { start_time_ms: Some(5000), end_time_ms: None },
        )
        .unwrap();
        assert_eq!(property.start_time_ms, 5000);
        assert_eq!(property.end_time_ms, 121_000);
        assert_eq!(property.judgerank, 150);
        assert_eq!(property.gauge, PracticeGaugeType::Hard);
        assert_eq!(property.total, Some(250.0));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn corrupt_practice_uses_fresh_chart_and_rule_defaults_with_cli_overrides() {
        for (rule_mode, key_mode, gauge, initial) in [
            (RuleMode::Dx, KeyMode::K7, GaugeTypeConfig::Normal, 22),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Hard, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Hazard, 100),
            (RuleMode::Beatoraja, KeyMode::K7, GaugeTypeConfig::Hard, 20),
        ] {
            for end_time_ms in [None, Some(60_000)] {
                let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
                let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
                let mut chart = empty_chart(120_000);
                chart.metadata.key_mode = key_mode;
                let path = practice_property_path(&paths, &chart.identity.file_sha256);
                let rules = PracticeRuleContext { rule_mode, key_mode };
                let cli = PracticeCliOverrides { start_time_ms: Some(5000), end_time_ms };
                let load = || {
                    load_practice_property(
                        &paths,
                        &chart.identity.file_sha256,
                        &chart,
                        gauge,
                        rules,
                        &cli,
                    )
                    .unwrap()
                };
                let fresh = load();
                assert!(!path.exists(), "loading missing settings must not save defaults");
                assert_eq!(fresh.start_time_ms, 5000);
                assert_eq!(fresh.end_time_ms, end_time_ms.unwrap_or(121_000));
                assert_eq!(fresh.start_gauge, initial);
                assert_eq!(fresh.gauge, gauge.into());
                assert_eq!(fresh.total, Some(250.0));
                assert_eq!(fresh.gauge_category, Some(GaugeProperty::from_keymode(key_mode)));
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                let mut wrong_type = serde_json::to_value(&fresh).unwrap();
                wrong_type["start_gauge"] = serde_json::json!("broken");
                for original in [
                    b"null".to_vec(),
                    b"{\"start_time_ms\":".to_vec(),
                    b"[]".to_vec(),
                    b"\xff\xfe\x80".to_vec(),
                    serde_json::to_vec(&wrong_type).unwrap(),
                ] {
                    std::fs::write(&path, &original).unwrap();
                    assert_eq!(load(), fresh, "{rule_mode:?} {key_mode:?} {gauge:?}");
                    assert_eq!(std::fs::read(&path).unwrap(), original);
                    assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
                }
            }
        }
    }

    #[test]
    fn unreadable_practice_uses_fresh_defaults_without_changing_the_path() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
        let chart = empty_chart(120_000);
        let path = practice_property_path(&paths, &chart.identity.file_sha256);
        let rules = PracticeRuleContext { rule_mode: RuleMode::Dx, key_mode: KeyMode::K9 };
        let load = || {
            load_practice_property(
                &paths,
                &chart.identity.file_sha256,
                &chart,
                GaugeTypeConfig::Hard,
                rules,
                &PracticeCliOverrides { start_time_ms: Some(5000), end_time_ms: None },
            )
            .unwrap()
        };
        let fresh = load();
        std::fs::create_dir_all(&path).unwrap();
        let sentinel = path.join("keep");
        std::fs::write(&sentinel, b"original").unwrap();
        assert_eq!(load(), fresh);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"original");
    }

    #[test]
    fn valid_practice_preserves_saved_values_and_only_applies_cli_in_memory() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
        let chart = empty_chart(120_000);
        let path = practice_property_path(&paths, &chart.identity.file_sha256);
        let saved = PracticeProperty {
            start_time_ms: 10_000,
            end_time_ms: 30_000,
            start_gauge: 117,
            gauge: PracticeGaugeType::Hard,
            gauge_category: Some(GaugeProperty::Pms),
            judgerank: 222,
            total: Some(4321.0),
            ..Default::default()
        };
        save_practice_property(&paths, &chart.identity.file_sha256, &saved).unwrap();
        let original = std::fs::read(&path).unwrap();
        let rules = PracticeRuleContext { rule_mode: RuleMode::Dx, key_mode: KeyMode::K9 };
        for cli in [
            PracticeCliOverrides::default(),
            PracticeCliOverrides { start_time_ms: Some(15_000), end_time_ms: Some(40_000) },
        ] {
            let loaded = load_practice_property(
                &paths,
                &chart.identity.file_sha256,
                &chart,
                GaugeTypeConfig::Normal,
                rules,
                &cli,
            )
            .unwrap();
            let expected = PracticeProperty {
                start_time_ms: cli.start_time_ms.unwrap_or(saved.start_time_ms),
                end_time_ms: cli.end_time_ms.unwrap_or(saved.end_time_ms),
                ..saved.clone()
            };
            assert_eq!(loaded, expected);
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        }
    }

    #[test]
    fn practice_judgerank_normalizes_source_rank_kinds() {
        let mut chart = empty_chart(120_000);
        for (key_mode, spec, expected) in [
            (
                bmz_core::lane::KeyMode::K7,
                JudgeRankSpec { value: 3, kind: JudgeRankKind::BmsRank },
                100,
            ),
            (
                bmz_core::lane::KeyMode::K7,
                JudgeRankSpec { value: 2, kind: JudgeRankKind::BmsRank },
                75,
            ),
            (
                bmz_core::lane::KeyMode::K9,
                JudgeRankSpec { value: 2, kind: JudgeRankKind::BmsRank },
                70,
            ),
            (
                bmz_core::lane::KeyMode::K7,
                JudgeRankSpec { value: 125, kind: JudgeRankKind::DefExRank },
                93,
            ),
            (
                bmz_core::lane::KeyMode::K7,
                JudgeRankSpec { value: 125, kind: JudgeRankKind::BmsonJudgeRank },
                125,
            ),
        ] {
            chart.metadata.key_mode = key_mode;
            chart.metadata.judge_rank_spec = Some(spec);
            assert_eq!(practice_judgerank_percent(&chart, RuleMode::Beatoraja), expected);
        }
    }

    #[test]
    fn legacy_practice_judgerank_migrates_only_unchanged_source_default() {
        let mut chart = empty_chart(120_000);
        chart.metadata.judge_rank_spec =
            Some(JudgeRankSpec { value: 3, kind: JudgeRankKind::BmsRank });

        let mut legacy_default =
            PracticeProperty { format_version: 0, judgerank: 3, ..Default::default() };
        migrate_legacy_practice_property(&mut legacy_default, &chart, RuleMode::Beatoraja);
        assert_eq!(legacy_default.judgerank, 100);
        assert_eq!(legacy_default.format_version, PRACTICE_PROPERTY_FORMAT_VERSION);

        let mut customized =
            PracticeProperty { format_version: 0, judgerank: 2, ..Default::default() };
        migrate_legacy_practice_property(&mut customized, &chart, RuleMode::Beatoraja);
        assert_eq!(customized.judgerank, 2);
        assert_eq!(customized.format_version, PRACTICE_PROPERTY_FORMAT_VERSION);
    }

    #[test]
    fn practice_cursor_includes_start_and_leave_actions() {
        assert_eq!(practice_cursor_count(false), 12);
        assert_eq!(practice_cursor_target(9, false), PracticeCursorTarget::Field(9));
        assert_eq!(practice_cursor_target(10, false), PracticeCursorTarget::Start);
        assert_eq!(practice_cursor_target(11, false), PracticeCursorTarget::Leave);
        assert_eq!(practice_cursor_count(true), 14);
        assert_eq!(practice_cursor_target(12, true), PracticeCursorTarget::Start);
        assert_eq!(practice_cursor_target(13, true), PracticeCursorTarget::Leave);
    }

    #[test]
    fn practice_cursor_wraps_across_action_rows() {
        let mut cursor = 0;
        move_practice_cursor(&mut cursor, false, false);
        assert_eq!(practice_cursor_target(cursor, false), PracticeCursorTarget::Leave);
        move_practice_cursor(&mut cursor, false, true);
        assert_eq!(practice_cursor_target(cursor, false), PracticeCursorTarget::Field(0));
    }

    #[test]
    fn practice_action_rows_activate_only_in_the_increment_direction() {
        let mut property = PracticeProperty::default();
        let start = practice_start_cursor(false);
        let leave = practice_leave_cursor(false);

        assert_eq!(
            apply_practice_cursor_horizontal(
                &mut property,
                start,
                false,
                true,
                120_000,
                PracticeRuleContext::default()
            ),
            PracticeCursorAction::Start
        );
        assert_eq!(
            apply_practice_cursor_horizontal(
                &mut property,
                start,
                false,
                false,
                120_000,
                PracticeRuleContext::default()
            ),
            PracticeCursorAction::None
        );
        assert_eq!(
            apply_practice_cursor_horizontal(
                &mut property,
                leave,
                false,
                true,
                120_000,
                PracticeRuleContext::default()
            ),
            PracticeCursorAction::Leave
        );
        assert_eq!(
            apply_practice_cursor_horizontal(
                &mut property,
                leave,
                false,
                false,
                120_000,
                PracticeRuleContext::default()
            ),
            PracticeCursorAction::None
        );
    }

    #[test]
    fn practice_playback_rate_remains_limited_to_fifty_through_two_hundred_percent() {
        let chart = empty_chart(120_000);
        let mut slow = PracticeProperty { playback_rate_percent: 25, ..Default::default() };
        clamp_practice_property(&mut slow, &chart, PracticeRuleContext::default());
        assert_eq!(slow.playback_rate_percent, 50);

        let mut fast = PracticeProperty { playback_rate_percent: 300, ..Default::default() };
        clamp_practice_property(&mut fast, &chart, PracticeRuleContext::default());
        assert_eq!(fast.playback_rate_percent, 200);
    }

    #[test]
    fn fresh_practice_uses_dx_gauge_initial_values_without_changing_other_rules() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
        let mut chart = empty_chart(120_000);
        for (rule_mode, key_mode, gauge, expected) in [
            (RuleMode::Dx, KeyMode::K7, GaugeTypeConfig::Normal, 22),
            (RuleMode::Dx, KeyMode::K7, GaugeTypeConfig::Hard, 100),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::AssistEasy, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Easy, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Normal, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Hard, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::ExHard, 30),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::Hazard, 100),
            (RuleMode::Dx, KeyMode::K9, GaugeTypeConfig::AutoShift, 30),
            (RuleMode::Beatoraja, KeyMode::K7, GaugeTypeConfig::Hard, 20),
            (RuleMode::Lr2Oraja, KeyMode::K9, GaugeTypeConfig::Normal, 20),
        ] {
            chart.metadata.key_mode = key_mode;
            let rules = PracticeRuleContext { rule_mode, key_mode };
            let property = load_practice_property(
                &paths,
                &chart.identity.file_sha256,
                &chart,
                gauge,
                rules,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(property.start_gauge, expected, "{rule_mode:?} {key_mode:?} {gauge:?}");
        }
    }

    #[test]
    fn saved_practice_preserves_custom_values_and_clamps_only_to_actual_gauge_maximum() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
        let chart = empty_chart(120_000);
        for (rule_mode, key_mode, gauge, value, expected) in [
            (RuleMode::Dx, KeyMode::K7, PracticeGaugeType::Normal, 20, 20),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::Hard, 117, 117),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::ExHard, 150, 120),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::Hazard, 117, 100),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::Class, 117, 100),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::ExClass, 117, 100),
            (RuleMode::Dx, KeyMode::K9, PracticeGaugeType::ExHardClass, 117, 100),
            (RuleMode::Beatoraja, KeyMode::K9, PracticeGaugeType::Normal, 117, 117),
            (RuleMode::Lr2Oraja, KeyMode::K9, PracticeGaugeType::Normal, 117, 100),
        ] {
            let saved = PracticeProperty {
                gauge,
                start_gauge: value,
                gauge_category: Some(GaugeProperty::Pms),
                judgerank: 222,
                total: Some(4321.0),
                ..Default::default()
            };
            save_practice_property(&paths, &chart.identity.file_sha256, &saved).unwrap();
            let loaded = load_practice_property(
                &paths,
                &chart.identity.file_sha256,
                &chart,
                GaugeTypeConfig::Hard,
                PracticeRuleContext { rule_mode, key_mode },
                &Default::default(),
            )
            .unwrap();
            assert_eq!(loaded.start_gauge, expected, "{rule_mode:?} {key_mode:?} {gauge:?}");
            assert_eq!(loaded.gauge, saved.gauge);
            assert_eq!(loaded.gauge_category, saved.gauge_category);
            assert_eq!(loaded.judgerank, saved.judgerank);
            assert_eq!(loaded.total, saved.total);
        }
    }

    #[test]
    fn practice_gauge_selection_and_keyboard_share_dx_initial_values_and_non_dx_clamping() {
        for (rule_mode, expected) in [(RuleMode::Dx, 30), (RuleMode::Beatoraja, 100)] {
            let rules = PracticeRuleContext { rule_mode, key_mode: KeyMode::K9 };
            let mut selected = PracticeProperty {
                gauge_category: Some(GaugeProperty::Pms),
                start_gauge: 119,
                ..Default::default()
            };
            let mut keyboard = selected.clone();
            rules.set_gauge(&mut selected, PracticeGaugeType::Hard);
            adjust_practice_selected_field(&mut keyboard, 2, false, true, 120_000, rules);
            assert_eq!(selected, keyboard);
            assert_eq!(selected.start_gauge, expected);
        }
        let rules = PracticeRuleContext { rule_mode: RuleMode::Dx, key_mode: KeyMode::K9 };
        for gauge in PracticeGaugeType::VALUES {
            let mut property =
                PracticeProperty { gauge: PracticeGaugeType::AutoShift, ..Default::default() };
            rules.set_gauge(&mut property, gauge);
            let (initial, max) = if matches!(
                gauge,
                PracticeGaugeType::AssistEasy
                    | PracticeGaugeType::Easy
                    | PracticeGaugeType::Normal
                    | PracticeGaugeType::Hard
                    | PracticeGaugeType::ExHard
            ) {
                (30, 120)
            } else {
                (100, 100)
            };
            assert_eq!(property.start_gauge, initial, "{gauge:?}");
            assert_eq!(rules.gauge_bounds(&property).max, max, "{gauge:?}");
        }
    }

    #[test]
    fn practice_keyboard_uses_actual_maximum_and_keeps_dx_fixed_fields_unchanged() {
        let rules = PracticeRuleContext { rule_mode: RuleMode::Dx, key_mode: KeyMode::K9 };
        let mut property = PracticeProperty {
            start_gauge: 119,
            judgerank: 222,
            total: Some(4321.0),
            gauge_category: Some(GaugeProperty::FiveKeys),
            ..Default::default()
        };
        for _ in 0..2 {
            adjust_practice_selected_field(&mut property, 4, false, true, 120_000, rules);
            assert_eq!(property.start_gauge, 120);
        }
        adjust_practice_selected_field(&mut property, 4, false, false, 120_000, rules);
        assert_eq!(property.start_gauge, 119);
        let before = property.clone();
        for field in [3, 5, 6] {
            for increment in [true, false] {
                apply_practice_cursor_horizontal(
                    &mut property,
                    field,
                    false,
                    increment,
                    120_000,
                    rules,
                );
                assert_eq!(property, before);
            }
        }
        rules.set_gauge_category(&mut property, GaugeProperty::Pms);
        assert_eq!(property, before);
        let non_dx = PracticeRuleContext { rule_mode: RuleMode::Beatoraja, ..rules };
        non_dx.set_gauge_category(&mut property, GaugeProperty::Pms);
        assert_eq!(property.start_gauge, 30);
        assert_eq!(non_dx.gauge_bounds(&property).max, 120);
    }
}
