use super::*;

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
            let items = cache.render(&doc, 0, "n", 7, frame, time, &sources, signed);
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
