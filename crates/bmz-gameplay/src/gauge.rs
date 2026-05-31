use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::judge::Judge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaugeAutoShiftMode {
    #[default]
    Off,
    Continue,
    HardToGroove,
    BestClear,
    SelectToUnder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeModifier {
    None,
    Total,
    LimitIncrement,
    ModifyDamage,
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
    pub values: [f32; 6],
    pub guts: &'static [(f32, f32)],
}

#[derive(Debug, Clone)]
pub struct SingleGaugeState {
    pub definition: GaugeRuntimeDefinition,
    pub value: f32,
}

#[derive(Debug, Clone)]
pub struct GaugeState {
    pub selected: GaugeType,
    pub original: GaugeType,
    pub auto_shift: bool,
    pub auto_shift_mode: GaugeAutoShiftMode,
    pub gauges: Vec<SingleGaugeState>,
}

impl GaugeState {
    pub fn new(selected: GaugeType, total: f64, total_notes: u32) -> Self {
        let gauges = default_gauge_definitions()
            .iter()
            .map(|definition| {
                let definition = compile_gauge_definition(definition, total, total_notes);
                SingleGaugeState { value: definition.init, definition }
            })
            .collect();

        Self {
            selected,
            original: selected,
            auto_shift: false,
            auto_shift_mode: GaugeAutoShiftMode::Off,
            gauges,
        }
    }

    pub fn new_auto_shift(total: f64, total_notes: u32) -> Self {
        Self::new_with_auto_shift(
            GaugeType::ExHard,
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
        let start = match mode {
            GaugeAutoShiftMode::BestClear => GaugeType::ExHard,
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove
            | GaugeAutoShiftMode::SelectToUnder => selected,
        };
        let mut state = Self::new(start, total, total_notes);
        state.original = selected;
        state.auto_shift = mode != GaugeAutoShiftMode::Off;
        state.auto_shift_mode = mode;
        state
    }

    /// Overrides every gauge's starting value with `value`, clamped to
    /// `[min, max]`.  Used to carry the gauge over between charts in a
    /// course (beatoraja keeps the gauge between songs).
    pub fn set_initial_value(&mut self, value: f32) {
        for gauge in &mut self.gauges {
            gauge.value = value.clamp(gauge.definition.min, gauge.definition.max);
        }
    }

    pub fn current(&self) -> &SingleGaugeState {
        self.gauges
            .iter()
            .find(|gauge| gauge.definition.gauge_type == self.selected)
            .expect("selected gauge must exist")
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
    pub fn apply_mine(&mut self, damage: u16) {
        for gauge in &mut self.gauges {
            gauge.apply_mine(damage);
        }
        self.auto_shift_if_needed();
    }

    /// HCN 押下中のゲージ増加 (beatoraja 準拠の近似)。
    pub fn apply_hcn_hold(&mut self, delta_seconds: f32) {
        const RATE_PER_SEC: f32 = 6.0;
        let inc = RATE_PER_SEC * delta_seconds;
        for gauge in &mut self.gauges {
            if gauge.value > 0.0 {
                gauge.value = (gauge.value + inc).clamp(gauge.definition.min, gauge.definition.max);
            }
        }
        self.auto_shift_if_needed();
    }

    /// HCN 早離し後のゲージ減衰 (beatoraja 準拠の近似)。
    pub fn apply_hcn_drain(&mut self, delta_seconds: f32) {
        const RATE_PER_SEC: f32 = 10.0;
        let dec = RATE_PER_SEC * delta_seconds;
        for gauge in &mut self.gauges {
            if gauge.value > 0.0 {
                gauge.value = (gauge.value - dec).clamp(gauge.definition.min, gauge.definition.max);
            }
        }
        self.auto_shift_if_needed();
    }

    fn best_auto_shift_clear_gauge(&self) -> Option<&SingleGaugeState> {
        AUTO_SHIFT_RESULT_ORDER.iter().find_map(|gauge_type| {
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
                {
                    self.selected = GaugeType::Normal;
                }
            }
            GaugeAutoShiftMode::BestClear | GaugeAutoShiftMode::SelectToUnder => {
                while self.current().value <= self.current().definition.min {
                    let Some(next) = next_auto_shift_gauge(self.selected) else {
                        break;
                    };
                    self.selected = next;
                }
            }
        }
    }

    fn gauge(&self, gauge_type: GaugeType) -> Option<&SingleGaugeState> {
        self.gauges.iter().find(|gauge| gauge.definition.gauge_type == gauge_type)
    }
}

const AUTO_SHIFT_RESULT_ORDER: &[GaugeType] = &[
    GaugeType::ExHard,
    GaugeType::Hard,
    GaugeType::Normal,
    GaugeType::Easy,
    GaugeType::AssistEasy,
];

fn auto_shift_result_rank(gauge_type: GaugeType) -> u8 {
    match gauge_type {
        GaugeType::AssistEasy => 0,
        GaugeType::Easy => 1,
        GaugeType::Normal => 2,
        GaugeType::Hard => 3,
        GaugeType::ExHard => 4,
        GaugeType::Hazard => 5,
        GaugeType::Class => 6,
        GaugeType::ExClass => 7,
        GaugeType::ExHardClass => 8,
    }
}

fn next_auto_shift_gauge(current: GaugeType) -> Option<GaugeType> {
    match current {
        GaugeType::ExHard => Some(GaugeType::Hard),
        GaugeType::Hard => Some(GaugeType::Normal),
        _ => None,
    }
}

fn gauge_closes_play_on_zero(gauge_type: GaugeType) -> bool {
    matches!(
        gauge_type,
        GaugeType::Hard
            | GaugeType::ExHard
            | GaugeType::Hazard
            | GaugeType::Class
            | GaugeType::ExClass
            | GaugeType::ExHardClass
    )
}

impl SingleGaugeState {
    pub fn apply(&mut self, index: GaugeJudgeIndex, rate: f32) {
        let mut inc = self.definition.values[index as usize] * rate;

        if inc < 0.0 {
            for &(threshold, scale) in self.definition.guts {
                if self.value < threshold {
                    inc *= scale;
                    break;
                }
            }
        }

        if self.value > 0.0 {
            self.value = (self.value + inc).clamp(self.definition.min, self.definition.max);
        }
    }

    pub fn is_qualified(&self) -> bool {
        self.value > 0.0 && self.value >= self.definition.border
    }

    /// Mine 用の直接減算（beatoraja は `Gauge.addValue(-damage)` 相当）。
    /// 通常の `apply` と違って guts 補正を入れず、min..=max にだけクランプする。
    pub fn apply_mine(&mut self, damage: u16) {
        if self.value <= 0.0 {
            return;
        }
        self.value = (self.value - damage as f32).clamp(self.definition.min, self.definition.max);
    }
}

impl From<Judge> for GaugeJudgeIndex {
    fn from(value: Judge) -> Self {
        match value {
            Judge::PGreat => Self::Pg,
            Judge::Great => Self::Gr,
            Judge::Good => Self::Gd,
            Judge::Bad => Self::Bd,
            Judge::Poor => Self::Pr,
            Judge::EmptyPoor => Self::Epr,
        }
    }
}

pub fn compile_gauge_definition(
    base: &GaugeDefinition,
    total: f64,
    total_notes: u32,
) -> GaugeRuntimeDefinition {
    let mut values = base.values;
    for value in &mut values {
        *value = apply_modifier(*value, base.modifier, total, total_notes);
    }

    GaugeRuntimeDefinition {
        gauge_type: base.gauge_type,
        clear_type: base.clear_type,
        min: base.min,
        max: base.max,
        init: base.init,
        border: base.border,
        values,
        guts: base.guts,
    }
}

fn apply_modifier(value: f32, modifier: GaugeModifier, total: f64, total_notes: u32) -> f32 {
    match modifier {
        GaugeModifier::None => value,
        GaugeModifier::Total => {
            if value > 0.0 && total_notes > 0 {
                value * total as f32 / total_notes as f32
            } else {
                value
            }
        }
        GaugeModifier::LimitIncrement => {
            if value > 0.0 && total_notes > 0 {
                let pg = ((2.0 * total as f32 - 320.0) / total_notes as f32).clamp(0.0, 0.15);
                value * pg / 0.15
            } else {
                value
            }
        }
        GaugeModifier::ModifyDamage => value,
    }
}

pub fn default_gauge_definitions() -> &'static [GaugeDefinition] {
    &[
        GaugeDefinition {
            gauge_type: GaugeType::AssistEasy,
            clear_type: Some(ClearType::AssistEasy),
            modifier: GaugeModifier::Total,
            min: 0.0,
            max: 100.0,
            init: 20.0,
            border: 60.0,
            values: [0.16, 0.16, 0.0, -1.2, -2.0, -0.5],
            guts: NORMAL_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::Easy,
            clear_type: Some(ClearType::Easy),
            modifier: GaugeModifier::Total,
            min: 0.0,
            max: 100.0,
            init: 20.0,
            border: 80.0,
            values: [0.16, 0.16, 0.0, -2.0, -3.0, -1.0],
            guts: NORMAL_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::Normal,
            clear_type: Some(ClearType::Normal),
            modifier: GaugeModifier::Total,
            min: 0.0,
            max: 100.0,
            init: 20.0,
            border: 80.0,
            values: [0.16, 0.16, 0.0, -4.0, -6.0, -2.0],
            guts: NORMAL_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::Hard,
            clear_type: Some(ClearType::Hard),
            modifier: GaugeModifier::LimitIncrement,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.15, 0.15, 0.0, -5.0, -9.0, -3.0],
            guts: HARD_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::ExHard,
            clear_type: Some(ClearType::ExHard),
            modifier: GaugeModifier::LimitIncrement,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.15, 0.15, 0.0, -8.0, -18.0, -6.0],
            guts: HARD_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::Hazard,
            clear_type: None,
            modifier: GaugeModifier::None,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.0, 0.0, 0.0, -100.0, -100.0, -100.0],
            guts: &[],
        },
        // Course (段位) gauges. beatoraja `GaugeProperty.SEVENKEYS` の CLASS / EXCLASS / EXHARDCLASS。
        // 単曲プレイでは選ばれず、`apply_course_constraints` がコース時にプレイヤー選択の Gauge から
        // 6/7/8 のいずれかへマップする。
        GaugeDefinition {
            gauge_type: GaugeType::Class,
            clear_type: Some(ClearType::Normal),
            modifier: GaugeModifier::None,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.15, 0.12, 0.06, -1.5, -3.0, -1.5],
            guts: CLASS_GUTS,
        },
        GaugeDefinition {
            gauge_type: GaugeType::ExClass,
            clear_type: Some(ClearType::Hard),
            modifier: GaugeModifier::None,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.15, 0.12, 0.03, -3.0, -6.0, -3.0],
            guts: &[],
        },
        GaugeDefinition {
            gauge_type: GaugeType::ExHardClass,
            clear_type: Some(ClearType::ExHard),
            modifier: GaugeModifier::None,
            min: 0.0,
            max: 100.0,
            init: 100.0,
            border: 1.0,
            values: [0.15, 0.06, 0.0, -5.0, -10.0, -5.0],
            guts: &[],
        },
    ]
}

const NORMAL_GUTS: &[(f32, f32)] = &[(30.0, 0.5), (50.0, 0.7)];
const HARD_GUTS: &[(f32, f32)] = &[(30.0, 0.6), (50.0, 0.8)];
// beatoraja CLASS の guts テーブル（7keys）。下限近くで減衰量が弱まる救済補正。
const CLASS_GUTS: &[(f32, f32)] = &[(5.0, 0.4), (10.0, 0.5), (15.0, 0.6), (20.0, 0.7), (25.0, 0.8)];

/// beatoraja `BMSPlayerRule.calculateDefaultTotal` 相当。
pub fn default_gauge_total(total_notes: u32) -> f64 {
    let notes = total_notes as f64;
    if notes <= 0.0 {
        return 260.0;
    }
    260.0_f64.max(7.605 * notes / (0.01 * notes + 6.5))
}

/// 譜面メタの `#TOTAL` が未指定または 0 以下のとき beatoraja 既定式へフォールバックする。
pub fn gauge_total_for_chart(metadata_total: Option<f64>, total_notes: u32) -> f64 {
    metadata_total.filter(|total| *total > 0.0).unwrap_or_else(|| default_gauge_total(total_notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gauge_total_matches_beatoraja_formula() {
        assert_eq!(default_gauge_total(0), 260.0);
        assert_eq!(default_gauge_total(100), 260.0);
        let dense = default_gauge_total(2000);
        assert!(dense > 260.0);
    }

    #[test]
    fn gauge_total_for_chart_uses_metadata_when_positive() {
        assert_eq!(gauge_total_for_chart(Some(320.0), 500), 320.0);
        assert_eq!(gauge_total_for_chart(Some(0.0), 500), default_gauge_total(500));
        assert_eq!(gauge_total_for_chart(None, 500), default_gauge_total(500));
    }

    #[test]
    fn creates_selected_gauge_state_from_defaults() {
        let gauge = GaugeState::new(GaugeType::Hard, 160.0, 1000);

        assert_eq!(gauge.selected, GaugeType::Hard);
        assert_eq!(gauge.current().definition.gauge_type, GaugeType::Hard);
        assert_eq!(gauge.current().value, 100.0);
    }

    #[test]
    fn hazard_fails_on_any_damage_judge() {
        let mut gauge = GaugeState::new(GaugeType::Hazard, 160.0, 1000);

        gauge.apply_judge(Judge::Bad, 1.0);

        assert_eq!(gauge.current().value, 0.0);
        assert!(!gauge.current().is_qualified());
        assert!(gauge.current_closes_play_on_zero());
    }

    #[test]
    fn auto_shift_starts_from_exhard_and_falls_back_to_hard() {
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

        gauge.apply_judge(Judge::Poor, 6.0);

        assert!(gauge.auto_shift);
        assert_eq!(gauge.original, GaugeType::ExHard);
        assert_eq!(gauge.selected, GaugeType::Hard);
    }

    #[test]
    fn auto_shift_result_uses_highest_qualified_gauge() {
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
            .unwrap()
            .value = 0.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
            .unwrap()
            .value = 0.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Normal)
            .unwrap()
            .value = 70.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Easy)
            .unwrap()
            .value = 82.0;
        gauge.selected = GaugeType::Normal;

        let result_gauge = gauge.result_gauge();

        assert_eq!(result_gauge.definition.gauge_type, GaugeType::Easy);
    }

    #[test]
    fn continue_mode_does_not_fail_or_shift_at_zero() {
        let mut gauge = GaugeState::new_with_auto_shift(
            GaugeType::Hard,
            GaugeAutoShiftMode::Continue,
            160.0,
            1000,
        );

        gauge.apply_judge(Judge::Poor, 20.0);

        assert_eq!(gauge.selected, GaugeType::Hard);
        assert_eq!(gauge.current().value, 0.0);
        assert!(!gauge.current_closes_play_on_zero());
    }

    #[test]
    fn hard_to_groove_shifts_survival_gauge_to_normal() {
        let mut gauge = GaugeState::new_with_auto_shift(
            GaugeType::ExHard,
            GaugeAutoShiftMode::HardToGroove,
            160.0,
            1000,
        );

        gauge.apply_judge(Judge::Poor, 20.0);

        assert_eq!(gauge.selected, GaugeType::Normal);
    }

    #[test]
    fn select_to_under_result_does_not_exceed_original_gauge() {
        let mut gauge = GaugeState::new_with_auto_shift(
            GaugeType::Hard,
            GaugeAutoShiftMode::SelectToUnder,
            160.0,
            1000,
        );
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
            .unwrap()
            .value = 100.0;
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
            .unwrap()
            .value = 90.0;

        let result_gauge = gauge.result_gauge();

        assert_eq!(result_gauge.definition.gauge_type, GaugeType::Hard);
    }
}
