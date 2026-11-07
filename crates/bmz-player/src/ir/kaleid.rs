//! Provider acceptance is separate from best-effort playback and local persistence.
use bmz_chart::kaleid::KaleidProvenance;

pub fn bms_ir_block_reason(chart: &KaleidProvenance) -> Option<&'static str> {
    if chart.version != bmz_chart::kaleid::INTERPRETATION_VERSION {
        return Some(
            "BMS-IR: このプレイの拡張解釈情報がありません。旧結果を再スキャンだけで送信可能にはできません。",
        );
    }
    if chart.partial() {
        return Some("BMS-IR: 部分対応・不正拡張を含む結果の受入条件は未確認です。");
    }
    let features = chart.source.union(chart.effective);
    if features.hln
        || features.conditional
        || features.free_zone
        || features.potion
        || features.dummy
    {
        return Some(
            "BMS-IR: HLN・動的分岐・フリーゾーン・ポーション・ダミーの受入条件は未確認です。FORCEやEXTRA NOTE適用後も元譜面の機能で判定します。",
        );
    }
    None
}

pub fn rian_ir_block_reason(chart: &KaleidProvenance) -> Option<&'static str> {
    if chart.partial() {
        return Some("rianIR: この部分対応結果がED-rianと同じ解釈になることを確認できていません。");
    }
    if chart.version > bmz_chart::kaleid::INTERPRETATION_VERSION {
        return Some("rianIR: この結果のKaleid解釈バージョンに対応していません。");
    }
    None
}

pub fn block_reason(
    provider: &crate::config::profile_config::IrProviderConfig,
    chart: &KaleidProvenance,
) -> Option<&'static str> {
    if super::bms_ir::is_bms_ir_config(provider) {
        bms_ir_block_reason(chart)
    } else if super::rian_ir::is_rian_ir_config(provider) {
        rian_ir_block_reason(chart)
    } else {
        None
    }
}

pub fn ensure_course_supported(
    provider: &crate::config::profile_config::IrProviderConfig,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    if !super::bms_ir::is_bms_ir_config(provider) && !super::rian_ir::is_rian_ir_config(provider) {
        return Ok(());
    }
    let entries =
        payload.pointer("/play_options/entry_kaleid").and_then(serde_json::Value::as_array);
    // Old rianIR course jobs retain their original eligibility. BMS-IR needs evidence for every stage.
    if let Some(entries) = entries {
        let chart_count = payload
            .pointer("/course/charts")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        if entries.len() != chart_count {
            anyhow::bail!("course Kaleid evidence does not cover every chart");
        }
        for entry in entries {
            let provenance: KaleidProvenance = serde_json::from_value(entry.clone())?;
            if let Some(reason) = block_reason(provider, &provenance) {
                anyhow::bail!(reason);
            }
        }
    } else if super::bms_ir::is_bms_ir_config(provider) {
        anyhow::bail!("BMS-IR: コース各曲の拡張解釈情報がありません。");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::IrProviderConfig;
    use bmz_chart::kaleid::KaleidFeatures;

    #[test]
    fn provider_matrix_uses_source_and_effective_features_but_not_unused_declarations() {
        let bmz = IrProviderConfig::bmz_ir();
        let rian = IrProviderConfig::rian_ir();
        let bms = IrProviderConfig::bms_ir();
        let normal = KaleidProvenance::imported();
        assert!(block_reason(&bms, &normal).is_none()); // Includes extension-only .bmc.
        let mut declared = normal.clone();
        declared.declared.hln = true;
        assert!(block_reason(&bms, &declared).is_none());
        for features in [
            KaleidFeatures { hln: true, ..Default::default() },
            KaleidFeatures { conditional: true, ..Default::default() },
            KaleidFeatures { free_zone: true, ..Default::default() },
            KaleidFeatures { potion: true, ..Default::default() },
            KaleidFeatures { dummy: true, ..Default::default() },
            KaleidFeatures { hln: true, potion: true, conditional: true, ..Default::default() },
        ] {
            for source in [true, false] {
                let chart = KaleidProvenance {
                    source: if source { features } else { Default::default() },
                    effective: if source { Default::default() } else { features },
                    ..normal.clone()
                };
                assert!(block_reason(&bmz, &chart).is_none());
                assert!(block_reason(&rian, &chart).is_none());
                assert!(block_reason(&bms, &chart).is_some());
            }
        }
        let audio_visual = KaleidProvenance {
            source: KaleidFeatures { typed_end: true, bar_line: true, ..Default::default() },
            ..normal.clone()
        };
        assert!(block_reason(&bms, &audio_visual).is_none());
        let partial =
            KaleidProvenance { diagnostics: vec!["invalid object ignored".into()], ..normal };
        assert!(block_reason(&bmz, &partial).is_none());
        assert!(block_reason(&rian, &partial).is_some());
        assert!(block_reason(&bms, &partial).is_some());
        assert!(block_reason(&bms, &KaleidProvenance::default()).is_some());
    }

    #[test]
    fn courses_apply_the_same_policy_to_unplayed_stages_and_legacy_jobs() {
        let mut payload = serde_json::json!({"course": {"charts": ["a", "b"]},
            "play_options": {"entry_kaleid": [KaleidProvenance::imported(), KaleidProvenance::imported()]}});
        assert!(ensure_course_supported(&IrProviderConfig::bms_ir(), &payload).is_ok());
        payload["play_options"]["entry_kaleid"][1]["source"]["conditional"] = true.into();
        assert!(ensure_course_supported(&IrProviderConfig::bms_ir(), &payload).is_err());
        assert!(ensure_course_supported(&IrProviderConfig::rian_ir(), &payload).is_ok());
        assert!(ensure_course_supported(&IrProviderConfig::bmz_ir(), &payload).is_ok());
        assert!(
            ensure_course_supported(&IrProviderConfig::bms_ir(), &serde_json::json!({})).is_err()
        );
    }
}
