use super::*;

pub(super) fn def(
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
pub(super) const HARD_GUTS: &[(f32, f32)] =
    &[(10.0, 0.4), (20.0, 0.5), (30.0, 0.6), (40.0, 0.7), (50.0, 0.8)];
// beatoraja CLASS guts テーブル（7keys / PMS / KB 共通）。
pub(super) const CLASS_GUTS: &[(f32, f32)] =
    &[(5.0, 0.4), (10.0, 0.5), (15.0, 0.6), (20.0, 0.7), (25.0, 0.8)];
// beatoraja の GAUGE_LR2 の guts。30% 未満で減衰量を 60% に弱める。
pub(super) const BEATORAJA_LR2_GUTS: &[(f32, f32)] = &[(30.0, 0.6)];
// LR2oraja 0.8.3+ の LR2 HARD 系 guts。32% 未満で減衰量を 60% に弱める。
pub(super) const LR2_HARD_GUTS: &[(f32, f32)] = &[(32.0, 0.6)];
pub(super) const DX_HARD_GUTS: &[(f32, f32)] = &[(30.0, 0.5)];
