//! beatoraja skin-facing property names, resolved within their factory family.
//!
//! The checked-in tables are generated from the pinned upstream Java sources.
//! IDs describe names only; a recognized name does not imply runtime support.

mod generated;

/// Property factories have separate name spaces, even when names or IDs overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyFamily {
    Integer,
    Index,
    Rate,
    Float,
    String,
    Boolean,
}

/// A Boolean name and the parity of its leading `!` operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedBooleanProperty {
    pub id: i32,
    pub negated: bool,
}

/// Resolve an exact, case-sensitive name using the matching beatoraja factory.
/// Boolean negation is accepted only by [`resolve_boolean_property_name`].
pub fn resolve_property_name(family: PropertyFamily, name: &str) -> Option<i32> {
    use generated::*;
    match family {
        PropertyFamily::Integer => {
            lookup(INTEGER_PATTERN_NAMES, name).or_else(|| lookup(INTEGER_NAMES, name))
        }
        PropertyFamily::Index => {
            lookup(INDEX_PATTERN_NAMES, name).or_else(|| lookup(INDEX_NAMES, name))
        }
        PropertyFamily::Rate => lookup(RATE_NAMES, name),
        PropertyFamily::Float => lookup(FLOAT_PATTERN_NAMES, name)
            .or_else(|| lookup(FLOAT_NAMES, name))
            .or_else(|| lookup(RATE_NAMES, name)),
        PropertyFamily::String => {
            lookup_numbered(STRING_PATTERNS, name).or_else(|| lookup(STRING_NAMES, name))
        }
        PropertyFamily::Boolean => {
            lookup_numbered(BOOLEAN_PATTERNS, name).or_else(|| lookup(BOOLEAN_NAMES, name))
        }
    }
}

/// Resolve repeated leading `!` without recursion or losing negation for ID zero.
pub fn resolve_boolean_property_name(name: &str) -> Option<ResolvedBooleanProperty> {
    let bare = name.trim_start_matches('!');
    let id = resolve_property_name(PropertyFamily::Boolean, bare)?;
    Some(ResolvedBooleanProperty { id, negated: (name.len() - bare.len()) % 2 == 1 })
}

fn lookup(table: &[(&str, i32)], name: &str) -> Option<i32> {
    table.binary_search_by_key(&name, |&(key, _)| key).ok().map(|index| table[index].1)
}

struct NumberedProperty {
    prefix: &'static str,
    suffix: &'static str,
    first_id: i32,
    count: i32,
    direction: i32,
    offset: i32,
}

impl NumberedProperty {
    const fn new(
        prefix: &'static str,
        suffix: &'static str,
        first_id: i32,
        count: i32,
        direction: i32,
        offset: i32,
    ) -> Self {
        Self { prefix, suffix, first_id, count, direction, offset }
    }

    fn resolve(&self, name: &str) -> Option<i32> {
        let number = name.strip_prefix(self.prefix)?.strip_suffix(self.suffix)?;
        let value = parse_java_int(number)?;
        let index = value.wrapping_mul(self.direction).wrapping_add(self.offset);
        (0..self.count).contains(&index).then(|| self.first_id + index)
    }
}

fn lookup_numbered(patterns: &[NumberedProperty], name: &str) -> Option<i32> {
    patterns.iter().find_map(|pattern| pattern.resolve(name))
}

/// Java Integer.parseInt accepts an ASCII sign and Character.digit(char, 10).
/// Its UTF-16 char loop accepts BMP decimal digits but rejects supplementary ones.
fn parse_java_int(text: &str) -> Option<i32> {
    let (negative, digits) = if let Some(digits) = text.strip_prefix('-') {
        (true, digits)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    if digits.is_empty() {
        return None;
    }
    // Accumulate negatively so Java's -2147483648 remains representable.
    let mut value = 0_i32;
    for character in digits.chars() {
        value = value.checked_mul(10)?.checked_sub(java_decimal_digit(character)?)?;
    }
    if negative { Some(value) } else { value.checked_neg() }
}

fn java_decimal_digit(character: char) -> Option<i32> {
    // The BMP Nd blocks accepted by Character.digit(char, 10), including ASCII.
    const ZEROES: &[u32] = &[
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10,
    ];
    let value = character as u32;
    ZEROES
        .iter()
        .find_map(|&zero| value.checked_sub(zero).filter(|&digit| digit < 10))
        .map(|digit| digit as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fixed_enum_names_and_exact_patterns_are_present_once_and_resolve() {
        use generated::*;
        let fixed = [
            (PropertyFamily::Integer, INTEGER_NAMES, 148),
            (PropertyFamily::Index, INDEX_NAMES, 62),
            (PropertyFamily::Rate, RATE_NAMES, 31),
            (PropertyFamily::Float, FLOAT_NAMES, 29),
            (PropertyFamily::Boolean, BOOLEAN_NAMES, 216),
            (PropertyFamily::String, STRING_NAMES, 26),
        ];
        assert_eq!(fixed.iter().map(|(_, table, _)| table.len()).sum::<usize>(), 512);
        for (family, table, count) in fixed.into_iter().chain([
            (PropertyFamily::Integer, INTEGER_PATTERN_NAMES, 64),
            (PropertyFamily::Index, INDEX_PATTERN_NAMES, 20),
            (PropertyFamily::Float, FLOAT_PATTERN_NAMES, 11),
        ]) {
            assert_eq!(table.len(), count);
            assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
            for &(name, id) in table {
                assert_eq!(resolve_property_name(family, name), Some(id), "{family:?}: {name}");
            }
        }
        // Keep every enum declaration, including mixed-case public names.
        assert_eq!(resolve_property_name(PropertyFamily::Index, "cleartype"), Some(370));
        assert_eq!(resolve_property_name(PropertyFamily::Index, "cleartype_target"), Some(371));
        assert_eq!(resolve_property_name(PropertyFamily::String, "irUserName"), Some(1021));
    }

    #[test]
    fn names_are_family_scoped_and_keep_public_case_and_spelling() {
        use PropertyFamily::*;
        for (family, name, expected) in [
            (Integer, "score_rate", Some(102)),
            (Float, "score_rate", Some(1102)),
            (Rate, "score_rate", None),
            (Index, "score_rate", None),
            (Rate, "music_progress", Some(6)),
            (Float, "music_progress", Some(6)),
            (Float, "ir_player_perfect_rate", Some(223)),
            (Rate, "ir_player_perfect_rate", None),
            (Integer, "playtime_totla_saecond", Some(19)),
            (Integer, "playtime_total_second", None),
            (Integer, "folder_prefect", Some(329)),
            (Integer, "folder_perfect", None),
            (Float, "timign_stddev", Some(376)),
            (String, "irusername", None),
        ] {
            assert_eq!(resolve_property_name(family, name), expected, "{family:?}: {name}");
        }
    }

    #[test]
    fn exact_integer_patterns_reject_numeric_aliases() {
        use PropertyFamily::*;
        for (family, name, expected) in [
            (Integer, "ranking_exscore1", Some(380)),
            (Integer, "ranking_exscore10", Some(389)),
            (Integer, "ranking_index10", Some(399)),
            (Index, "playertype_ranking10", Some(389)),
            (Index, "cleartype_ranking1", Some(390)),
            (Integer, "ranking_exscore0", None),
            (Integer, "ranking_exscore11", None),
            (Integer, "ranking_exscore01", None),
            (Integer, "ranking_exscore+1", None),
            (Integer, "ranking_exscore１", None),
            (Index, "ranking_exscore1", None),
        ] {
            assert_eq!(resolve_property_name(family, name), expected);
        }
    }

    #[test]
    fn numbered_string_ranges_keep_gaps_reversed_ids_and_label_value_aliases() {
        assert_eq!(generated::STRING_PATTERNS.len(), 13);
        assert_eq!(generated::BOOLEAN_PATTERNS.len(), 2);
        for pattern in generated::STRING_PATTERNS.iter().chain(generated::BOOLEAN_PATTERNS) {
            for index in [-1, 0, pattern.count - 1, pattern.count] {
                let number = (index - pattern.offset) * pattern.direction;
                let name = format!("{}{number}{}", pattern.prefix, pattern.suffix);
                let expected =
                    (0..pattern.count).contains(&index).then(|| pattern.first_id + index);
                assert_eq!(pattern.resolve(&name), expected, "{name}");
            }
        }
        for (name, id) in [
            ("key1", 40),
            ("key10", 49),
            ("key11", 240),
            ("key54", 283),
            ("skincategory1", 100),
            ("skincategory10", 109),
            ("skinitem1", 110),
            ("skinitem10", 119),
            ("rankingname1", 120),
            ("rankingname10", 129),
            ("coursetitle1", 150),
            ("coursetitle10", 159),
            ("targetnamep1", 209),
            ("targetnamep10", 200),
            ("targetnamen1", 210),
            ("targetnamen10", 219),
            ("practice_item1", 1040),
            ("practice_item16", 1055),
            ("practice_item1_label", 1060),
            ("practice_item_label16", 1075),
            ("practice_item1_value", 1080),
            ("practice_item_value16", 1095),
        ] {
            assert_eq!(resolve_property_name(PropertyFamily::String, name), Some(id), "{name}");
        }
        for name in ["key0", "key55", "targetnamep0", "targetnamep11", "practice_item17"] {
            assert_eq!(resolve_property_name(PropertyFamily::String, name), None, "{name}");
        }
    }

    #[test]
    fn parsed_number_patterns_follow_java_sign_digits_and_overflow_rules() {
        for name in ["key+01", "key0001", "key１", "key١"] {
            assert_eq!(resolve_property_name(PropertyFamily::String, name), Some(40), "{name}");
        }
        for name in ["practice_item+016_selected", "practice_item００１６_selected"] {
            assert_eq!(resolve_property_name(PropertyFamily::Boolean, name), Some(3035));
        }
        for name in [
            "key",
            "key 1",
            "key1 ",
            "key1_",
            "key-1",
            "key𝟙",
            "key2147483648",
            "key-2147483648",
            "key-2147483649",
        ] {
            assert_eq!(resolve_property_name(PropertyFamily::String, name), None, "{name}");
        }
        assert_eq!(parse_java_int("+2147483647"), Some(i32::MAX));
        assert_eq!(parse_java_int("-2147483648"), Some(i32::MIN));
        assert_eq!(parse_java_int("-0"), Some(0));
        assert_eq!(parse_java_int("+"), None);
    }

    #[test]
    fn boolean_negation_preserves_parity_and_unknown_names() {
        for (name, id, negated) in [
            ("practice_item1", 3000, false),
            ("!practice_item16", 3015, true),
            ("!!practice_item1_selected", 3020, false),
            ("!!!practice_item16_selected", 3035, true),
        ] {
            assert_eq!(
                resolve_boolean_property_name(name),
                Some(ResolvedBooleanProperty { id, negated })
            );
        }
        assert_eq!(resolve_property_name(PropertyFamily::Boolean, "!practice_item1"), None);
        for name in
            ["", "!", "!!unknown", "practice_item0", "practice_item17_selected", "chart_7key "]
        {
            assert_eq!(resolve_boolean_property_name(name), None, "{name}");
        }
        for family in [
            PropertyFamily::Integer,
            PropertyFamily::Index,
            PropertyFamily::Rate,
            PropertyFamily::Float,
            PropertyFamily::String,
            PropertyFamily::Boolean,
        ] {
            for name in ["", "123", "unknown_property"] {
                assert_eq!(resolve_property_name(family, name), None);
            }
        }
    }
}
