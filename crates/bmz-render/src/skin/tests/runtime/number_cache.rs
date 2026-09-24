use super::*;

#[test]
fn number_cache_matches_uncached_digits_for_animation_style_sources_and_sentinels() {
    for cycle in [0, 100] {
        let document: SkinDocument = serde_json::from_value(serde_json::json!({
            "w": 1920, "h": 1080,
            "value": [{"id":"n", "src":"digits", "w":-1, "h":-1,
                "divx":12, "divy":4, "digit":6, "zeropadding":1, "cycle":cycle,
                "space":2, "align":2}]
        }))
        .unwrap();
        let mut cache = NumberRenderCache::default();
        let source = SkinDocumentTexture {
            source_id: "digits".into(),
            texture: SkinTextureId(7),
            source_size: SkinImageSize { width: 120.0, height: 40.0 },
        };
        let mut sources = HashMap::from([("digits".into(), source)]);
        let original_frame =
            ResolvedSkinFrame { x: 100, y: 200, w: 20, h: 30, ..Default::default() };
        let mut frame = original_frame;
        for step in 0..8 {
            match step {
                2 => frame.x += 5,
                3 => {
                    frame.r = 96;
                    frame.a = 128;
                }
                4 => {
                    frame.w = 24;
                    frame.h = 32;
                    frame.y = 150;
                }
                5 => sources.get_mut("digits").unwrap().texture = SkinTextureId(8),
                6 => {
                    sources.get_mut("digits").unwrap().source_size =
                        SkinImageSize { width: 240.0, height: 80.0 }
                }
                7 => frame = original_frame,
                _ => {}
            }
            for elapsed in [0, 0, 25, 50, 99, 100, -1] {
                for number in [12, 12, 0, -123, i64::from(i32::MIN), i64::from(i32::MAX), 12] {
                    for signed in [
                        SignedNumberRender::Unsigned,
                        SignedNumberRender::Signed(SignedNumberRowOrder::PositiveFirst),
                    ] {
                        let expected = document.value_number_render_items(
                            "n",
                            number,
                            ResolvedSkinFrame::default(),
                            frame,
                            elapsed,
                            &sources,
                            false,
                            None,
                            signed,
                        );
                        // Repeated calls exercise both rebuilding and reuse.
                        for _ in 0..2 {
                            let actual = cache.render(
                                &document, 5, "n", number, frame, elapsed, &sources, signed,
                            );
                            assert_eq!(
                                actual, expected,
                                "cycle={cycle} step={step} elapsed={elapsed} number={number} signed={signed:?}"
                            );
                        }
                    }
                }
            }
        }
        let source = sources.remove("digits").unwrap();
        assert!(
            cache
                .render(&document, 5, "n", 12, frame, 0, &sources, SignedNumberRender::Unsigned)
                .is_empty()
        );
        sources.insert("digits".into(), source);
        assert_eq!(
            cache.render(&document, 5, "n", 12, frame, 0, &sources, SignedNumberRender::Unsigned),
            document.value_number_render_items(
                "n",
                12,
                ResolvedSkinFrame::default(),
                frame,
                0,
                &sources,
                false,
                None,
                SignedNumberRender::Unsigned
            )
        );
    }
}
