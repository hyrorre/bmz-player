use super::*;
use bmz_skin_document::{LR2_NUMBER_BASE, LR2_OPTION_BASE};

#[test]
fn lr2_bar_classification_if_conditions_are_evaluated_at_runtime() {
    let value = load(
        5,
        "#IMAGE,a.png\n#IF,4\n#SRC_IMAGE,0,0,0,0,20,20,1,1,0,0\n#DST_IMAGE,0,0,0,0,20,20,0,255,255,255,255,1,0,0,0,0,0,5,0,0\n#ENDIF\n",
    );
    assert_eq!(value["destination"][0]["op"], json!([LR2_OPTION_BASE + 4, LR2_OPTION_BASE + 5]));
}

fn load(skin_type: i32, body: &str) -> JsonValue {
    let root = unique_test_dir(&format!("bmz-lr2-refs-{skin_type}"));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.lr2skin");
    fs::write(&path, format!("#INFORMATION,{skin_type},refs,BMZ\n#IMAGE,a.png\n{body}")).unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    fs::remove_dir_all(root).unwrap();
    loaded.value
}

fn number(num: i32) -> String {
    format!(
        "#SRC_NUMBER,0,0,0,0,110,20,11,1,0,0,{num},0,4,0,1\n\
         #DST_NUMBER,0,0,10,10,10,20,0,255,255,255,255,0,0,0,0,0,0\n"
    )
}

fn number_refs(document: &JsonValue) -> Vec<JsonValue> {
    document["value"].as_array().unwrap().iter().map(|value| value["ref"].clone()).collect()
}

#[test]
fn lr2_numbers_with_other_meanings_use_the_lr2_namespace() {
    // Play: 1P score / gauge / target diff / next rank / POOR with empty POOR.
    let play = load(0, &[100, 107, 108, 109, 114, 101, 290].map(number).concat());
    let lr2 = |id: i32| json!(LR2_NUMBER_BASE + id);
    assert_eq!(
        number_refs(&play),
        vec![lr2(100), lr2(107), lr2(108), lr2(109), lr2(114), json!(101), json!(90)]
    );
    // Select: IR rank / players / clear rate / rival diff, and the 290/291 BPM alias.
    let select = load(5, &[92, 93, 94, 95, 291, 96].map(number).concat());
    assert_eq!(
        number_refs(&select),
        vec![lr2(92), lr2(93), lr2(94), lr2(95), json!(91), json!(96)]
    );
    // Result 1P numbers keep the flip namespace; the renderer applies LR2's definitions.
    let result = load(7, &number(114));
    assert_eq!(number_refs(&result), vec![json!(bmz_skin_document::LR2_RESULT_NUMBER_BASE + 14)]);
}

#[test]
fn lr2_options_with_other_meanings_use_the_lr2_namespace() {
    let image = |ops: &str| {
        format!(
            "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
             #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,0,0,0,0,0,0,{ops}\n"
        )
    };
    let op = |document: &JsonValue, index: usize| document["destination"][index]["op"].clone();
    let lr2 = |id: i32| json!(LR2_OPTION_BASE + id);

    // Select song lamps / ranks are draw-time conditions, also inside #IF.
    let select = load(
        5,
        &format!("{}{}#IF,110\n{}#ENDIF\n", image("100,-117,0"), image("52,621,0"), image("0,0,0")),
    );
    assert_eq!(op(&select, 0), json!([LR2_OPTION_BASE + 100, -(LR2_OPTION_BASE + 117)]));
    assert_eq!(op(&select, 1), json!([lr2(52), lr2(621)]));
    assert_eq!(op(&select, 2), json!([lr2(110)]));

    // Fixed options resolve at load time: EXTRA is off and the ranking view is closed.
    let fixed = load(
        5,
        &format!("#IF,53\n{}#ENDIF\n#IF,52,620\n{}#ENDIF\n", image("0,0,0"), image("0,0,0")),
    );
    assert_eq!(fixed["destination"].as_array().unwrap().len(), 1);
    assert_eq!(op(&fixed, 0), json!([]));

    // Play keeps the beatoraja meaning of song options; result maps the new rank 340..347.
    assert_eq!(op(&load(0, &image("100,0,0")), 0), json!([100]));
    assert_eq!(op(&load(7, &image("340,-347,0")), 0), json!([lr2(340), -(LR2_OPTION_BASE + 347)]));
}

#[test]
fn lr2_select_values_with_other_meanings_use_the_lr2_namespace() {
    let lr2 = |id: i32| json!(LR2_NUMBER_BASE + id);
    // Judge timing, difficulty levels, EQ, my best score / rate and IR clear classes.
    let select = load(5, &[12, 45, 54, 57, 70, 73, 210, 219].map(number).concat());
    assert_eq!(
        number_refs(&select),
        vec![lr2(12), lr2(45), lr2(54), json!(57), lr2(70), lr2(73), lr2(210), lr2(219)]
    );
    // Extended LR2 result FAST / SLOW / combo break.
    let result = load(7, &[212, 214, 216].map(number).concat());
    assert_eq!(number_refs(&result), vec![json!(423), json!(424), json!(425)]);
    // Random course settings have no BMZ text.
    let text = load(
        5,
        "#FONT,20,1,0\n#SRC_TEXT,0,0,190,0\n#DST_TEXT,0,0,0,0,10,10,0,255,255,255,255,0,0,0,0,0,0\n#SRC_TEXT,0,0,10,0\n#DST_TEXT,0,0,0,0,10,10,0,255,255,255,255,0,0,0,0,0,0\n",
    );
    let refs =
        text["text"].as_array().unwrap().iter().map(|t| t["ref"].clone()).collect::<Vec<_>>();
    assert_eq!(refs, vec![json!(bmz_skin_document::LR2_TEXT_BASE + 190), json!(10)]);
}

#[test]
fn lr2_select_panel_and_ir_connection_are_draw_time_conditions() {
    let image = "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
                 #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n";
    let select =
        load(5, &format!("#IF,20\n{image}#ENDIF\n#IF,51\n{image}#ENDIF\n#IF,50\n{image}#ENDIF\n"));
    let ops = select["destination"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["op"].clone())
        .collect::<Vec<_>>();
    assert_eq!(ops, vec![json!([LR2_OPTION_BASE + 20]), json!([51]), json!([50])]);
}

#[test]
fn lr2_ghost_and_rival_refs_use_the_lr2_namespace() {
    let lr2 = |id: i32| json!(LR2_NUMBER_BASE + id);
    let play = load(0, &[150, 151, 155, 158].map(number).concat());
    assert_eq!(number_refs(&play), vec![lr2(150), lr2(151), lr2(155), lr2(158)]);
    let select = load(5, &[272, 273, 274, 275].map(number).concat());
    assert_eq!(number_refs(&select), vec![lr2(272), lr2(273), lr2(274), json!(275)]);
    let image = |ops: &str| {
        format!(
            "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
             #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,0,0,0,0,0,0,{ops}\n"
        )
    };
    let op = |document: &JsonValue| document["destination"][0]["op"].clone();
    assert_eq!(
        op(&load(5, &image("62,640,-657"))),
        json!([LR2_OPTION_BASE + 62, LR2_OPTION_BASE + 640, -(LR2_OPTION_BASE + 657)])
    );
    assert_eq!(op(&load(0, &image("352,0,0"))), json!([LR2_OPTION_BASE + 352]));
    // Result keeps the final-target comparison.
    assert_eq!(op(&load(7, &image("352,0,0"))), json!([352]));
}

#[test]
fn lr2_level_bar_options_are_draw_time_conditions() {
    let image = |ops: &str| {
        format!(
            "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
             #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,0,0,0,0,0,0,{ops}\n"
        )
    };
    let select = load(5, &format!("{}#IF,505\n{}#ENDIF\n", image("500,-523,75"), image("0,0,0")));
    let ops = select["destination"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["op"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ops,
        vec![
            json!([LR2_OPTION_BASE + 500, -(LR2_OPTION_BASE + 523), LR2_OPTION_BASE + 75]),
            json!([LR2_OPTION_BASE + 505]),
        ]
    );
}

#[test]
fn lr2_embedded_skin_settings_texts_use_the_lr2_namespace() {
    let font = "#FONT,20,1,0\n";
    let text = |id: i32| {
        format!(
            "#SRC_TEXT,0,0,{id},0\n\
             #DST_TEXT,0,0,0,0,10,10,0,255,255,255,255,0,0,0,0,0,0\n"
        )
    };
    let select =
        load(5, &format!("{font}{}", [50, 51, 100, 119, 10, 76, 80, 71].map(text).concat()));
    let refs =
        select["text"].as_array().unwrap().iter().map(|t| t["ref"].clone()).collect::<Vec<_>>();
    let lr2 = |id: i32| json!(bmz_skin_document::LR2_TEXT_BASE + id);
    assert_eq!(
        refs,
        vec![lr2(50), lr2(51), lr2(100), lr2(119), json!(10), lr2(76), lr2(80), json!(71)]
    );
}

#[test]
fn lr2_select_style_course_and_hover_conditions_survive_decode() {
    let image = "#SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
                 #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,0,0,0,0,0,0,10,-11,572\n";
    let select = load(5, &format!("{image}#IF,12\n{image}#ENDIF\n#IF,572\n{image}#ENDIF\n"));
    let ops = &select["destination"];
    assert_eq!(
        ops[0]["op"],
        json!([LR2_OPTION_BASE + 10, -(LR2_OPTION_BASE + 11), LR2_OPTION_BASE + 572])
    );
    assert!(ops[1]["op"].as_array().unwrap().contains(&json!(LR2_OPTION_BASE + 12)));
    assert!(ops[2]["op"].as_array().unwrap().contains(&json!(LR2_OPTION_BASE + 572)));

    for panel in [0, 1, 3, 7] {
        let hover = load(
            5,
            &format!(
                "#SRC_ONMOUSE,0,0,0,0,20,30,1,1,0,0,{panel},1,2,15,25\n\
             #DST_ONMOUSE,0,0,4,6,20,30,0,255,255,255,255,1,0,0,0,0,0,5,0,0\n"
            ),
        );
        assert_eq!(hover["destination"][0]["mouseRect"], json!({"x":1,"y":3,"w":15,"h":25}));
        let expected = if panel == 0 {
            json!([LR2_OPTION_BASE + 5])
        } else {
            json!([LR2_OPTION_BASE + 5, LR2_OPTION_BASE + 20 + panel])
        };
        assert_eq!(hover["destination"][0]["op"], expected);
    }
}
