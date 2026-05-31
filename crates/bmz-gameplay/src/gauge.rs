use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::judge::Judge;
use bmz_core::lane::KeyMode;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaugeProperty {
    FiveKeys,
    #[default]
    SevenKeys,
    /// pop'n music 系（9K）。bmz-player のキーモードでは現状未対応だが、
    /// `CourseGaugeConstraint::Keys9` から指定された場合の段位ゲージ値として保持する。
    Pms,
    /// keyboard mania 系（24K）。同上で、コース定義側から指定された場合のみ使う。
    Keyboard,
    /// LR2 互換。コース側で `gauge_lr2` 指定時に明示的に使う。
    Lr2,
}

impl GaugeProperty {
    /// チャートの `KeyMode` から beatoraja 既定の `GaugeProperty` を決める。
    /// `BMSPlayerRule.Beatoraja_5/7` と同等：5K/10K→FiveKeys、7K/14K→SevenKeys。
    /// PMS / KEYBOARD はチャート由来では選ばれず、コース定義側からのみ来る。
    pub fn from_keymode(key_mode: KeyMode) -> Self {
        match key_mode {
            KeyMode::K5 | KeyMode::K10 => Self::FiveKeys,
            KeyMode::K7 | KeyMode::K14 => Self::SevenKeys,
        }
    }
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
        let gauges = gauge_definitions_for(property)
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
        let start = match mode {
            GaugeAutoShiftMode::BestClear => GaugeType::ExHard,
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove
            | GaugeAutoShiftMode::SelectToUnder => selected,
        };
        let mut state = Self::new_with_property(start, total, total_notes, property);
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

/// 既定 `SevenKeys` プロパティのゲージ定義（後方互換）。
pub fn default_gauge_definitions() -> Vec<GaugeDefinition> {
    gauge_definitions_for(GaugeProperty::default())
}

/// 指定 `GaugeProperty` に対応するゲージ定義一式を返す。
/// グルーヴ系ゲージは全プロパティ共通（既存値を維持）、段位ゲージのみ
/// beatoraja `GaugeProperty.java` のキーモード別値を引く。
pub fn gauge_definitions_for(property: GaugeProperty) -> Vec<GaugeDefinition> {
    let mut defs = Vec::with_capacity(9);
    defs.extend_from_slice(&GROOVE_GAUGE_DEFINITIONS);
    let class = class_gauge_set(property);
    defs.push(GaugeDefinition {
        gauge_type: GaugeType::Class,
        clear_type: Some(ClearType::Normal),
        modifier: GaugeModifier::None,
        min: 0.0,
        max: 100.0,
        init: 100.0,
        border: 1.0,
        values: class.class,
        guts: class.class_guts,
    });
    defs.push(GaugeDefinition {
        gauge_type: GaugeType::ExClass,
        clear_type: Some(ClearType::Hard),
        modifier: GaugeModifier::None,
        min: 0.0,
        max: 100.0,
        init: 100.0,
        border: 1.0,
        values: class.exclass,
        guts: class.exclass_guts,
    });
    defs.push(GaugeDefinition {
        gauge_type: GaugeType::ExHardClass,
        clear_type: Some(ClearType::ExHard),
        modifier: GaugeModifier::None,
        min: 0.0,
        max: 100.0,
        init: 100.0,
        border: 1.0,
        values: class.exhardclass,
        guts: class.exhardclass_guts,
    });
    defs
}

/// グルーヴ系ゲージ (AssistEasy..Hazard) の定義。本実装ではキーモードによらず
/// 共通の値を使う（既存挙動の維持を優先し、beatoraja のキーモード別 groove 値
/// 移植は将来対応）。
const GROOVE_GAUGE_DEFINITIONS: [GaugeDefinition; 6] = [
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
];

/// 段位ゲージ 3 種の値と guts テーブルをまとめた組。
struct ClassGaugeSet {
    class: [f32; 6],
    class_guts: &'static [(f32, f32)],
    exclass: [f32; 6],
    exclass_guts: &'static [(f32, f32)],
    exhardclass: [f32; 6],
    exhardclass_guts: &'static [(f32, f32)],
}

/// beatoraja `GaugeProperty.java` から CLASS_X / EXCLASS_X / EXHARDCLASS_X を引く。
fn class_gauge_set(property: GaugeProperty) -> ClassGaugeSet {
    match property {
        GaugeProperty::SevenKeys => ClassGaugeSet {
            class: [0.15, 0.12, 0.06, -1.5, -3.0, -1.5],
            class_guts: CLASS_7K_GUTS,
            exclass: [0.15, 0.12, 0.03, -3.0, -6.0, -3.0],
            exclass_guts: &[],
            exhardclass: [0.15, 0.06, 0.0, -5.0, -10.0, -5.0],
            exhardclass_guts: &[],
        },
        GaugeProperty::FiveKeys => ClassGaugeSet {
            class: [0.01, 0.01, 0.0, -0.5, -1.0, -0.5],
            class_guts: &[],
            exclass: [0.01, 0.01, 0.0, -1.0, -2.0, -1.0],
            exclass_guts: &[],
            exhardclass: [0.01, 0.01, 0.0, -2.5, -5.0, -2.5],
            exhardclass_guts: &[],
        },
        GaugeProperty::Pms => ClassGaugeSet {
            class: [0.15, 0.12, 0.06, -1.5, -3.0, -3.0],
            class_guts: CLASS_7K_GUTS,
            exclass: [0.15, 0.12, 0.03, -3.0, -6.0, -6.0],
            exclass_guts: &[],
            exhardclass: [0.15, 0.06, 0.0, -5.0, -10.0, -10.0],
            exhardclass_guts: &[],
        },
        GaugeProperty::Keyboard => ClassGaugeSet {
            class: [0.20, 0.20, 0.10, -1.5, -3.0, -1.5],
            class_guts: CLASS_7K_GUTS,
            exclass: [0.20, 0.20, 0.10, -3.0, -6.0, -3.0],
            exclass_guts: &[],
            exhardclass: [0.20, 0.10, 0.0, -5.0, -10.0, -5.0],
            exhardclass_guts: &[],
        },
        GaugeProperty::Lr2 => ClassGaugeSet {
            class: [0.10, 0.10, 0.05, -2.0, -3.0, -2.0],
            class_guts: LR2_CLASS_GUTS,
            exclass: [0.10, 0.10, 0.05, -6.0, -10.0, -2.0],
            exclass_guts: LR2_CLASS_GUTS,
            exhardclass: [0.10, 0.10, 0.05, -12.0, -20.0, -2.0],
            exhardclass_guts: &[],
        },
    }
}

const NORMAL_GUTS: &[(f32, f32)] = &[(30.0, 0.5), (50.0, 0.7)];
const HARD_GUTS: &[(f32, f32)] = &[(30.0, 0.6), (50.0, 0.8)];
// beatoraja CLASS の guts テーブル（7keys / PMS / KB 共通）。下限近くで減衰量が弱まる救済補正。
const CLASS_7K_GUTS: &[(f32, f32)] =
    &[(5.0, 0.4), (10.0, 0.5), (15.0, 0.6), (20.0, 0.7), (25.0, 0.8)];
// beatoraja LR2 CLASS / EXCLASS の guts。30 以下で減衰量を 60% に弱める。
const LR2_CLASS_GUTS: &[(f32, f32)] = &[(30.0, 0.6)];

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

    fn definition_for(gauge_type: GaugeType) -> GaugeDefinition {
        default_gauge_definitions()
            .into_iter()
            .find(|def| def.gauge_type == gauge_type)
            .expect("definition exists")
    }

    fn definition_for_property(gauge_type: GaugeType, property: GaugeProperty) -> GaugeDefinition {
        gauge_definitions_for(property)
            .into_iter()
            .find(|def| def.gauge_type == gauge_type)
            .expect("definition exists")
    }

    #[test]
    fn class_gauges_start_full_and_clear_above_zero() {
        for &(ty, expected_clear) in &[
            (GaugeType::Class, Some(ClearType::Normal)),
            (GaugeType::ExClass, Some(ClearType::Hard)),
            (GaugeType::ExHardClass, Some(ClearType::ExHard)),
        ] {
            let def = definition_for(ty);
            assert_eq!(def.init, 100.0, "{ty:?} init");
            assert_eq!(def.max, 100.0, "{ty:?} max");
            assert_eq!(def.min, 0.0, "{ty:?} min");
            assert_eq!(def.border, 1.0, "{ty:?} border");
            assert_eq!(def.clear_type, expected_clear, "{ty:?} clear_type");
        }
    }

    #[test]
    fn class_gauge_fails_at_zero_like_survival_gauges() {
        for ty in [GaugeType::Class, GaugeType::ExClass, GaugeType::ExHardClass] {
            let mut gauge = GaugeState::new(ty, 160.0, 1000);
            gauge.apply_judge(Judge::Poor, 200.0);
            assert_eq!(gauge.current().value, 0.0, "{ty:?} should drain to zero");
            assert!(!gauge.current().is_qualified(), "{ty:?} not qualified at zero");
            assert!(gauge.current_closes_play_on_zero(), "{ty:?} closes play on zero");
        }
    }

    #[test]
    fn class_gauges_drain_strictly_more_than_normal() {
        let class = definition_for(GaugeType::Class);
        let exclass = definition_for(GaugeType::ExClass);
        let exhardclass = definition_for(GaugeType::ExHardClass);
        // Bad index = 3. Each tier should drain at least as hard as Class.
        assert!(class.values[3] >= exclass.values[3]);
        assert!(exclass.values[3] >= exhardclass.values[3]);
    }

    #[test]
    fn gauge_property_from_keymode_matches_beatoraja_player_rule() {
        assert_eq!(GaugeProperty::from_keymode(KeyMode::K5), GaugeProperty::FiveKeys);
        assert_eq!(GaugeProperty::from_keymode(KeyMode::K10), GaugeProperty::FiveKeys);
        assert_eq!(GaugeProperty::from_keymode(KeyMode::K7), GaugeProperty::SevenKeys);
        assert_eq!(GaugeProperty::from_keymode(KeyMode::K14), GaugeProperty::SevenKeys);
    }

    #[test]
    fn class_gauge_values_differ_per_property() {
        // beatoraja FIVEKEYS の CLASS は SEVENKEYS よりはるかにマイルド。
        let class_5 = definition_for_property(GaugeType::Class, GaugeProperty::FiveKeys);
        let class_7 = definition_for_property(GaugeType::Class, GaugeProperty::SevenKeys);
        assert_eq!(class_5.values, [0.01, 0.01, 0.0, -0.5, -1.0, -0.5]);
        assert_eq!(class_7.values, [0.15, 0.12, 0.06, -1.5, -3.0, -1.5]);

        // PMS の CLASS は SEVENKEYS と回復は同じだが EmptyPoor (idx 5) が厳しい (-3 vs -1.5)。
        let class_pms = definition_for_property(GaugeType::Class, GaugeProperty::Pms);
        assert_eq!(class_pms.values[5], -3.0);

        // LR2 EXHARDCLASS は突き抜けて重い (-12 BAD)。
        let exhardclass_lr2 = definition_for_property(GaugeType::ExHardClass, GaugeProperty::Lr2);
        assert_eq!(exhardclass_lr2.values[3], -12.0);

        // KEYBOARD CLASS は PG/GR 回復が 0.20 と高い。
        let class_kb = definition_for_property(GaugeType::Class, GaugeProperty::Keyboard);
        assert_eq!(class_kb.values[0], 0.20);
    }

    #[test]
    fn groove_gauges_stay_constant_across_properties() {
        // グルーヴゲージは本実装ではキーモード非依存。プロパティを変えても同じ定義。
        for property in [
            GaugeProperty::FiveKeys,
            GaugeProperty::SevenKeys,
            GaugeProperty::Pms,
            GaugeProperty::Keyboard,
            GaugeProperty::Lr2,
        ] {
            let normal = definition_for_property(GaugeType::Normal, property);
            assert_eq!(normal.values, [0.16, 0.16, 0.0, -4.0, -6.0, -2.0], "{property:?}");
        }
    }

    #[test]
    fn set_initial_value_carries_over_class_gauge() {
        let mut gauge = GaugeState::new(GaugeType::Class, 160.0, 1000);
        gauge.set_initial_value(45.0);
        assert_eq!(gauge.current().value, 45.0);
        assert!(gauge.current().is_qualified());
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
