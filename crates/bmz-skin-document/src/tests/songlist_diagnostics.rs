use super::*;
use serde_json::json;

fn assert_error(value: JsonValue, path: &str, detail: &str) {
    for error in [
        SkinDocument::from_json_value(&value).unwrap_err(),
        serde_json::from_str::<SkinDocument>(&value.to_string()).unwrap_err(),
    ] {
        let message = error.to_string();
        assert!(message.contains(path), "{message}");
        assert!(message.contains(detail), "{message}");
        assert!(!message.contains("OneOrMany"), "{message}");
    }
}

#[test]
fn songlist_field_errors_name_the_field_and_array_entry() {
    for field in [
        "scorerank",
        "exscore",
        "scorerate",
        "minbp",
        "mincb",
        "maxcombo",
        "nextrank",
        "nextrankdiff",
        "nearestrank",
        "nearestrankdiff",
        "rivalscorerank",
        "rivaldiff",
        "favorite",
    ] {
        let mut songlist = serde_json::Map::new();
        songlist.insert(field.into(), json!({"mode":"digit"}));
        assert_error(
            json!({"songlist":songlist}),
            &format!("songlist.{field}:"),
            "unknown variant `digit`",
        );

        let mut songlist = serde_json::Map::new();
        songlist.insert(field.into(), json!([{"mode":"text"}, {"mode":"digits"}]));
        assert_error(
            json!({"songlist":songlist}),
            &format!("songlist.{field}[1]:"),
            "missing field `digits`",
        );
    }
}

#[test]
fn songlist_shape_errors_keep_the_definition_path() {
    for invalid in [JsonValue::Null, json!(3), json!("digits"), json!(false)] {
        assert_error(
            json!({"songlist":{"exscore":invalid}}),
            "songlist.exscore:",
            "expected an object or array",
        );
    }
    assert_error(
        json!({"songlist":{"rivaldiff":[{"mode":"text"},null]}}),
        "songlist.rivaldiff[1]:",
        "invalid type",
    );
}

#[test]
fn songlist_score_order_errors_name_the_invalid_entry() {
    assert_error(
        json!({"songlist":{"scoreOrder":["exscore","typo"]}}),
        "songlist.scoreOrder[1]:",
        "unknown variant `typo`",
    );
    assert_error(
        json!({"songlist":{"scoreOrder":["exscore",5]}}),
        "songlist.scoreOrder[1]:",
        "invalid type",
    );
    for invalid in [JsonValue::Null, json!("exscore"), json!({"first":"exscore"})] {
        assert_error(
            json!({"songlist":{"scoreOrder":invalid}}),
            "songlist.scoreOrder:",
            "expected an array",
        );
    }
}
