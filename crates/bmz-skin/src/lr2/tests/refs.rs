use super::*;
use bmz_skin_document::{LR2_NUMBER_BASE, LR2_OPTION_BASE};

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
