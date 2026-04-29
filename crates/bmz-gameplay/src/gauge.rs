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
