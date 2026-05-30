use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::judge::Judge;

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

        Self { selected, original: selected, gauges }
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

    pub fn apply_judge(&mut self, judge: Judge, rate: f32) {
        let index = GaugeJudgeIndex::from(judge);
        for gauge in &mut self.gauges {
            gauge.apply(index, rate);
        }
    }

    /// Mine ノーツを踏んだときの直接ダメージ適用（beatoraja 準拠で
    /// gauge から `damage` を引く）。コンボ/スコアには影響しない。
    pub fn apply_mine(&mut self, damage: u16) {
        for gauge in &mut self.gauges {
            gauge.apply_mine(damage);
        }
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
    }
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
    ]
}

const NORMAL_GUTS: &[(f32, f32)] = &[(30.0, 0.5), (50.0, 0.7)];
const HARD_GUTS: &[(f32, f32)] = &[(30.0, 0.6), (50.0, 0.8)];

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
    }
}
