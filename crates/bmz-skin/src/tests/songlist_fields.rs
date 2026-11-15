use super::*;
use bmz_skin_document::{SongListField, SongListFieldRender};

#[test]
fn songlist_lua_definition_errors_include_the_field_and_entry() {
    let root = unique_test_dir("songlist-score-errors");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.luaskin");
    for (songlist, location, detail) in [
        (r#"exscore={mode="digit"}"#, "songlist.exscore:", "unknown variant `digit`"),
        (
            r#"rivaldiff={{mode="text"},{mode="digits"}}"#,
            "songlist.rivaldiff[1]:",
            "missing field `digits`",
        ),
        (r#"scoreOrder={"exscore","typo"}"#, "songlist.scoreOrder[1]:", "unknown variant `typo`"),
    ] {
        fs::write(&path, format!("return {{type=5,songlist={{{songlist}}}}}")).unwrap();
        let error = load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new())
            .expect_err("invalid songlist must reject the skin");
        let message = format!("{error:#}");
        assert!(message.contains(location), "{message}");
        assert!(message.contains(detail), "{message}");
    }
}

#[test]
fn songlist_fields_load_in_json_and_lua_without_global_value_refs() {
    let root = unique_test_dir("songlist-score-fields");
    fs::create_dir_all(&root).unwrap();
    let names = [
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
    ];
    let mut fields = serde_json::Map::new();
    let mut lua = String::from("return { type=5, songlist={ id='list', scoreOrder={");
    for name in names {
        lua.push_str(&format!("'{name}',"));
    }
    lua.push_str("},");
    for name in names {
        fields.insert(
            name.into(),
            serde_json::json!({"mode":"text","text":{"font":"f","size":20},
            "dst":[{"x":1,"y":2,"w":100,"h":20}]}),
        );
        lua.push_str(&format!(
            "{name}={{mode='text',text={{font='f',size=20}},dst={{{{x=1,y=2,w=100,h=20}}}}}},"
        ));
    }
    lua.push_str("}}");
    let path = root.join("select.luaskin");
    fs::write(&path, lua).unwrap();
    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let json: bmz_skin_document::SkinDocument = serde_json::from_value(serde_json::json!({
        "type":5,"songlist":serde_json::Value::Object(fields)
    }))
    .unwrap();
    let lua = loaded.document.songlist.as_ref().unwrap();
    let json = json.songlist.as_ref().unwrap();
    assert_eq!(lua.score_order, json.score_order);
    for field in &json.score_order {
        assert_eq!(lua.score_fields(*field), json.score_fields(*field));
    }
    assert!(loaded.document.value.is_empty());
    assert!(loaded.document.text.is_empty());

    let path = root.join("modes.luaskin");
    fs::write(&path,r#"return {type=5,songlist={
        scorerank={{mode="image",images={"F","E","D","C","B","A","AA","AAA","MAX"},dst={{w=20,h=10}}},
                   {mode="text",text={font="f"},dst={{w=20,h=10}}}},
        favorite={mode="text",labels={"none","chart","song","both"},text={},format={},dst={{w=20,h=10}}},
        rivaldiff={mode="digits",digits={src="atlas",divx=12,divy=2,digit=6},
            glyphs={plus=11.0,minus=23.0},format={showPlus=true,decimals=2.0},dst={{w=10,h=20}}}
    }}"#).unwrap();
    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let list = loaded.document.songlist.as_ref().unwrap();
    assert_eq!(list.scorerank.len(), 2);
    assert!(
        matches!(&list.scorerank[0].render,SongListFieldRender::Image{images} if images.len()==9)
    );
    assert!(
        matches!(&list.rivaldiff[0].render,SongListFieldRender::Digits{glyphs,..} if glyphs.minus==Some(23))
    );
    assert_eq!(list.score_fields(SongListField::Favorite).len(), 1);
}
