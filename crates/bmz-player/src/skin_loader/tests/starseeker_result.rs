use super::*;

#[test]
fn starseeker_result_rank_difference_uses_shared_lua_and_runtime_values_when_available() {
    let paths = test_app_paths();
    let skin_path = paths.resource_dir.join("skins/ADFX02/Starseeker/result/result.luaskin");
    if !skin_path.is_file() {
        eprintln!("skipping: Starseeker assets not present at {}", skin_path.display());
        return;
    }
    let files = BTreeMap::from([
        ("使用テーマ".to_string(), "Theme/starseeker".to_string()),
        ("フォント".to_string(), "_font/starseeker".to_string()),
        ("シャッター".to_string(), "Shutter/TYPE-M".to_string()),
    ]);
    let cases = [(2969, 135.0, 135.0), (3104, 0.0, 0.0), (2760, 344.0, 0.0), (2800, 304.0, -40.0)];
    let context = SkinPathContext::new(&skin_path, paths.skin_library_roots()).unwrap();
    for runtime_mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        for mode in ["NEXT RANK", "NEAREST RANK"] {
            for (score, next, nearest) in cases {
                // Exercise the app's document loader without repeatedly decoding images.
                // Entry-only loading rejects ../../rank_diff.lua, and the skin's pcall
                // then drops the score frame; use the normal app's library roots.
                let loaded = load_skin_document_with_path_context(
                    &skin_path,
                    SkinKind::Result,
                    &BTreeMap::from([("ランク差分表示".into(), mode.into())]),
                    &files,
                    &LuaLoadRuntimeState {
                        runtime_mode,
                        number_values: BTreeMap::from([(71, score), (74, 1552), (171, score)]),
                        ..Default::default()
                    },
                    None,
                    Some(&context),
                )
                .expect("decode Starseeker Result with the app's library roots");
                assert!(loaded.document.image.iter().any(|image| image.id == "SCORE_FRAME"));
                let value = loaded
                    .document
                    .value
                    .iter()
                    .find(|value| value.id == "RANK_Diff_Exscore")
                    .expect("Starseeker rank-difference number");
                let expected = if mode == "NEXT RANK" { next } else { nearest };
                assert_eq!(
                    value.y,
                    if expected == 0.0 { 165 } else { 292 },
                    "{runtime_mode:?} {mode}, EX SCORE {score}"
                );
                let callback_id = value
                    .value_expr
                    .strip_prefix("bmz:lua_value_callback:")
                    .expect("rank difference must retain a runtime callback")
                    .parse::<usize>()
                    .unwrap();
                let mut runtime = loaded.lua_runtime.expect("rank-difference Lua runtime");
                // Reuse the loaded VM across scores: a constant captured during load
                // would pass the sheet check but fail this sequence.
                for (current, next, nearest) in cases {
                    let state = SkinDrawState {
                        ex_score: current as u32,
                        total_notes: 1552,
                        ..Default::default()
                    };
                    let text_values = BTreeMap::new();
                    let provider = RenderLuaMainState {
                        state: &state,
                        enabled_options: &[],
                        text_values: &text_values,
                    };
                    runtime.begin_frame();
                    assert_eq!(
                        runtime.evaluate_number(callback_id, &provider),
                        Some(if mode == "NEXT RANK" { next } else { nearest }),
                        "{runtime_mode:?} {mode}, loaded {score}, current {current}"
                    );
                }
            }
        }
    }
}
