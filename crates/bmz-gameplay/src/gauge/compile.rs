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
        legacy: (rule_mode == RuleMode::Legacy)
            .then(|| legacy_gauge_rule(base.gauge_type, total, total_notes))
            .flatten(),
    }
}

/// OpenLR2 `judge_damage` for a single song (LR2_bmsload.cpp). `total` is the
/// integer LR2 TOTAL. The C types are kept: `total / (float)notes` is a float
/// division, while the HARD damage factor uses doubles.
pub fn legacy_gauge_rule(
    gauge_type: GaugeType,
    total: f64,
    total_notes: u32,
) -> Option<LegacyGaugeRule> {
    let table = legacy_judge_damage(total, total_notes)?;
    let (index, guts, floor_two) = match gauge_type {
        GaugeType::Normal => (0, false, true),
        GaugeType::Hard => (1, true, false),
        GaugeType::Hazard => (2, true, false),
        GaugeType::Easy => (3, false, true),
        _ => return None,
    };
    Some(LegacyGaugeRule { damage: table[index], guts, floor_two })
}

/// OpenLR2's whole `judge_damage[gauge][judge]` table for a single song.
/// Gauges: GROOVE, HARD, DEATH, EASY, P-ATTACK, G-ATTACK. Judges: EMPTY POOR,
/// POOR, BAD, GOOD, GREAT, PGREAT. `None` without notes, where LR2 leaves the
/// table at 0.
pub fn legacy_judge_damage(total: f64, total_notes: u32) -> Option<[[f64; 6]; 6]> {
    if total_notes == 0 {
        return None;
    }
    let total_int = total as i32;
    let notes = total_notes as i32;
    let per_note = (total_int as f32 / notes as f32) as f64;
    let per_good = (total_int as f32 / (notes + notes) as f32) as f64;
    let notes_f = f64::from(notes);
    let notebase = if notes < 20 {
        10.0
    } else if notes < 30 {
        10.0 - (notes_f - 20.0) / 10.0 * 2.0
    } else if notes < 45 {
        7.0 - (notes_f - 30.0) / 15.0
    } else if notes < 60 {
        6.0 - (notes_f - 45.0) / 15.0
    } else if notes < 125 {
        5.0 - (notes_f - 60.0) / 65.0
    } else if notes < 250 {
        4.0 - (notes_f - 125.0) / 125.0
    } else if notes < 500 {
        3.0 - (notes_f - 250.0) / 250.0
    } else if notes < 1000 {
        2.0 - (notes_f - 500.0) / 500.0
    } else {
        1.0
    };
    let mut recover = ((f64::from(total_int) - 80.0) * 0.125 / 2.0) as i32;
    if recover <= 0 {
        recover = 1;
    }
    let totalbase = 100.0 / f64::from(recover);
    let dmg = (notebase * 10.0).max(totalbase) / 10.0;
    Some([
        [-2.0, -6.0, -4.0, per_good, per_note, per_note],
        [dmg * -2.0, dmg * -10.0, dmg * -6.0, 0.05, 0.1, 0.1],
        [0.0, -100.0, -100.0, 0.0, 0.0, 0.0],
        [-1.6, -4.800000000000001, -3.2, per_good * 1.2, per_note * 1.2, per_note * 1.2],
        [-100.0, -100.0, -100.0, -100.0, -1.0, 0.1],
        [dmg * -2.0, dmg * -10.0, -6.0, 0.1, -1.0, dmg * -10.0],
    ])
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
