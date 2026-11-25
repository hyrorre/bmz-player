//! BMZ extension property names. Kept apart from the generated beatoraja tables.

use bmz_core::lane::LANE_COUNT;

use super::PropertyFamily;
use crate::{
    SKIN_OPTION_BMZ_CHART_MISS_TEXT_ACTIVE, SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE,
    SKIN_REF_BMZ_LANE_BP_BASE, SKIN_REF_BMZ_LANE_NOTES_BASE, SKIN_REF_BMZ_LANE_RANK_BASE,
    SKIN_REF_BMZ_SELECT_MODE_INDEX, SKIN_TEXT_BMZ_CHART_MISS_TEXT, SKIN_TEXT_BMZ_CHART_TEXT,
    SKIN_TEXT_BMZ_LANE_EX_RATE_BASE,
};

const INTEGER_NAMES: &[(&str, i32)] = &[("bmz_select_mode_index", SKIN_REF_BMZ_SELECT_MODE_INDEX)];

const LANE_SUFFIXES: [&str; LANE_COUNT] = [
    "scratch", "key1", "key2", "key3", "key4", "key5", "key6", "key7", "key8", "key9", "key10",
    "key11", "key12", "key13", "key14", "scratch2",
];

const STRING_NAMES: &[(&str, i32)] = &[
    ("bmz_chart_miss_text", SKIN_TEXT_BMZ_CHART_MISS_TEXT),
    ("bmz_chart_text", SKIN_TEXT_BMZ_CHART_TEXT),
];

const BOOLEAN_NAMES: &[(&str, i32)] = &[
    ("bmz_chart_miss_text_active", SKIN_OPTION_BMZ_CHART_MISS_TEXT_ACTIVE),
    ("bmz_chart_text_visible", SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE),
];

pub(super) fn resolve(family: PropertyFamily, name: &str) -> Option<i32> {
    if let Some(lane_property) = name.strip_prefix("bmz_lane_") {
        let (property, lane) = lane_property.rsplit_once('_')?;
        let base = match (family, property) {
            (PropertyFamily::Integer, "notes") => SKIN_REF_BMZ_LANE_NOTES_BASE,
            (PropertyFamily::Integer | PropertyFamily::String, "rank") => {
                SKIN_REF_BMZ_LANE_RANK_BASE
            }
            (PropertyFamily::String, "ex_rate") => SKIN_TEXT_BMZ_LANE_EX_RATE_BASE,
            (PropertyFamily::Integer, "bp") => SKIN_REF_BMZ_LANE_BP_BASE,
            _ => return None,
        };
        return LANE_SUFFIXES
            .iter()
            .position(|&suffix| suffix == lane)
            .map(|index| base + index as i32);
    }
    let table = match family {
        PropertyFamily::Integer => INTEGER_NAMES,
        PropertyFamily::String => STRING_NAMES,
        PropertyFamily::Boolean => BOOLEAN_NAMES,
        _ => return None,
    };
    table.iter().find(|(key, _)| *key == name).map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{BOOLEAN_NAMES, INTEGER_NAMES, STRING_NAMES};

    #[test]
    fn bmz_names_are_family_scoped_and_do_not_shadow_beatoraja_names() {
        for (family, table) in [
            (PropertyFamily::Integer, INTEGER_NAMES),
            (PropertyFamily::String, STRING_NAMES),
            (PropertyFamily::Boolean, BOOLEAN_NAMES),
        ] {
            for &(name, id) in table {
                assert!(name.starts_with("bmz_"), "{name}");
                assert_eq!(resolve_beatoraja_property_name(family, name), None, "{name}");
                assert_eq!(resolve_property_name(family, name), Some(id), "{name}");
            }
        }
        for (family, property, base) in [
            (PropertyFamily::Integer, "notes", crate::SKIN_REF_BMZ_LANE_NOTES_BASE),
            (PropertyFamily::Integer, "rank", crate::SKIN_REF_BMZ_LANE_RANK_BASE),
            (PropertyFamily::String, "rank", crate::SKIN_REF_BMZ_LANE_RANK_BASE),
            (PropertyFamily::String, "ex_rate", crate::SKIN_TEXT_BMZ_LANE_EX_RATE_BASE),
            (PropertyFamily::Integer, "bp", crate::SKIN_REF_BMZ_LANE_BP_BASE),
        ] {
            for (index, suffix) in super::LANE_SUFFIXES.iter().enumerate() {
                let name = format!("bmz_lane_{property}_{suffix}");
                assert_eq!(resolve_beatoraja_property_name(family, &name), None);
                assert_eq!(resolve_property_name(family, &name), Some(base + index as i32));
            }
        }
        assert_eq!(resolve_property_name(PropertyFamily::String, "bmz_chart_text"), Some(20_100));
        assert_eq!(
            resolve_property_name(PropertyFamily::String, "bmz_chart_miss_text"),
            Some(20_103)
        );
        assert_eq!(
            resolve_property_name(PropertyFamily::Boolean, "bmz_chart_miss_text_active"),
            Some(20_104)
        );
        assert_eq!(resolve_property_name(PropertyFamily::Integer, "bmz_chart_text"), None);
        assert_eq!(
            resolve_property_name(PropertyFamily::Integer, "bmz_select_mode_index"),
            Some(19_250)
        );
        // The beatoraja `mode` IndexType name keeps resolving to the legacy ref=11.
        assert_eq!(resolve_property_name(PropertyFamily::Index, "mode"), Some(11));
        for family in [PropertyFamily::Index, PropertyFamily::String, PropertyFamily::Boolean] {
            assert_eq!(resolve_property_name(family, "bmz_select_mode_index"), None);
        }
        assert_eq!(
            resolve_boolean_property_name("!bmz_chart_text_visible"),
            Some(ResolvedBooleanProperty { id: 20_101, negated: true })
        );
    }
}
