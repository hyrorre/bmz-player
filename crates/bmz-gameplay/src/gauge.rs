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
        let gauges = gauge_definitions_for_rule_mode(property, rule_mode)
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
        let start = match mode {
            GaugeAutoShiftMode::BestClear => GaugeType::Hazard,
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove
            | GaugeAutoShiftMode::SelectToUnder => selected,
        };
        let mut state =
            Self::new_with_property_and_rule_mode(start, total, total_notes, property, rule_mode);
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
                self.selected = self.best_current_auto_shift_gauge();
            }
        }
    }

    fn best_current_auto_shift_gauge(&self) -> GaugeType {
        let top_rank = match self.auto_shift_mode {
            GaugeAutoShiftMode::BestClear => auto_shift_result_rank(GaugeType::Hazard),
            GaugeAutoShiftMode::SelectToUnder => auto_shift_result_rank(self.original),
            GaugeAutoShiftMode::Off
            | GaugeAutoShiftMode::Continue
            | GaugeAutoShiftMode::HardToGroove => auto_shift_result_rank(self.selected),
        };
        let current_rank = auto_shift_result_rank(self.selected);
        let bottom_rank = auto_shift_result_rank(self.bottom_shiftable_gauge);
        let start_rank = current_rank.min(bottom_rank);

        AUTO_SHIFT_RESULT_ORDER
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
            .unwrap_or_else(|| auto_shift_gauge_for_rank(start_rank))
    }

    fn gauge(&self, gauge_type: GaugeType) -> Option<&SingleGaugeState> {
        self.gauges.iter().find(|gauge| gauge.definition.gauge_type == gauge_type)
    }
}

const AUTO_SHIFT_RESULT_ORDER: &[GaugeType] = &[
    GaugeType::Hazard,
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

fn normalize_bottom_shiftable_gauge(gauge_type: GaugeType) -> GaugeType {
    match gauge_type {
        GaugeType::AssistEasy | GaugeType::Easy | GaugeType::Normal => gauge_type,
        _ => GaugeType::AssistEasy,
    }
}

fn auto_shift_gauge_for_rank(rank: u8) -> GaugeType {
    match rank {
        0 => GaugeType::AssistEasy,
        1 => GaugeType::Easy,
        _ => GaugeType::Normal,
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
            self.apply_death_threshold();
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
        self.apply_death_threshold();
    }

    fn apply_death_threshold(&mut self) {
        if self.value > self.definition.min && self.value < self.definition.death {
            self.value = self.definition.min;
        }
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
        death: base.death,
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
        GaugeModifier::ModifyDamage => {
            if value < 0.0 {
                value * modify_damage_scale(total, total_notes)
            } else {
                value
            }
        }
        GaugeModifier::Iidx => {
            if value > 0.0 && total_notes > 0 {
                value * iidx_total_value(total_notes) / total_notes as f32
            } else {
                value
            }
        }
    }
}

fn modify_damage_scale(total: f64, total_notes: u32) -> f32 {
    const FIX1_TOTAL: [f64; 10] =
        [240.0, 230.0, 210.0, 200.0, 180.0, 160.0, 150.0, 130.0, 120.0, 0.0];
    const FIX1_TABLE: [f32; 10] = [1.0, 1.11, 1.25, 1.5, 1.666, 2.0, 2.5, 3.333, 5.0, 10.0];

    let mut i = 0;
    while i < FIX1_TOTAL.len() - 1 && total < FIX1_TOTAL[i] {
        i += 1;
    }

    let mut fix2 = 1.0;
    let mut note = 1000_u32;
    let mut modifier = 0.002;
    while note > total_notes || note > 1 {
        fix2 += modifier * note.saturating_sub(total_notes.max(note / 2)) as f32;
        note /= 2;
        modifier *= 2.0;
    }

    FIX1_TABLE[i].max(fix2)
}

fn iidx_total_value(total_notes: u32) -> f32 {
    let notes = total_notes as f32;
    if notes <= 0.0 {
        return 0.0;
    }
    260.0_f32.max(7.605 * notes / (0.01 * notes + 6.5))
}

/// 既定 `SevenKeys` プロパティのゲージ定義（後方互換）。
pub fn default_gauge_definitions() -> Vec<GaugeDefinition> {
    gauge_definitions_for(GaugeProperty::default())
}

/// 指定 `GaugeProperty` に対応するゲージ定義一式を返す。
/// beatoraja `GaugeProperty.java` の全ゲージ値をプロパティ別に引く。
pub fn gauge_definitions_for(property: GaugeProperty) -> Vec<GaugeDefinition> {
    gauge_definition_table(property).to_vec()
}

pub fn gauge_definitions_for_rule_mode(
    property: GaugeProperty,
    rule_mode: RuleMode,
) -> Vec<GaugeDefinition> {
    match rule_mode {
        RuleMode::Beatoraja => gauge_definitions_for(property),
        RuleMode::Lr2Oraja => lr2oraja_gauge_definitions(),
        RuleMode::Dx => dx_gauge_definition_table().to_vec(),
    }
}

fn lr2oraja_gauge_definitions() -> Vec<GaugeDefinition> {
    let mut definitions = gauge_definition_table(GaugeProperty::Lr2).to_vec();
    for definition in &mut definitions {
        match definition.gauge_type {
            GaugeType::Hard => {
                definition.guts = LR2_HARD_GUTS;
                definition.death = 2.0;
            }
            GaugeType::ExHard | GaugeType::Hazard => {
                definition.death = 2.0;
            }
            GaugeType::Class | GaugeType::ExClass => {
                definition.guts = LR2_HARD_GUTS;
                definition.death = 2.0;
            }
            _ => {}
        }
    }
    definitions
}

/// beatoraja `GaugeProperty.java` の `GaugeElementProperty` 表。
fn gauge_definition_table(property: GaugeProperty) -> [GaugeDefinition; 9] {
    match property {
        GaugeProperty::FiveKeys => [
            def(
                GaugeType::AssistEasy,
                Some(ClearType::AssistEasy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                50.0,
                [1.0, 1.0, 0.5, -1.5, -3.0, -0.5],
                &[],
            ),
            def(
                GaugeType::Easy,
                Some(ClearType::Easy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                75.0,
                [1.0, 1.0, 0.5, -1.5, -4.5, -1.0],
                &[],
            ),
            def(
                GaugeType::Normal,
                Some(ClearType::Normal),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                75.0,
                [1.0, 1.0, 0.5, -3.0, -6.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Hard,
                Some(ClearType::Hard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.0, 0.0, 0.0, -5.0, -10.0, -5.0],
                &[],
            ),
            def(
                GaugeType::ExHard,
                Some(ClearType::ExHard),
                GaugeModifier::ModifyDamage,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.0, 0.0, 0.0, -10.0, -20.0, -10.0],
                &[],
            ),
            def(
                GaugeType::Hazard,
                None,
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.0, 0.0, 0.0, -100.0, -100.0, -100.0],
                &[],
            ),
            def(
                GaugeType::Class,
                Some(ClearType::Normal),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.01, 0.01, 0.0, -0.5, -1.0, -0.5],
                &[],
            ),
            def(
                GaugeType::ExClass,
                Some(ClearType::Hard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.01, 0.01, 0.0, -1.0, -2.0, -1.0],
                &[],
            ),
            def(
                GaugeType::ExHardClass,
                Some(ClearType::ExHard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.01, 0.01, 0.0, -2.5, -5.0, -2.5],
                &[],
            ),
        ],
        GaugeProperty::SevenKeys => [
            def(
                GaugeType::AssistEasy,
                Some(ClearType::AssistEasy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                60.0,
                [1.0, 1.0, 0.5, -1.5, -3.0, -0.5],
                &[],
            ),
            def(
                GaugeType::Easy,
                Some(ClearType::Easy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                80.0,
                [1.0, 1.0, 0.5, -1.5, -4.5, -1.0],
                &[],
            ),
            def(
                GaugeType::Normal,
                Some(ClearType::Normal),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                80.0,
                [1.0, 1.0, 0.5, -3.0, -6.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Hard,
                Some(ClearType::Hard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.03, -5.0, -10.0, -5.0],
                HARD_GUTS,
            ),
            def(
                GaugeType::ExHard,
                Some(ClearType::ExHard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -8.0, -16.0, -8.0],
                &[],
            ),
            def(
                GaugeType::Hazard,
                None,
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -100.0, -100.0, -10.0],
                &[],
            ),
            def(
                GaugeType::Class,
                Some(ClearType::Normal),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.06, -1.5, -3.0, -1.5],
                CLASS_GUTS,
            ),
            def(
                GaugeType::ExClass,
                Some(ClearType::Hard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.03, -3.0, -6.0, -3.0],
                &[],
            ),
            def(
                GaugeType::ExHardClass,
                Some(ClearType::ExHard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -5.0, -10.0, -5.0],
                &[],
            ),
        ],
        GaugeProperty::Pms => [
            def(
                GaugeType::AssistEasy,
                Some(ClearType::AssistEasy),
                GaugeModifier::Total,
                2.0,
                120.0,
                30.0,
                65.0,
                [1.0, 1.0, 0.5, -1.0, -2.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Easy,
                Some(ClearType::Easy),
                GaugeModifier::Total,
                2.0,
                120.0,
                30.0,
                85.0,
                [1.0, 1.0, 0.5, -1.0, -3.0, -3.0],
                &[],
            ),
            def(
                GaugeType::Normal,
                Some(ClearType::Normal),
                GaugeModifier::Total,
                2.0,
                120.0,
                30.0,
                85.0,
                [1.0, 1.0, 0.5, -2.0, -6.0, -6.0],
                &[],
            ),
            def(
                GaugeType::Hard,
                Some(ClearType::Hard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.03, -5.0, -10.0, -10.0],
                HARD_GUTS,
            ),
            def(
                GaugeType::ExHard,
                Some(ClearType::ExHard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -10.0, -15.0, -15.0],
                &[],
            ),
            def(
                GaugeType::Hazard,
                None,
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -100.0, -100.0, -100.0],
                &[],
            ),
            def(
                GaugeType::Class,
                Some(ClearType::Normal),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.06, -1.5, -3.0, -3.0],
                CLASS_GUTS,
            ),
            def(
                GaugeType::ExClass,
                Some(ClearType::Hard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.12, 0.03, -3.0, -6.0, -6.0],
                &[],
            ),
            def(
                GaugeType::ExHardClass,
                Some(ClearType::ExHard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -5.0, -10.0, -10.0],
                &[],
            ),
        ],
        GaugeProperty::Keyboard => [
            def(
                GaugeType::AssistEasy,
                Some(ClearType::AssistEasy),
                GaugeModifier::Total,
                2.0,
                100.0,
                30.0,
                50.0,
                [1.0, 1.0, 0.5, -1.0, -2.0, -1.0],
                &[],
            ),
            def(
                GaugeType::Easy,
                Some(ClearType::Easy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                70.0,
                [1.0, 1.0, 0.5, -1.0, -3.0, -1.0],
                &[],
            ),
            def(
                GaugeType::Normal,
                Some(ClearType::Normal),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                70.0,
                [1.0, 1.0, 0.5, -2.0, -4.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Hard,
                Some(ClearType::Hard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.2, 0.1, -4.0, -8.0, -4.0],
                HARD_GUTS,
            ),
            def(
                GaugeType::ExHard,
                Some(ClearType::ExHard),
                GaugeModifier::LimitIncrement,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.1, 0.0, -6.0, -12.0, -6.0],
                &[],
            ),
            def(
                GaugeType::Hazard,
                None,
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.1, 0.0, -100.0, -100.0, -100.0],
                &[],
            ),
            def(
                GaugeType::Class,
                Some(ClearType::Normal),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.2, 0.1, -1.5, -3.0, -1.5],
                CLASS_GUTS,
            ),
            def(
                GaugeType::ExClass,
                Some(ClearType::Hard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.2, 0.1, -3.0, -6.0, -3.0],
                &[],
            ),
            def(
                GaugeType::ExHardClass,
                Some(ClearType::ExHard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.2, 0.1, 0.0, -5.0, -10.0, -5.0],
                &[],
            ),
        ],
        GaugeProperty::Lr2 => [
            def(
                GaugeType::AssistEasy,
                Some(ClearType::AssistEasy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                60.0,
                [1.2, 1.2, 0.6, -3.2, -4.8, -1.6],
                &[],
            ),
            def(
                GaugeType::Easy,
                Some(ClearType::Easy),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                80.0,
                [1.2, 1.2, 0.6, -3.2, -4.8, -1.6],
                &[],
            ),
            def(
                GaugeType::Normal,
                Some(ClearType::Normal),
                GaugeModifier::Total,
                2.0,
                100.0,
                20.0,
                80.0,
                [1.0, 1.0, 0.5, -4.0, -6.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Hard,
                Some(ClearType::Hard),
                GaugeModifier::ModifyDamage,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.1, 0.1, 0.05, -6.0, -10.0, -2.0],
                LR2_CLASS_GUTS,
            ),
            def(
                GaugeType::ExHard,
                Some(ClearType::ExHard),
                GaugeModifier::ModifyDamage,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.1, 0.1, 0.05, -12.0, -20.0, -2.0],
                &[],
            ),
            def(
                GaugeType::Hazard,
                None,
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.15, 0.06, 0.0, -100.0, -100.0, -10.0],
                &[],
            ),
            def(
                GaugeType::Class,
                Some(ClearType::Normal),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.1, 0.1, 0.05, -2.0, -3.0, -2.0],
                LR2_CLASS_GUTS,
            ),
            def(
                GaugeType::ExClass,
                Some(ClearType::Hard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.1, 0.1, 0.05, -6.0, -10.0, -2.0],
                LR2_CLASS_GUTS,
            ),
            def(
                GaugeType::ExHardClass,
                Some(ClearType::ExHard),
                GaugeModifier::None,
                0.0,
                100.0,
                100.0,
                0.0,
                [0.1, 0.1, 0.05, -12.0, -20.0, -2.0],
                &[],
            ),
        ],
    }
}

fn dx_gauge_definition_table() -> [GaugeDefinition; 9] {
    [
        def(
            GaugeType::AssistEasy,
            Some(ClearType::AssistEasy),
            GaugeModifier::Iidx,
            2.0,
            100.0,
            20.0,
            60.0,
            [1.0, 1.0, 0.5, -1.6, -4.8, -1.6],
            &[],
        ),
        def(
            GaugeType::Easy,
            Some(ClearType::Easy),
            GaugeModifier::Iidx,
            2.0,
            100.0,
            20.0,
            80.0,
            [1.0, 1.0, 0.5, -1.6, -4.8, -1.6],
            &[],
        ),
        def(
            GaugeType::Normal,
            Some(ClearType::Normal),
            GaugeModifier::Iidx,
            2.0,
            100.0,
            20.0,
            80.0,
            [1.0, 1.0, 0.5, -2.0, -6.0, -2.0],
            &[],
        ),
        def(
            GaugeType::Hard,
            Some(ClearType::Hard),
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.0, -4.5, -9.0, -4.5],
            DX_HARD_GUTS,
        ),
        def(
            GaugeType::ExHard,
            Some(ClearType::ExHard),
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.0, -9.0, -18.0, -9.0],
            &[],
        ),
        def(
            GaugeType::Hazard,
            None,
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.0, -100.0, -100.0, -9.0],
            &[],
        ),
        def(
            GaugeType::Class,
            Some(ClearType::Normal),
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.04, -1.5, -2.5, -1.5],
            DX_HARD_GUTS,
        ),
        def(
            GaugeType::ExClass,
            Some(ClearType::Hard),
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.04, -3.0, -5.0, -3.0],
            &[],
        ),
        def(
            GaugeType::ExHardClass,
            Some(ClearType::ExHard),
            GaugeModifier::None,
            0.0,
            100.0,
            100.0,
            0.0,
            [0.16, 0.16, 0.04, -6.0, -10.0, -6.0],
            &[],
        ),
    ]
}

fn def(
    gauge_type: GaugeType,
    clear_type: Option<ClearType>,
    modifier: GaugeModifier,
    min: f32,
    max: f32,
    init: f32,
    border: f32,
    values: [f32; 6],
    guts: &'static [(f32, f32)],
) -> GaugeDefinition {
    GaugeDefinition {
        gauge_type,
        clear_type,
        modifier,
        min,
        max,
        init,
        border,
        death: 0.0,
        values,
        guts,
    }
}

// beatoraja HARD guts テーブル（7keys / PMS / KB 共通）。
const HARD_GUTS: &[(f32, f32)] = &[(10.0, 0.4), (20.0, 0.5), (30.0, 0.6), (40.0, 0.7), (50.0, 0.8)];
// beatoraja CLASS guts テーブル（7keys / PMS / KB 共通）。
const CLASS_GUTS: &[(f32, f32)] = &[(5.0, 0.4), (10.0, 0.5), (15.0, 0.6), (20.0, 0.7), (25.0, 0.8)];
// beatoraja LR2 CLASS / EXCLASS の guts。30 以下で減衰量を 60% に弱める。
const LR2_CLASS_GUTS: &[(f32, f32)] = &[(30.0, 0.6)];
// LR2oraja 0.8.3+ の LR2 HARD 系 guts。32% 未満で減衰量を 60% に弱める。
const LR2_HARD_GUTS: &[(f32, f32)] = &[(32.0, 0.6)];
const DX_HARD_GUTS: &[(f32, f32)] = &[(30.0, 0.5)];

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
    fn auto_shift_starts_from_hazard_and_falls_back_to_exhard() {
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

        // Poor at rate 1.0 drains Hazard (value -100) in one hit, then
        // ExHard only loses -16 and stays alive.
        gauge.apply_judge(Judge::Poor, 1.0);

        assert!(gauge.auto_shift);
        assert_eq!(gauge.original, GaugeType::Hazard);
        assert_eq!(gauge.selected, GaugeType::ExHard);
    }

    #[test]
    fn auto_shift_result_uses_highest_qualified_gauge() {
        let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hazard)
            .unwrap()
            .value = 0.0;
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

    fn definition_for_rule_mode(gauge_type: GaugeType, rule_mode: RuleMode) -> GaugeDefinition {
        gauge_definitions_for_rule_mode(GaugeProperty::SevenKeys, rule_mode)
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
            assert_eq!(def.border, 0.0, "{ty:?} border");
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
    fn groove_gauge_values_match_beatoraja_gauge_property() {
        let normal_7 = definition_for_property(GaugeType::Normal, GaugeProperty::SevenKeys);
        assert_eq!(normal_7.min, 2.0);
        assert_eq!(normal_7.max, 100.0);
        assert_eq!(normal_7.init, 20.0);
        assert_eq!(normal_7.border, 80.0);
        assert_eq!(normal_7.values, [1.0, 1.0, 0.5, -3.0, -6.0, -2.0]);

        let hard_7 = definition_for_property(GaugeType::Hard, GaugeProperty::SevenKeys);
        assert_eq!(hard_7.values, [0.15, 0.12, 0.03, -5.0, -10.0, -5.0]);
        assert_eq!(hard_7.guts, HARD_GUTS);

        let exhard_7 = definition_for_property(GaugeType::ExHard, GaugeProperty::SevenKeys);
        assert_eq!(exhard_7.values, [0.15, 0.06, 0.0, -8.0, -16.0, -8.0]);

        let normal_pms = definition_for_property(GaugeType::Normal, GaugeProperty::Pms);
        assert_eq!(normal_pms.max, 120.0);
        assert_eq!(normal_pms.init, 30.0);
        assert_eq!(normal_pms.border, 85.0);
        assert_eq!(normal_pms.values, [1.0, 1.0, 0.5, -2.0, -6.0, -6.0]);

        let normal_kb = definition_for_property(GaugeType::Normal, GaugeProperty::Keyboard);
        assert_eq!(normal_kb.border, 70.0);
        assert_eq!(normal_kb.values, [1.0, 1.0, 0.5, -2.0, -4.0, -2.0]);

        let lr2_hard = definition_for_property(GaugeType::Hard, GaugeProperty::Lr2);
        assert_eq!(lr2_hard.modifier, GaugeModifier::ModifyDamage);
        assert_eq!(lr2_hard.values, [0.1, 0.1, 0.05, -6.0, -10.0, -2.0]);
    }

    #[test]
    fn lr2oraja_hard_gauge_uses_32_percent_guts_and_2_percent_death() {
        let hard = definition_for_rule_mode(GaugeType::Hard, RuleMode::Lr2Oraja);
        assert_eq!(hard.guts, LR2_HARD_GUTS);
        assert_eq!(hard.death, 2.0);

        let mut at_threshold = GaugeState::new_with_property_and_rule_mode(
            GaugeType::Hard,
            160.0,
            1000,
            GaugeProperty::SevenKeys,
            RuleMode::Lr2Oraja,
        );
        let hard_at_threshold = at_threshold
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
            .unwrap();
        hard_at_threshold.value = 32.0;
        let damage_without_guts = hard_at_threshold.definition.values[GaugeJudgeIndex::Pr as usize];
        hard_at_threshold.apply(GaugeJudgeIndex::Pr, 1.0);
        assert_eq!(hard_at_threshold.value, 32.0 + damage_without_guts);

        let mut below_threshold = GaugeState::new_with_property_and_rule_mode(
            GaugeType::Hard,
            160.0,
            1000,
            GaugeProperty::SevenKeys,
            RuleMode::Lr2Oraja,
        );
        let hard_below_threshold = below_threshold
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
            .unwrap();
        hard_below_threshold.value = 31.9;
        hard_below_threshold.apply(GaugeJudgeIndex::Pr, 1.0);
        assert!((hard_below_threshold.value - (31.9 + damage_without_guts * 0.6)).abs() < 0.000_1);

        hard_below_threshold.value = 2.1;
        hard_below_threshold.apply(GaugeJudgeIndex::Epr, 0.1);
        assert_eq!(hard_below_threshold.value, 0.0);
    }

    #[test]
    fn dx_gauge_definitions_match_lr2oraja_iidx_mode() {
        let normal = definition_for_rule_mode(GaugeType::Normal, RuleMode::Dx);
        assert_eq!(normal.modifier, GaugeModifier::Iidx);
        assert_eq!(normal.values, [1.0, 1.0, 0.5, -2.0, -6.0, -2.0]);

        let hard = definition_for_rule_mode(GaugeType::Hard, RuleMode::Dx);
        assert_eq!(hard.modifier, GaugeModifier::None);
        assert_eq!(hard.values, [0.16, 0.16, 0.0, -4.5, -9.0, -4.5]);
        assert_eq!(hard.guts, DX_HARD_GUTS);

        let mut gauge = GaugeState::new_with_property_and_rule_mode(
            GaugeType::Normal,
            999.0,
            1000,
            GaugeProperty::SevenKeys,
            RuleMode::Dx,
        );
        let start = gauge.current().value;
        gauge.apply_judge(Judge::PGreat, 1.0);
        assert!(
            (gauge.current().value - (start + iidx_total_value(1000) / 1000.0)).abs() < 0.000_1
        );
    }

    #[test]
    fn hcn_gauge_updates_use_beatoraja_great_and_bad_half_rate() {
        let mut gauge = GaugeState::new(GaugeType::Normal, 160.0, 1000);
        let start = gauge.current().value;

        // 1 tick = GREAT × 0.5 / BAD × 0.5 (beatoraja gauge.update(1|3, 0.5f))
        gauge.apply_hcn_hold();
        assert!((gauge.current().value - (start + 0.08)).abs() < f32::EPSILON);

        gauge.apply_hcn_drain();
        assert!((gauge.current().value - (start - 1.42)).abs() < 0.000_1);
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

    #[test]
    fn auto_shift_respects_bottom_shiftable_gauge() {
        let mut gauge = GaugeState::new_with_auto_shift(
            GaugeType::ExHard,
            GaugeAutoShiftMode::BestClear,
            160.0,
            1000,
        );
        gauge.set_bottom_shiftable_gauge(GaugeType::Normal);
        for gauge in &mut gauge.gauges {
            gauge.value = 0.0;
        }
        gauge
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == GaugeType::Easy)
            .unwrap()
            .value = 100.0;

        gauge.apply_judge(Judge::Poor, 1.0);

        assert_eq!(gauge.selected, GaugeType::Normal);
    }
}
