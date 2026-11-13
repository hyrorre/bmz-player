use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::judge::Judge;
use bmz_core::lane::KeyMode;

use crate::rule::RuleMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaugeAutoShiftMode {
    #[default]
    Off,
    Continue,
    HardToGroove,
    BestClear,
    SelectToUnder,
}

/// beatoraja `GaugeProperty` 相当。キーモード別の段位ゲージ係数を選ぶ。
/// グルーヴ系ゲージ (AssistEasy..Hazard) は本実装では全プロパティ共通だが、
/// CLASS / EXCLASS / EXHARDCLASS は beatoraja の各キーモード値を移植する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaugeProperty {
    FiveKeys,
    #[default]
    SevenKeys,
    /// pop'n music 系（9K）。通常は K9 chart から選び、
    /// `CourseGaugeConstraint::Keys9` から明示される場合もある。
    Pms,
    /// keyboard mania 系（24K）。コース定義側から指定された場合のみ使う。
    Keyboard,
    /// LR2 互換。コース側で `gauge_lr2` 指定時に明示的に使う。
    Lr2,
}

impl GaugeProperty {
    /// チャートの `KeyMode` から beatoraja 既定の `GaugeProperty` を決める。
    /// `BMSPlayerRule.Beatoraja_5/7/9` 準拠：
    /// - K5 / K10 → FiveKeys (BEAT_5K / BEAT_10K)
    /// - K7 / K14 → SevenKeys (BEAT_7K / BEAT_14K)
    /// - K9 → Pms (POPN_9K)
    /// - K4 / K6 / K8 は beatoraja に対応モードがないため、`Beatoraja_Other`
    ///   と同じ SevenKeys にフォールバック（Qwilight 系の派生キーモード）。
    ///
    /// KEYBOARD はチャート由来では選ばれず、コース定義側からのみ来る。
    pub fn from_keymode(key_mode: KeyMode) -> Self {
        match key_mode {
            KeyMode::K5 | KeyMode::K10 => Self::FiveKeys,
            KeyMode::K9 => Self::Pms,
            KeyMode::K4 | KeyMode::K6 | KeyMode::K7 | KeyMode::K8 | KeyMode::K14 => Self::SevenKeys,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeModifier {
    None,
    Total,
    LimitIncrement,
    ModifyDamage,
    Iidx,
    Pop,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeJudgeIndex {
    Pg = 0,
    Gr = 1,
    Gd = 2,
    Bd = 3,
    Pr = 4,
    Epr = 5,
}

#[derive(Debug, Clone)]
pub struct GaugeDefinition {
    pub gauge_type: GaugeType,
    pub clear_type: Option<ClearType>,
    pub modifier: GaugeModifier,
    pub min: f32,
    pub max: f32,
    pub init: f32,
    pub border: f32,
    pub death: f32,
    pub values: [f32; 6],
    pub guts: &'static [(f32, f32)],
}

#[derive(Debug, Clone)]
pub struct GaugeRuntimeDefinition {
    pub gauge_type: GaugeType,
    pub clear_type: Option<ClearType>,
    pub min: f32,
    pub max: f32,
    pub init: f32,
    pub border: f32,
    pub death: f32,
    pub values: [f32; 6],
    pub guts: &'static [(f32, f32)],
}

#[derive(Debug, Clone)]
pub struct SingleGaugeState {
    pub definition: GaugeRuntimeDefinition,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaugeCarryValue {
    pub gauge_type: GaugeType,
    pub value: f32,
}

#[derive(Debug, Clone)]
pub struct GaugeState {
    pub selected: GaugeType,
    pub original: GaugeType,
    pub auto_shift: bool,
    pub auto_shift_mode: GaugeAutoShiftMode,
    pub bottom_shiftable_gauge: GaugeType,
    pub gauges: Vec<SingleGaugeState>,
}

impl GaugeState {
    /// 既定の SevenKeys プロパティでゲージ状態を作る（テストや旧呼び出し用）。
    pub fn new(selected: GaugeType, total: f64, total_notes: u32) -> Self {
        Self::new_with_property(selected, total, total_notes, GaugeProperty::default())
    }

    /// 指定 `GaugeProperty` でゲージ状態を作る。キーモードに応じた段位ゲージ値を引く。
    pub fn new_with_property(
        selected: GaugeType,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
    ) -> Self {
        Self::new_with_property_and_rule_mode(
            selected,
            total,
            total_notes,
            property,
            RuleMode::Beatoraja,
        )
    }

    pub fn new_with_property_and_rule_mode(
        selected: GaugeType,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
        rule_mode: RuleMode,
    ) -> Self {
        Self::new_with_property_and_rule_mode_and_keymode(
            selected,
            total,
            total_notes,
            property,
            rule_mode,
            KeyMode::K7,
        )
    }

    pub fn new_with_property_and_rule_mode_and_keymode(
        selected: GaugeType,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
        rule_mode: RuleMode,
        key_mode: KeyMode,
    ) -> Self {
        let gauges = gauge_definitions_for_rule_mode_and_keymode(property, rule_mode, key_mode)
            .into_iter()
            .map(|definition| {
                let definition = compile_gauge_definition(&definition, total, total_notes);
                SingleGaugeState { value: definition.init, definition }
            })
            .collect();

        Self {
            selected,
            original: selected,
            auto_shift: false,
            auto_shift_mode: GaugeAutoShiftMode::Off,
            bottom_shiftable_gauge: GaugeType::AssistEasy,
            gauges,
        }
    }

    pub fn new_auto_shift(total: f64, total_notes: u32) -> Self {
        Self::new_with_auto_shift(
            GaugeType::Hazard,
            GaugeAutoShiftMode::BestClear,
            total,
            total_notes,
        )
    }

    pub fn new_with_auto_shift(
        selected: GaugeType,
        mode: GaugeAutoShiftMode,
        total: f64,
        total_notes: u32,
    ) -> Self {
        Self::new_with_auto_shift_property(
            selected,
            mode,
            total,
            total_notes,
            GaugeProperty::default(),
        )
    }

    pub fn new_with_auto_shift_property(
        selected: GaugeType,
        mode: GaugeAutoShiftMode,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
    ) -> Self {
        Self::new_with_auto_shift_property_and_rule_mode(
            selected,
            mode,
            total,
            total_notes,
            property,
            RuleMode::Beatoraja,
        )
    }

    pub fn new_with_auto_shift_property_and_rule_mode(
        selected: GaugeType,
        mode: GaugeAutoShiftMode,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
        rule_mode: RuleMode,
    ) -> Self {
        Self::new_with_auto_shift_property_and_rule_mode_and_keymode(
            selected,
            mode,
            total,
            total_notes,
            property,
            rule_mode,
            KeyMode::K7,
        )
    }

    pub fn new_with_auto_shift_property_and_rule_mode_and_keymode(
        selected: GaugeType,
        mode: GaugeAutoShiftMode,
        total: f64,
        total_notes: u32,
        property: GaugeProperty,
        rule_mode: RuleMode,
        key_mode: KeyMode,
    ) -> Self {
        let start = match mode {
            GaugeAutoShiftMode::BestClear => {
                if is_course_gauge(selected) {
                    GaugeType::ExHardClass
                } else {
                    GaugeType::Hazard
                }
            }
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove
            | GaugeAutoShiftMode::SelectToUnder => selected,
        };
        let mut state = Self::new_with_property_and_rule_mode_and_keymode(
            start,
            total,
            total_notes,
            property,
            rule_mode,
            key_mode,
        );
        state.original = selected;
        state.auto_shift = mode != GaugeAutoShiftMode::Off;
        state.auto_shift_mode = mode;
        state
    }

    pub fn set_bottom_shiftable_gauge(&mut self, gauge_type: GaugeType) {
        self.bottom_shiftable_gauge = normalize_bottom_shiftable_gauge(gauge_type);
    }

    /// Overrides every gauge's starting value with `value`, clamped to
    /// `[min, max]`.  Used to carry the gauge over between charts in a
    /// course (beatoraja keeps the gauge between songs).
    pub fn set_initial_value(&mut self, value: f32) {
        for gauge in &mut self.gauges {
            gauge.value = value.clamp(gauge.definition.min, gauge.definition.max);
        }
    }

    pub fn carry_values(&self) -> Vec<GaugeCarryValue> {
        self.gauges
            .iter()
            .map(|gauge| GaugeCarryValue {
                gauge_type: gauge.definition.gauge_type,
                value: gauge.value,
            })
            .collect()
    }

    pub fn set_initial_values(&mut self, values: &[GaugeCarryValue]) {
        for value in values {
            if let Some(gauge) =
                self.gauges.iter_mut().find(|gauge| gauge.definition.gauge_type == value.gauge_type)
            {
                gauge.value = value.value.clamp(gauge.definition.min, gauge.definition.max);
            }
        }
        self.auto_shift_if_needed();
    }

    pub fn current(&self) -> &SingleGaugeState {
        let selected =
            self.gauges.iter().find(|gauge| gauge.definition.gauge_type == self.selected);
        debug_assert!(selected.is_some(), "selected gauge {:?} must exist", self.selected);
        // rule mode の定義列に selected が無くてもプレイ中に panic せず、
        // 先頭定義へフォールバックする。定義列は常に非空。
        selected.or_else(|| self.gauges.first()).expect("gauge definitions must not be empty")
    }

    pub fn current_clear_type(&self) -> Option<ClearType> {
        self.current().definition.clear_type
    }

    pub fn current_closes_play_on_zero(&self) -> bool {
        self.current().value <= self.current().definition.min
            && self.auto_shift_mode == GaugeAutoShiftMode::Off
            && gauge_closes_play_on_zero(self.current().definition.gauge_type)
    }

    pub fn result_gauge(&self) -> &SingleGaugeState {
        if matches!(
            self.auto_shift_mode,
            GaugeAutoShiftMode::BestClear | GaugeAutoShiftMode::SelectToUnder
        ) {
            self.best_auto_shift_clear_gauge().unwrap_or_else(|| self.current())
        } else {
            self.current()
        }
    }

    pub fn apply_judge(&mut self, judge: Judge, rate: f32) {
        let index = GaugeJudgeIndex::from(judge);
        for gauge in &mut self.gauges {
            gauge.apply(index, rate);
        }
        self.auto_shift_if_needed();
    }

    /// Mine ノーツを踏んだときの直接ダメージ適用（beatoraja 準拠で
    /// gauge から `damage` を引く）。コンボ/スコアには影響しない。
    pub fn apply_mine(&mut self, damage: f64) {
        for gauge in &mut self.gauges {
            gauge.apply_mine(damage);
        }
        self.auto_shift_if_needed();
    }

    /// Direct recovery affects every living gauge; it cannot revive a failed gauge.
    pub fn apply_potion(&mut self, recovery: f64) {
        for gauge in &mut self.gauges {
            gauge.apply_potion(recovery);
        }
    }

    /// HCN 押下中のゲージ増加 1 tick。beatoraja `JudgeManager` は
    /// `mpassingcount` が +200ms を超えるたびに GREAT を rate 0.5 で適用する。
    pub fn apply_hcn_hold(&mut self) {
        self.apply_judge(Judge::Great, 0.5);
    }

    /// HCN 早離し中のゲージ減衰 1 tick。beatoraja `JudgeManager` は
    /// `mpassingcount` が -200ms を下回るたびに BAD を rate 0.5 で適用する。
    pub fn apply_hcn_drain(&mut self) {
        self.apply_judge(Judge::Bad, 0.5);
    }

    fn best_auto_shift_clear_gauge(&self) -> Option<&SingleGaugeState> {
        let order = auto_shift_result_order(self.original);
        let bottom_rank = auto_shift_result_rank(self.original)
            .min(auto_shift_result_rank(self.bottom_shiftable_gauge));
        order.iter().find_map(|gauge_type| {
            if auto_shift_result_rank(*gauge_type) < bottom_rank {
                return None;
            }
            if self.auto_shift_mode == GaugeAutoShiftMode::SelectToUnder
                && auto_shift_result_rank(*gauge_type) > auto_shift_result_rank(self.original)
            {
                return None;
            }
            self.gauge(*gauge_type).and_then(|gauge| gauge.is_qualified().then_some(gauge))
        })
    }

    fn auto_shift_if_needed(&mut self) {
        match self.auto_shift_mode {
            GaugeAutoShiftMode::Off | GaugeAutoShiftMode::Continue => {}
            GaugeAutoShiftMode::HardToGroove => {
                if self.current().value <= self.current().definition.min
                    && gauge_closes_play_on_zero(self.selected)
                    && !is_course_gauge(self.selected)
                {
                    self.selected = GaugeType::Normal;
                }
            }
            GaugeAutoShiftMode::BestClear | GaugeAutoShiftMode::SelectToUnder => {
                self.selected = self.best_current_auto_shift_gauge();
            }
        }
    }

    fn best_current_auto_shift_gauge(&self) -> GaugeType {
        let top_rank = match self.auto_shift_mode {
            GaugeAutoShiftMode::BestClear => {
                if is_course_gauge(self.original) {
                    auto_shift_result_rank(GaugeType::ExHardClass)
                } else {
                    auto_shift_result_rank(GaugeType::Hazard)
                }
            }
            GaugeAutoShiftMode::SelectToUnder => auto_shift_result_rank(self.original),
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove => auto_shift_result_rank(self.selected),
        };
        let current_rank = auto_shift_result_rank(self.selected);
        let bottom_rank = auto_shift_result_rank(self.bottom_shiftable_gauge);
        let start_rank = current_rank.min(bottom_rank);
        let order = auto_shift_result_order(self.original);

        order
            .iter()
            .copied()
            .filter(|gauge_type| {
                let rank = auto_shift_result_rank(*gauge_type);
                rank >= start_rank && rank <= top_rank
            })
            .find(|gauge_type| {
                self.gauge(*gauge_type)
                    .is_some_and(|gauge| gauge.value > gauge.definition.min && gauge.is_qualified())
            })
            .unwrap_or_else(|| lowest_auto_shift_gauge(order, start_rank, top_rank))
    }

    fn gauge(&self, gauge_type: GaugeType) -> Option<&SingleGaugeState> {
        self.gauges.iter().find(|gauge| gauge.definition.gauge_type == gauge_type)
    }
}

mod compile;
mod definitions;
mod definitions_common;
mod definitions_dx;
mod state;
mod totals;

pub use compile::*;
pub use definitions::*;
use definitions_common::*;
use definitions_dx::*;
use state::*;
pub use totals::*;

#[cfg(test)]
#[path = "gauge/tests.rs"]
mod tests;
