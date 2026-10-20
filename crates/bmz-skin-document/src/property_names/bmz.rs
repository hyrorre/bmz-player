//! BMZ extension property names. Kept apart from the generated beatoraja tables.

use super::PropertyFamily;
use crate::{
    SKIN_OPTION_BMZ_CHART_MISS_TEXT_ACTIVE, SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE,
    SKIN_REF_BMZ_SELECT_MODE_INDEX, SKIN_TEXT_BMZ_CHART_MISS_TEXT, SKIN_TEXT_BMZ_CHART_TEXT,
};

const INTEGER_NAMES: &[(&str, i32)] = &[("bmz_select_mode_index", SKIN_REF_BMZ_SELECT_MODE_INDEX)];

const STRING_NAMES: &[(&str, i32)] = &[
    ("bmz_chart_miss_text", SKIN_TEXT_BMZ_CHART_MISS_TEXT),
    ("bmz_chart_text", SKIN_TEXT_BMZ_CHART_TEXT),
];

const BOOLEAN_NAMES: &[(&str, i32)] = &[
    ("bmz_chart_miss_text_active", SKIN_OPTION_BMZ_CHART_MISS_TEXT_ACTIVE),
    ("bmz_chart_text_visible", SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE),
];

pub(super) fn resolve(family: PropertyFamily, name: &str) -> Option<i32> {
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
