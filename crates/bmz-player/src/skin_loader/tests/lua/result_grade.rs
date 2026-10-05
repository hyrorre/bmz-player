use super::*;
use bmz_render::skin::SkinTextureId;

#[test]
fn luxe_flat_result_grade_draws_zero_and_boundary_differences_in_auto_and_compat() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Luxez-Flat/result");
    for file in ["result.luaskin", "courseresult.luaskin"] {
        let path = root.join(file);
        if !path.is_file() {
            eprintln!("skipping unavailable skin: {}", path.display());
            continue;
        }
        for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
            let loaded = load_skin_document_uncached(
                &path,
                SkinKind::Result,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &LuaLoadRuntimeState {
                    runtime_mode: mode,
                    number_values: BTreeMap::from([(71, 3610), (74, 1805)]),
                    option_values: BTreeMap::from([(50, false), (51, true), (160, true)]),
                    ..Default::default()
                },
            )
            .unwrap();
            let mut document = loaded.document;
            // Keep the original grade callbacks, atlas definitions, positions and
            // destination order. Substitute texture handles only; no GPU needed.
            document.destination = document
                .all_destinations(&document.enabled_options())
                .into_iter()
                .filter(|dst| dst.id.starts_with("rank_diff_"))
                .cloned()
                .map(DestinationListEntry::Single)
                .collect();
            assert!(!document.destination.is_empty(), "{file}");
            let grade_value =
                document.value.iter().find(|value| value.id == "rank_diff_count").unwrap();
            let digit_width = (grade_value.w / grade_value.divx) as f32;
            let digit_x = grade_value.x as f32;
            let mut sources = vec![SkinDocumentTexture {
                source_id: "figure".into(),
                texture: SkinTextureId(1),
                source_size: SkinImageSize { width: 170.0, height: 320.0 },
            }];
            let mut labels = BTreeMap::new();
            for (index, image) in document.image.iter_mut().enumerate() {
                if image.id.starts_with("rank_diff_") {
                    image.src = image.id.clone();
                    let texture = SkinTextureId(index as u32 + 100);
                    labels.insert(texture.0, image.id.clone());
                    sources.push(SkinDocumentTexture {
                        source_id: image.src.clone(),
                        texture,
                        source_size: SkinImageSize { width: 50.0, height: 255.0 },
                    });
                }
            }
            let runtime = Arc::new(LuaSkinDrawRuntimeAdapter::new(loaded.lua_runtime.unwrap()));
            let mut context = SkinContext::from_manifest_and_document(
                bmz_render::skin::default_skin_manifest(),
                document,
                sources,
            );
            context.set_lua_draw_runtime(Some(runtime.clone()));
            for (score, label, difference) in [
                (3610, "max_plus", "0"),
                (3609, "max_minus", "1"),
                (3410, "max_minus", "200"),
                (3409, "aaa_plus", "200"),
                (3300, "aaa_plus", "91"),
                (3209, "aaa_plus", "0"),
                (3208, "aaa_minus", "1"),
                (0, "f_plus", "0"),
                (3610, "max_plus", "0"),
            ] {
                let state = SkinDrawState {
                    elapsed_ms: 5000,
                    ex_score: score,
                    total_notes: 1805,
                    result_failed: Some(false),
                    ..Default::default()
                };
                // Repeated frames exercise the production Result number cache
                // and allow earlier labels to observe the preceding preparation.
                for frame in 0..3 {
                    context.begin_frame();
                    let items = context.static_document_items_for_result_state_and_text(
                        &Arc::default(),
                        &state,
                        &SkinTextState::default(),
                    );
                    let mut visible_labels = Vec::new();
                    let mut digits = String::new();
                    for item in items {
                        if let SkinRenderItem::Image { texture, uv, .. } = item {
                            if texture == SkinTextureId(1) {
                                let digit = ((uv.x * 170.0 - digit_x) / digit_width).round() as u8;
                                assert!(digit <= 10, "{file} {mode:?}: unexpected digit {digit}");
                                // The 11-cell atlas includes the leading blank.
                                if digit != 10 {
                                    digits.push(char::from(b'0' + digit));
                                }
                            } else if let Some(label) = labels.get(&texture.0) {
                                visible_labels.push(label.clone());
                            }
                        }
                    }
                    assert_eq!(digits, difference, "{file} {mode:?} score={score} frame={frame}");
                    if frame > 0 {
                        assert_eq!(
                            visible_labels,
                            [format!("rank_diff_{label}")],
                            "{file} {mode:?} score={score} frame={frame}"
                        );
                    }
                }
            }
            assert_eq!(runtime.runtime.lock().unwrap().as_ref().unwrap().failure_log_count(), 0);
        }
    }
}
