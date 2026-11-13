use super::*;

pub(super) const AUTO_SHIFT_RESULT_ORDER: &[GaugeType] = &[
    GaugeType::Hazard,
    GaugeType::ExHard,
    GaugeType::Hard,
    GaugeType::Normal,
    GaugeType::Easy,
    GaugeType::AssistEasy,
];

pub(super) const COURSE_AUTO_SHIFT_RESULT_ORDER: &[GaugeType] =
    &[GaugeType::ExHardClass, GaugeType::ExClass, GaugeType::Class];

pub(super) fn auto_shift_result_order(original: GaugeType) -> &'static [GaugeType] {
    if is_course_gauge(original) { COURSE_AUTO_SHIFT_RESULT_ORDER } else { AUTO_SHIFT_RESULT_ORDER }
}

pub(super) fn auto_shift_result_rank(gauge_type: GaugeType) -> u8 {
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

pub(super) fn is_course_gauge(gauge_type: GaugeType) -> bool {
    matches!(gauge_type, GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass)
}

pub(super) fn normalize_bottom_shiftable_gauge(gauge_type: GaugeType) -> GaugeType {
    match gauge_type {
        GaugeType::AssistEasy | GaugeType::Easy | GaugeType::Normal => gauge_type,
        _ => GaugeType::AssistEasy,
    }
}

pub(super) fn auto_shift_gauge_for_rank(rank: u8) -> GaugeType {
    match rank {
        0 => GaugeType::AssistEasy,
        1 => GaugeType::Easy,
        2 => GaugeType::Normal,
        3 => GaugeType::Hard,
        4 => GaugeType::ExHard,
        5 => GaugeType::Hazard,
        6 => GaugeType::Class,
        7 => GaugeType::ExClass,
        _ => GaugeType::ExHardClass,
    }
}

pub(super) fn lowest_auto_shift_gauge(
    order: &[GaugeType],
    start_rank: u8,
    top_rank: u8,
) -> GaugeType {
    order
        .iter()
        .copied()
        .rfind(|gauge_type| {
            let rank = auto_shift_result_rank(*gauge_type);
            rank >= start_rank && rank <= top_rank
        })
        .unwrap_or_else(|| auto_shift_gauge_for_rank(start_rank))
}

pub(super) fn gauge_closes_play_on_zero(gauge_type: GaugeType) -> bool {
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
    pub fn apply_mine(&mut self, damage: f64) {
        if self.value <= 0.0 {
            return;
        }
        self.value = (self.value - damage as f32).clamp(self.definition.min, self.definition.max);
        self.apply_death_threshold();
    }

    pub fn apply_potion(&mut self, recovery: f64) {
        if self.value > 0.0 && recovery.is_finite() && recovery > 0.0 {
            self.value =
                (self.value + recovery as f32).clamp(self.definition.min, self.definition.max);
        }
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
