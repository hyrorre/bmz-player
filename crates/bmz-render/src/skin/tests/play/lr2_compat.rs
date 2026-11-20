use super::*;

#[test]
fn lr2_judge_and_combo_skip_outside_cells_on_each_blink() {
    let sources = mock_source("src", 500.0, 140.0);
    for lr2 in [true, false] {
        let destination = |id: &str| {
            serde_json::json!({ "id":id, "lr2Timing":lr2, "timer":46, "loop":-1,
                "dst":[{"time":0,"w":77,"h":35},{"time":498}] })
        };
        let doc: SkinDocument = serde_json::from_value(serde_json::json!({
            "w":500,"h":140,
            "image":[
                {"id":"great","src":"src","y":105,"w":77,"h":70,
                    "divy":2,"cycle":75,"timer":46,"lr2Clip":lr2},
                {"id":"good","src":"src","x":247,"w":77,"h":70,
                    "divy":2,"cycle":75,"timer":46,"lr2Clip":lr2},
            ],
            "value":[{"id":"combo","src":"src","x":77,"y":105,"w":170,"h":70,
                "divx":10,"divy":2,"digit":1,"cycle":75,"timer":46,"lr2Number":lr2}],
            "judge":[{"id":"judge","images":[{},destination("great"),destination("good")],
                "numbers":[{},destination("combo"),destination("combo")]}],
        }))
        .unwrap();

        for (judge, label_y) in [("GREAT", 0.75), ("GOOD", 0.0)] {
            let render = |time| doc.judge_render_items(judge, 1, time, &sources).unwrap();
            let on = render(0);
            assert!(matches!(on.as_slice(), [
                SkinRenderItem::Image { uv: label, .. }, SkinRenderItem::Image { uv: number, .. },
            ] if label.y == label_y && label.height == 0.25 && number.y == 0.75 && number.height == 0.25));
            assert_eq!(render(37), on);
            let off = render(38);
            if lr2 && judge == "GREAT" {
                assert!(off.is_empty());
            } else if lr2 {
                // The GOOD label's second cell is a transparent region inside the image.
                assert!(matches!(off.as_slice(), [SkinRenderItem::Image { uv, .. }]
                    if uv.y == 0.25 && uv.height == 0.25));
            } else {
                assert!(matches!(off.as_slice(), [
                    SkinRenderItem::Image { uv: label, .. }, SkinRenderItem::Image { uv: number, .. },
                ] if label.y == label_y + 0.25 && label.height == 0.25 && number.y == 1.0 && number.height == 0.25));
            }
            assert_eq!(render(74), off);
            assert_eq!(render(75), on);
        }
    }
}

#[test]
fn lr2_signed_digits_use_twelve_cell_halves_on_every_grid() {
    for (divx, divy) in [(12, 2), (6, 4), (4, 6), (24, 1)] {
        let doc: SkinDocument = serde_json::from_value(serde_json::json!({
            "w":100,"h":100,"value":[{"id":"n","src":"src","w":-1,"h":-1,
                "divx":divx,"divy":divy,"digit":3,"lr2Number":true}]
        }))
        .unwrap();
        let sources = mock_source("src", (divx * 10) as f32, (divy * 10) as f32);
        let frame = ResolvedSkinFrame { w: 5, h: 5, ..Default::default() };
        for (number, cells) in [(7, [11, 10, 7]), (-7, [23, 22, 19])] {
            let items = doc.value_number_render_items(
                "n",
                number,
                ResolvedSkinFrame::default(),
                frame,
                0,
                &sources,
                false,
                None,
                SignedNumberRender::Signed(SignedNumberRowOrder::PositiveFirst),
            );
            assert_eq!(items.len(), 3);
            for (item, cell) in items.iter().zip(cells) {
                let SkinRenderItem::Image { uv, .. } = item else { panic!("digit image") };
                assert!((uv.x - (cell % divx) as f32 / divx as f32).abs() < 0.0001);
                assert!((uv.y - (cell / divx) as f32 / divy as f32).abs() < 0.0001);
            }
        }
    }
}

#[test]
fn lr2_number_animation_uses_consecutive_frames_and_cache_tracks_cycle() {
    for (divx, divy, stride) in [(20, 1, 10), (22, 1, 11), (11, 2, 11), (6, 8, 24)] {
        let doc: SkinDocument = serde_json::from_value(serde_json::json!({
            "w":100,"h":100,"value":[{"id":"n","src":"src","w":-1,"h":-1,
                "divx":divx,"divy":divy,"digit":if stride==24 {2}else{1},
                "cycle":100,"lr2Number":true}]
        }))
        .unwrap();
        let sources = mock_source("src", (divx * 10) as f32, (divy * 10) as f32);
        let frame = ResolvedSkinFrame { w: 5, h: 5, ..Default::default() };
        let signed = signed_number_render_for_value(&doc.value[0], &SkinDrawState::default());
        let mut cache = NumberRenderCache::default();
        for (time, offset) in [(0, 0), (50, stride), (50, stride), (100, 0)] {
            let items = cache.render(&doc, 0, "n", 7, frame, time, &sources, false, signed);
            let SkinRenderItem::Image { uv, .. } = items.last().unwrap() else {
                panic!("digit image")
            };
            let cell = 7 + offset;
            assert!((uv.x - (cell % divx) as f32 / divx as f32).abs() < 0.0001);
            assert!((uv.y - (cell / divx) as f32 / divy as f32).abs() < 0.0001);
        }
    }
}

#[test]
fn lr2_number_format_hides_unsigned_negatives_and_preserves_alignment_and_overflow() {
    let mut value =
        SkinValueDef { lr2_number: true, divx: 11, divy: 1, digit: 3, ..Default::default() };
    assert_eq!(lr2_number_digits(&value, -1, 0), None);
    assert_eq!(lr2_number_digits(&value, 7, 0), Some(vec![10, 10, 7]));
    assert_eq!(lr2_number_digits(&value, 7, 1), Some(vec![7, 10, 10]));
    assert_eq!(lr2_number_digits(&value, 1234, 0), Some(vec![2, 3, 4]));
    assert_eq!(lr2_number_digits(&value, 1234, 1), Some(vec![1, 2, 3]));
    value.divx = 10;
    assert_eq!(lr2_number_digits(&value, 7, 0), Some(vec![u8::MAX, u8::MAX, 7]));
    value.divx = 12;
    value.divy = 2;
    assert_eq!(lr2_number_digits(&value, -1234, 0), Some(vec![23, 15, 16]));
}

#[test]
fn lr2_source_fallback_hides_only_when_all_replacement_conditions_match() {
    let doc: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,
        "image":[{"id":"default","src":"src","w":10,"h":10},
                 {"id":"replacement","src":"src","x":10,"w":10,"h":10}],
        "destination":[
            {"id":"default","lr2Not":[{"op":[33,154]}],"dst":[{"w":10,"h":10}]},
            {"id":"replacement","op":[33,154],"dst":[{"w":10,"h":10}]}
        ]
    }))
    .unwrap();
    let sources = mock_source("src", 20.0, 10.0);
    for (autoplay, difficulty, expected_x) in
        [(false, 4, 0.0), (true, 3, 0.0), (true, 4, 0.5), (false, 4, 0.0)]
    {
        let state = SkinDrawState { autoplay, difficulty, ..Default::default() };
        let items = doc.static_image_render_items(&sources, &state);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0],SkinRenderItem::Image {uv,..} if uv.x==expected_x));
    }
}
