use super::*;

pub fn compile_gauge_definition(
    base: &GaugeDefinition,
    total: f64,
    total_notes: u32,
) -> GaugeRuntimeDefinition {
    compile_gauge_definition_for_rule(base, total, total_notes, RuleMode::Beatoraja)
}

pub(super) fn compile_gauge_definition_for_rule(
    base: &GaugeDefinition,
    total: f64,
    total_notes: u32,
    rule_mode: RuleMode,
) -> GaugeRuntimeDefinition {
    let mut values = base.values;
    if base.modifier == GaugeModifier::Pop && total_notes >= 1537 {
        // Endless Dream DX 9KEY: 辛ゲージ帯では GOOD の基礎回復量を2倍にする。
        values[GaugeJudgeIndex::Gd as usize] *= 2.0;
    }
    for value in &mut values {
        *value = if base.modifier == GaugeModifier::ModifyDamage
            && rule_mode == RuleMode::Beatoraja
            && *value < 0.0
        {
            *value * beatoraja_damage_scale(total, total_notes)
        } else {
            apply_modifier(*value, base.modifier, total, total_notes)
        };
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

pub(super) fn apply_modifier(
    value: f32,
    modifier: GaugeModifier,
    total: f64,
    total_notes: u32,
) -> f32 {
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
        GaugeModifier::Pop => {
            if value > 0.0 {
                if total_notes > 0 {
                    value * pop_total_value(total_notes) / total_notes as f32
                } else {
                    0.0
                }
            } else {
                value
            }
        }
    }
}

fn beatoraja_damage_scale(total: f64, total_notes: u32) -> f32 {
    let thresholds = [240.0, 230.0, 210.0, 200.0, 180.0, 160.0, 150.0, 130.0, 120.0];
    let scales: [f32; 10] = [1.0, 1.11, 1.25, 1.5, 1.666, 2.0, 2.5, 3.333, 5.0, 10.0];
    let total_scale = scales[thresholds.iter().take_while(|&&threshold| total < threshold).count()];
    let mut note_scale = 1.0_f32;
    let mut note = 1000_u32;
    let mut multiplier = 0.002_f32;
    // Preserve the upstream OR and signed subtraction, including negative terms.
    while note > total_notes || note > 1 {
        note_scale += multiplier * (i64::from(note) - i64::from(total_notes.max(note / 2))) as f32;
        note /= 2;
        multiplier *= 2.0;
    }
    total_scale.max(note_scale)
}

pub(super) fn modify_damage_scale(total: f64, total_notes: u32) -> f32 {
    let fix1_divisor = ((total / 16.0).floor() - 5.0).clamp(1.0, 10.0);
    let fix1 = (10.0 / fix1_divisor) as f32;

    let notes = total_notes as f32;
    let fix2 = if total_notes <= 20 {
        10.0
    } else if total_notes < 30 {
        8.0 + 0.2 * (30.0 - notes)
    } else if total_notes < 60 {
        5.0 + 0.2 * (60.0 - notes) / 3.0
    } else if total_notes < 125 {
        4.0 + (125.0 - notes) / 65.0
    } else if total_notes < 250 {
        3.0 + 0.008 * (250.0 - notes)
    } else if total_notes < 500 {
        2.0 + 0.004 * (500.0 - notes)
    } else if total_notes < 1000 {
        1.0 + 0.002 * (1000.0 - notes)
    } else {
        1.0
    };

    fix1.max(fix2)
}

pub(super) fn iidx_total_value(total_notes: u32) -> f32 {
    let notes = total_notes as f32;
    if notes <= 0.0 {
        return 0.0;
    }
    260.0_f32.max(7.605 * notes / (0.01 * notes + 6.5))
}

pub(super) fn pop_total_value(total_notes: u32) -> f32 {
    if total_notes == 0 {
        return 0.0;
    }
    if total_notes > 3072 {
        (0.097 * total_notes as f32).floor()
    } else {
        let multiplier = 3072 / total_notes;
        (multiplier as f32 * total_notes as f32 / 1024.0 * 100.0).min(300.0)
    }
}
