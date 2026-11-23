use super::*;

/// beatoraja `BMSPlayerRule.calculateDefaultTotal` 相当。
pub fn default_gauge_total(total_notes: u32) -> f64 {
    let notes = total_notes as f64;
    if notes <= 0.0 {
        return 260.0;
    }
    260.0_f64.max(7.605 * notes / (0.01 * notes + 6.5))
}

/// LR2oraja endlessdream `BMSPlayerRule.calculateDefaultTotal` 相当。
pub fn lr2oraja_default_gauge_total(total_notes: u32) -> f64 {
    let notes = total_notes as f64;
    let extra = total_notes.saturating_sub(400).min(200) as f64;
    160.0 + (notes + extra) * 0.16
}

/// 譜面メタの `#TOTAL` が未指定または 0 以下のとき beatoraja 既定式へフォールバックする。
pub fn gauge_total_for_chart(metadata_total: Option<f64>, total_notes: u32) -> f64 {
    metadata_total.filter(|total| *total > 0.0).unwrap_or_else(|| default_gauge_total(total_notes))
}

/// rule mode 別に、譜面メタの `#TOTAL` 未指定時の既定 TOTAL を返す。
pub fn gauge_total_for_chart_and_rule_mode(
    metadata_total: Option<f64>,
    total_notes: u32,
    rule_mode: RuleMode,
) -> f64 {
    if rule_mode == RuleMode::Legacy {
        // LR2 keeps TOTAL as an int and replaces values below 1.
        return metadata_total
            .map(|total| f64::from(total as i32))
            .filter(|total| *total >= 1.0)
            .unwrap_or_else(|| openlr2_default_gauge_total(total_notes));
    }
    metadata_total.filter(|total| *total > 0.0).unwrap_or_else(|| match rule_mode {
        RuleMode::Beatoraja | RuleMode::Dx => default_gauge_total(total_notes),
        RuleMode::Lr2Oraja | RuleMode::Legacy => lr2oraja_default_gauge_total(total_notes),
    })
}

/// OpenLR2's default TOTAL (LR2_bmsload.cpp), truncated to its int TOTAL.
pub fn openlr2_default_gauge_total(total_notes: u32) -> f64 {
    let notes = total_notes as i32;
    let calc = if notes < 400 {
        f64::from(notes) / 5.0 + 200.0
    } else if notes < 600 {
        f64::from(notes - 400) / 2.5 + 280.0
    } else {
        f64::from(notes - 600) / 5.0 + 360.0
    };
    f64::from((calc * 0.8) as i32)
}

pub fn gauge_total_for_play(
    metadata: &bmz_chart::model::ChartMetadata,
    total_notes: u32,
    rule_mode: RuleMode,
) -> f64 {
    if let Some(percent) = metadata.bmson_total_percent {
        let base = gauge_total_for_chart_and_rule_mode(None, total_notes, rule_mode);
        return if percent > 0.0 { base * (percent / 100.0) } else { base };
    }
    gauge_total_for_chart_and_rule_mode(metadata.total, total_notes, rule_mode)
}
