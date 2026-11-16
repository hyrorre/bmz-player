use super::*;
use serde_json::json;

#[test]
fn field_destinations_keep_conditions_timers_animation_and_clipping() {
    for render in [
        json!({"mode":"text","text":{"font":"vector"}}),
        json!({"mode":"digits","digits":{"src":"src","w":90,"h":10,"divx":10,"digit":3}}),
        json!({"mode":"image","images":["body","body","body","body","body","body","body","body","body"]}),
    ] {
        let field = if render["mode"] == "image" { "scorerank" } else { "exscore" };
        let mut entry = render;
        entry["timer"] = json!(1);
        entry["op"] = json!([1985]);
        entry["draw"] = json!("number(71) >= 100");
        entry["dst"] = json!([
            {"time":0,"x":10,"y":0,"w":10,"h":20,"clip_x":0,"clip_y":0,"clip_w":100,"clip_h":30},
            {"time":100,"x":30,"y":0,"w":10,"h":20}]);
        let mut fields = serde_json::Map::new();
        fields.insert(field.into(), entry);
        let doc = document(fields.into());
        let list = doc.songlist.as_ref().unwrap();
        let mut state = SkinDrawState {
            elapsed_ms: 50,
            start_input_ms: Some(50),
            select_screen: true,
            select_play_count: 1,
            select_ex_score: Some(150),
            select_total_notes: 100,
            ..Default::default()
        };
        SkinDocument::apply_select_songlist_render_row_state(&mut state, &song(), None);
        let images = doc.image_map();
        let images = images.into();
        let items =
            render_fields(&doc, list, &song(), (100, 200), false, &sources(), &images, &[], &state);
        let [SkinRenderItem::PushClip { .. }, inner @ .., SkinRenderItem::PopClip] =
            items.as_slice()
        else {
            panic!("{items:?}")
        };
        assert!(!inner.is_empty());
        let x = match &inner[0] {
            SkinRenderItem::Image { rect, .. } => rect.x,
            SkinRenderItem::Text { origin, .. } => origin.x,
            _ => panic!(),
        };
        assert!((x - 0.12).abs() < 0.00001, "{x}");
        state.start_input_ms = None;
        assert!(
            render_fields(&doc, list, &song(), (0, 0), false, &sources(), &images, &[], &state)
                .is_empty()
        );
        state.start_input_ms = Some(50);
        state.select_ex_score = Some(50);
        state.ex_score = 50;
        assert!(
            render_fields(&doc, list, &song(), (0, 0), false, &sources(), &images, &[], &state)
                .is_empty()
        );
    }
}

#[test]
fn explicit_order_and_empty_extensions_preserve_legacy_output() {
    let doc = document(
        json!({"exscore":text(),"scorerank":text(),"scoreOrder":["scorerank","scorerank","exscore"]}),
    );
    let snapshot = SelectSnapshot { rows: vec![song()], ..Default::default() };
    assert_eq!(texts(&doc.select_render_items(&sources(), &snapshot)), ["A", "150"]);
    let no_extensions = document(json!({}));
    let empty_extensions = document(json!({"exscore":[],"favorite":[],"scorerank":[]}));
    assert_eq!(
        no_extensions.select_render_items(&sources(), &snapshot),
        empty_extensions.select_render_items(&sources(), &snapshot)
    );
}

#[test]
fn numeric_formats_cover_signed_zero_fraction_precision_and_large_scores() {
    let plus = SongListFieldFormat { show_plus: true, ..Default::default() };
    assert_eq!(FieldValue::Number(0).formatted(&plus, &[]), "+0");
    assert_eq!(FieldValue::Number(-5).formatted(&plus, &[]), "-5");
    assert_eq!(FieldValue::Number(5).formatted(&plus, &[]), "+5");
    assert_eq!(FieldValue::Rate(1, 3).formatted(&Default::default(), &[]), "16.66%");
    assert_eq!(FieldValue::Rate(u32::MAX, u32::MAX).formatted(&Default::default(), &[]), "50.00%");
    assert_eq!(
        FieldValue::Rate(1, 3).formatted(
            &SongListFieldFormat {
                decimals: Some(0),
                percent: Some(false),
                prefix: "[".into(),
                suffix: "]".into(),
                ..Default::default()
            },
            &[]
        ),
        "[16]"
    );
}

#[test]
fn clear_only_records_hide_numeric_fields_but_keep_favorite_and_rival_rank() {
    let row = SelectRowSnapshot {
        numeric_score_available: false,
        ex_score: Some(0),
        bp: Some(0),
        cb: Some(0),
        max_combo: Some(0),
        ..song()
    };
    for field in [
        SongListField::Exscore,
        SongListField::Minbp,
        SongListField::Mincb,
        SongListField::Maxcombo,
        SongListField::Scorerank,
        SongListField::Scorerate,
        SongListField::Nextrank,
        SongListField::Nearestrank,
        SongListField::Nextrankdiff,
        SongListField::Nearestrankdiff,
        SongListField::Rivaldiff,
    ] {
        assert_eq!(field_value(field, &row, true), None);
    }
    assert_eq!(field_value(SongListField::Favorite, &row, true), Some(FieldValue::Favorite(3)));
    assert_eq!(field_value(SongListField::Rivalscorerank, &row, true), Some(FieldValue::Rank(5)));
}

fn song() -> SelectRowSnapshot {
    SelectRowSnapshot {
        total_notes: 100,
        ex_score: Some(150),
        bp: Some(3),
        cb: Some(2),
        max_combo: Some(80),
        play_count: 1,
        rival_ex_score: Some(140),
        favorite_chart: true,
        favorite_song: true,
        ..Default::default()
    }
}

fn document(fields: serde_json::Value) -> SkinDocument {
    let mut value = json!({
        "type":5,"w":1000,"h":1000,
        "font":[{"id":"vector","path":"test.otf"},{"id":"bitmap","path":"test.fnt"}],
        "image":[{"id":"body","src":"src","w":90,"h":10}],
        "imageset":[{"id":"bars","images":["body"]}],
        "songlist":{"id":"list","center":1,
            "liston":[
                {"id":"bars","dst":[{"x":0,"y":0,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":100,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":200,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":300,"w":100,"h":50}]}],
            "listoff":[
                {"id":"bars","dst":[{"x":0,"y":0,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":100,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":200,"w":100,"h":50}]},
                {"id":"bars","dst":[{"x":0,"y":300,"w":100,"h":50}]}]},
        "destination":[{"id":"list"}]
    });
    value["songlist"].as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn text() -> serde_json::Value {
    json!({"mode":"text","text":{"font":"vector","size":20},
        "dst":[{"x":5,"y":7,"w":200,"h":20}]})
}

fn sources() -> HashMap<String, SkinDocumentTexture> {
    HashMap::from([(
        "src".into(),
        SkinDocumentTexture {
            source_id: "src".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 90.0, height: 10.0 },
        },
    )])
}

fn texts(items: &[SkinRenderItem]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| {
            if let SkinRenderItem::Text { text, .. } = item { Some(text.as_str()) } else { None }
        })
        .collect()
}

#[test]
fn all_thirteen_values_reach_text_items_in_declaration_order() {
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
    let fields =
        names.into_iter().map(|name| (name.into(), text())).collect::<serde_json::Map<_, _>>();
    let doc = document(fields.into());
    let snapshot =
        SelectSnapshot { rival_selected: true, rows: vec![song()], ..Default::default() };
    let items = doc.select_render_items(&sources(), &snapshot);
    assert_eq!(
        texts(&items),
        ["A", "150", "75.00%", "3", "2", "80", "AA", "-6", "AA", "-6", "A", "10", "CHART+SONG"]
    );
}

#[test]
fn row_grade_values_match_existing_refs_at_every_boundary_including_tiny_charts() {
    for notes in 1..=100 {
        for score in 0..=notes * 2 {
            let row = SelectRowSnapshot {
                ex_score: Some(score),
                total_notes: notes,
                play_count: 1,
                ..song()
            };
            let state = SkinDrawState {
                select_screen: true,
                select_play_count: 1,
                select_ex_score: Some(score),
                select_total_notes: notes,
                ..Default::default()
            };
            for (field, reference) in [
                (SongListField::Scorerank, 1974),
                (SongListField::Nextrank, 1975),
                (SongListField::Nearestrank, 1976),
                (SongListField::Nextrankdiff, 1978),
                (SongListField::Nearestrankdiff, 1979),
            ] {
                let actual = match field_value(field, &row, false).unwrap() {
                    FieldValue::Rank(i) => i as i64,
                    FieldValue::Number(n) => n,
                    _ => unreachable!(),
                };
                assert_eq!(
                    Some(actual),
                    skin_state_number(reference, &state),
                    "{notes}/{score}/{reference}"
                );
            }
        }
    }
    assert_eq!(
        field_value(
            SongListField::Nextrank,
            &SelectRowSnapshot { ex_score: Some(200), ..song() },
            false
        ),
        Some(FieldValue::Rank(8))
    );
    let tie = SelectRowSnapshot { total_notes: 9, ex_score: Some(5), ..song() };
    assert_eq!(field_value(SongListField::Nearestrank, &tie, false), Some(FieldValue::Rank(1)));
    assert_eq!(
        field_value(SongListField::Nearestrankdiff, &tie, false),
        Some(FieldValue::Number(1))
    );
}

#[test]
fn missing_zero_rival_favorites_and_non_song_rows_stay_distinct() {
    let mut row = song();
    row.ex_score = None;
    row.bp = None;
    row.cb = None;
    row.max_combo = None;
    for field in [
        SongListField::Scorerank,
        SongListField::Exscore,
        SongListField::Scorerate,
        SongListField::Minbp,
        SongListField::Mincb,
        SongListField::Maxcombo,
        SongListField::Rivaldiff,
    ] {
        assert_eq!(field_value(field, &row, true), None);
    }
    for index in 0..4 {
        row.favorite_chart = index & 1 != 0;
        row.favorite_song = index & 2 != 0;
        assert_eq!(
            field_value(SongListField::Favorite, &row, false),
            Some(FieldValue::Favorite(index))
        );
    }
    row.ex_score = Some(0);
    assert_eq!(field_value(SongListField::Scorerank, &row, false), Some(FieldValue::Rank(0)));
    assert_eq!(field_value(SongListField::Exscore, &row, false), Some(FieldValue::Number(0)));
    assert_eq!(
        field_value(SongListField::Scorerate, &row, false)
            .unwrap()
            .formatted(&Default::default(), &[]),
        "0.00%"
    );
    assert_eq!(field_value(SongListField::Rivalscorerank, &row, false), None);
    row.rival_ex_score = None;
    assert_eq!(field_value(SongListField::Rivalscorerank, &row, true), None);
    assert_eq!(field_value(SongListField::Rivaldiff, &row, true), None);
    row.rival_ex_score = Some(0);
    assert_eq!(field_value(SongListField::Rivalscorerank, &row, true), Some(FieldValue::Rank(0)));
    assert_eq!(field_value(SongListField::Rivaldiff, &row, true), Some(FieldValue::Number(0)));
    row.total_notes = 0;
    assert_eq!(field_value(SongListField::Scorerate, &row, true), None);
    assert_eq!(field_value(SongListField::Scorerank, &row, true), None);
    for kind in [SelectRowKind::Folder, SelectRowKind::Config, SelectRowKind::Course] {
        row.kind = kind;
        assert_eq!(field_value(SongListField::Exscore, &row, true), None);
        assert_eq!(field_value(SongListField::Favorite, &row, true), None);
    }
}

#[test]
fn scroll_uses_each_rows_data_and_origin_and_updated_snapshot() {
    let doc = document(json!({"exscore":text()}));
    let mut snapshot = SelectSnapshot {
        selected_index: 1,
        rows: (0..3)
            .map(|i| SelectRowSnapshot { index: i, ex_score: Some(i * 100), ..song() })
            .collect(),
        bar_scroll_direction: 1,
        bar_scroll_progress: 0.5,
        ..Default::default()
    };
    let items = doc.select_render_items(&sources(), &snapshot);
    assert_eq!(texts(&items), ["0", "100", "200"]);
    let ys = items
        .iter()
        .filter_map(
            |i| if let SkinRenderItem::Text { origin, .. } = i { Some(origin.y) } else { None },
        )
        .collect::<Vec<_>>();
    for (actual, expected) in ys.iter().zip([0.923, 0.823, 0.723]) {
        assert!((actual - expected).abs() < 0.00001, "{ys:?}");
    }
    snapshot.bar_scroll_rows = Some(vec![SelectRowSnapshot { ex_score: Some(123), ..song() }]);
    assert_eq!(texts(&doc.select_render_items(&sources(), &snapshot)), ["123"]);
    snapshot.bar_scroll_rows = None;
    snapshot.rows[1].ex_score = Some(180);
    assert_eq!(texts(&doc.select_render_items(&sources(), &snapshot)), ["0", "180", "200"]);
}

#[test]
fn text_styles_bitmap_vector_labels_and_multiple_modes_share_values() {
    for path in ["test.ttf", "test.otf", "test.fnt", "test.lr2font"] {
        let mut entry = text();
        entry["text"] = json!({"font":"vector","size":18,"align":2,"overflow":2,
            "outlineColor":"000000","outlineWidth":1,"shadowOffsetX":2,"prefix":"[","suffix":"]"});
        entry["dst"][0]["a"] = json!(128);
        let mut doc = document(json!({"scorerank":[entry,text()],"favorite":{
            "mode":"text","labels":["none","chart","song","both"],"text":{"font":"bitmap"},
            "dst":[{"w":50,"h":20}]}}));
        doc.font[0].path = path.into();
        let items = doc.select_render_items(
            &sources(),
            &SelectSnapshot { rows: vec![song()], ..Default::default() },
        );
        assert_eq!(texts(&items), ["[A]", "A", "both"]);
        let SkinRenderItem::Text { style, .. } = &items[1] else { panic!() };
        assert_eq!(style.font_id.as_deref(), Some("vector"));
        assert_eq!(
            style.bitmap_size.is_some(),
            path.ends_with(".fnt") || path.ends_with(".lr2font")
        );
        assert_eq!(style.align, TextAlign::Right);
        assert!(style.outline.is_some());
        assert!(style.shadow.is_some());
        assert!((style.color.a - 128.0 / 255.0).abs() < 0.00001);
    }
}

#[test]
fn images_keep_max_and_favorite_indices_and_conditions() {
    let doc = document(
        json!({"scorerank":{"mode":"image","images":["body","body","body","body","body","body","body","missing","body"],
        "filter":1,"dst":[{"x":9,"y":4,"w":10,"h":10}]},
        "favorite":{"mode":"image","images":["missing","missing","missing","body"],"dst":[{"w":8,"h":8}]},
        "exscore":{"mode":"text","text":{},"draw":"false","dst":[{"w":10,"h":10}]}}),
    );
    let items = doc.select_render_items(
        &sources(),
        &SelectSnapshot {
            rows: vec![SelectRowSnapshot { ex_score: Some(200), ..song() }],
            ..Default::default()
        },
    );
    assert_eq!(items.len(), 3);
    assert!(texts(&items).is_empty());
    let SkinRenderItem::Image { linear_filter, .. } = &items[1] else { panic!() };
    assert!(*linear_filter);
}

#[test]
fn digit_formats_never_lose_required_glyphs_signs_or_overflow() {
    let mut digits = SkinValueDef { divx: 14, divy: 1, digit: 8, ..Default::default() };
    let glyphs = SongListDigitGlyphs {
        plus: Some(10),
        minus: Some(11),
        decimal: Some(12),
        percent: Some(13),
    };
    assert_eq!(digit_cells(&digits, &glyphs, "+12.34%"), Some(vec![10, 1, 2, 12, 3, 4, 13]));
    assert_eq!(digit_cells(&digits, &glyphs, "-0"), Some(vec![11, 0]));
    assert_eq!(digit_cells(&digits, &Default::default(), "-0"), None);
    assert_eq!(digit_cells(&digits, &Default::default(), "12.34%"), None);
    digits.digit = 2;
    assert_eq!(digit_cells(&digits, &glyphs, "-12"), None);
    digits.digit = 5;
    digits.zeropadding = 1;
    assert_eq!(digit_cells(&digits, &glyphs, "-12"), Some(vec![11, 0, 0, 1, 2]));
    digits.divx = 24;
    digits.zeropadding = 0;
    assert_eq!(digit_cells(&digits, &Default::default(), "-12"), Some(vec![23, 13, 14]));
    digits.lr2_number = true;
    digits.divx = 48;
    assert_eq!(digit_cells(&digits, &Default::default(), "+12"), Some(vec![11, 1, 2]));
    digits.divx = 10;
    assert_eq!(digit_cells(&digits, &Default::default(), "0"), Some(vec![0]));
    assert_eq!(digit_cells(&digits, &Default::default(), "-1"), None);
    digits.divx = 11;
    assert_eq!(digit_cells(&digits, &Default::default(), "10"), Some(vec![1, 0]));
}

#[test]
fn digits_generate_tinted_positioned_cells_for_fraction_percent_and_sign() {
    let doc = document(json!({"scorerate":{"mode":"digits",
        "format":{"showPlus":true},
        "digits":{"src":"src","w":140,"h":10,"divx":14,"digit":8,"align":1,"space":2},
        "glyphs":{"plus":10,"minus":11,"decimal":12,"percent":13},
        "dst":[{"x":9,"y":4,"w":10,"h":10,"r":128,"a":128}]}}));
    let items = doc.select_render_items(
        &sources(),
        &SelectSnapshot { rows: vec![song()], ..Default::default() },
    );
    // body + '+75.00%'
    assert_eq!(items.len(), 8);
    for (index, item) in items[1..].iter().enumerate() {
        let SkinRenderItem::Image { rect, uv, tint, .. } = item else { panic!() };
        assert!((rect.x - (9 + 12 * index) as f32 / 1000.0).abs() < 0.00001);
        assert!((tint.a - 128.0 / 255.0).abs() < 0.00001);
        let expected = [10, 7, 5, 12, 0, 0, 13][index] as f32 * 10.0 / 90.0;
        assert!((uv.x - expected).abs() < 0.00001);
    }
}
