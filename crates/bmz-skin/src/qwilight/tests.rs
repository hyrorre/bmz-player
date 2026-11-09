use super::*;

fn evaluator(
    yaml: &str,
    lua: &str,
    options: BTreeMap<String, String>,
) -> (tempfile::TempDir, Evaluator) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("skin.yaml");
    fs::write(&path, yaml).unwrap();
    fs::write(root.path().join("skin.lua"), lua).unwrap();
    let context = SkinPathContext::for_entry(&path).unwrap();
    let eval = Evaluator::new(Definition::read(&context).unwrap(), &context, &options).unwrap();
    (root, eval)
}

#[test]
fn qwilight_layout_remaps_scratches_and_expands_dp_with_configured_widths() {
    let yaml = "format: {}\nframe: {}\nlambda:\n  configure-comment-0: Width\n  configure-0: Narrow, Wide\n  drawingInputMode12: '1, 2, 3, 2, 3, 2'\n  drawingInputModeSystem: 1\npoint:\n  noteLength: Width(), 20, 10\n  judgmentMainPosition: 500\n  p2Position: 30\n";
    let (_root, mut eval) = evaluator(
        yaml,
        "function Width() return 30 + 10 * configures[1] end",
        BTreeMap::from([("Width".into(), "Wide".into())]),
    );
    let layout = layout::Layout::build(&mut eval, 10).unwrap();
    assert_eq!(
        layout.lanes.iter().map(|l| l.x).collect::<Vec<_>>(),
        vec![40.0, 60.0, 70.0, 90.0, 100.0, 0.0, 150.0, 170.0, 180.0, 200.0, 210.0, 230.0]
    );
    assert_eq!(layout.lanes[5].width, 40.0);
    assert_eq!(layout.lanes[11].width, 40.0);
    assert_eq!(
        layout.lanes.iter().map(|l| l.timer_index).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 0, 11, 12, 13, 14, 15, 10]
    );
}

#[test]
fn qwilight_lua_keyword_spacing_preserves_strings_and_comments() {
    let source =
        "if 3then return '3then', [=[4then]=] end -- 9then\n--[==[5then]==]\nlocal a3then = 0x3def";
    assert_eq!(
        syntax::normalize_lua(source),
        "if 3 then return '3then', [=[4then]=] end -- 9then\n--[==[5then]==]\nlocal a3then = 0x3def"
    );
}

#[test]
fn qwilight_numeric_layout_rejects_non_finite_values() {
    let (_root, eval) = evaluator(
        "format: {}\nframe: {}\nlambda: {}\npoint: {}",
        "function Bad() return 0/0 end",
        BTreeMap::new(),
    );
    assert!(eval.numeric("Bad()").is_err());
    assert!(eval.numeric("1e100").is_err());
}

#[test]
fn qwilight_theme_and_other_formats_are_not_play_candidates() {
    assert!(is_qwilight_play_skin_path(Path::new("CR_DX.yaml")));
    assert!(is_qwilight_play_skin_path(Path::new("Default.YML")));
    assert!(!is_qwilight_play_skin_path(Path::new("@Default.yaml")));
    assert!(!is_qwilight_play_skin_path(Path::new("play.luaskin")));
}

#[test]
fn qwilight_layout_vm_has_no_host_io_and_cannot_catch_budget_exhaustion() {
    let lua = crate::lua::new_layout_lua().unwrap();
    lua.load(
        "assert(io == nil and os == nil and package == nil and dofile == nil and loadfile == nil)",
    )
    .exec()
    .unwrap();
    assert!(lua.load("while true do pcall(function() while true do end end) end").exec().is_err());
}

#[test]
#[ignore = "requires installed Qwilight play skins via BMZ_QWILIGHT_SKIN_ROOT"]
fn installed_qwilight_play_skins() {
    let root = std::env::var_os("BMZ_QWILIGHT_SKIN_ROOT").expect("set BMZ_QWILIGHT_SKIN_ROOT");
    let root = std::path::PathBuf::from(root);
    let mut count = 0;
    for entry in fs::read_dir(&root).unwrap().flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        for file in fs::read_dir(entry.path()).unwrap().flatten() {
            let path = file.path();
            if !is_qwilight_play_skin_path(&path) {
                continue;
            }
            let context = SkinPathContext::new(&path, [root.clone()]).unwrap();
            for mode in [4, 5, 6, 7, 8, 9, 10, 14] {
                let mut state = LuaLoadRuntimeState::default();
                state.number_values.insert(SKIN_REF_BMZ_KEY_MODE, mode);
                let loaded = load_qwilight_play_skin(&context, &BTreeMap::new(), &state)
                    .unwrap_or_else(|e| panic!("{} {mode}K: {e:#}", path.display()));
                let expected = match mode {
                    5 => 6,
                    7 => 8,
                    10 => 12,
                    14 => 16,
                    _ => mode as usize,
                };
                assert_eq!(loaded.document.note.as_ref().unwrap().note.len(), expected);
                assert!(!loaded.document.source.is_empty());
                for source in &loaded.document.source {
                    context
                        .resolve_asset(&source.path)
                        .unwrap_or_else(|e| panic!("{}: {e:#}", source.path));
                }
                assert!(loaded.document.destination.len() > 10);
                eprintln!(
                    "{} {mode}K: {} sources, {} destinations",
                    path.display(),
                    loaded.document.source.len(),
                    loaded.document.destination.len()
                );
            }
            count += 1;
        }
    }
    assert!(count > 0, "no Qwilight play skins found");
}
