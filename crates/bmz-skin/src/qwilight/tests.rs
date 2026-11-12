use super::*;

fn evaluator(
    yaml: &str,
    lua: &str,
    options: BTreeMap<String, String>,
) -> (tempfile::TempDir, Evaluator) {
    evaluator_with_offsets(yaml, lua, options, BTreeMap::new())
}

fn evaluator_with_offsets(
    yaml: &str,
    lua: &str,
    options: BTreeMap<String, String>,
    offsets: BTreeMap<String, LuaSkinOffsetValue>,
) -> (tempfile::TempDir, Evaluator) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("skin.yaml");
    fs::write(&path, yaml).unwrap();
    fs::write(root.path().join("skin.lua"), lua).unwrap();
    let context = SkinPathContext::for_entry(&path).unwrap();
    let eval =
        Evaluator::new(Definition::read(&context).unwrap(), &context, &options, &offsets).unwrap();
    (root, eval)
}

fn global(eval: &Evaluator, name: &str) -> f64 {
    eval.lua.globals().get(name).unwrap()
}

#[test]
fn qwilight_perfect_inherits_full_combo_definitions_but_keeps_explicit_overrides() {
    let yaml = "format: {}\nframe: {band!-frame: 300, band!-framerate: 60, yell!-framerate: 30}\nlambda: {alt-band!: 1}\npoint: {band!System: 2, band!Position0: X(), band!Position1: 120, band!Length: 300, band!Height: 240, yell!Height: 200}";
    let (_root, eval) = evaluator(yaml, "function X() return 42 end", BTreeMap::new());
    for (section, key, expected) in [
        ("frame", "yell!-frame", 300.0),
        ("frame", "yell!-framerate", 30.0),
        ("point", "yell!System", 2.0),
        ("point", "yell!Position0", 42.0),
        ("point", "yell!Position1", 120.0),
        ("point", "yell!Length", 300.0),
        ("point", "yell!Height", 200.0),
        ("lambda", "alt-yell!", 1.0),
    ] {
        assert_eq!(eval.number(section, key, 0.0).unwrap(), expected, "{key}");
    }
}

#[test]
fn qwilight_adjustments_follow_set_flags_and_reach_lua_in_qwilight_direction() {
    let yaml = "format: {}\nframe: {}\nlambda:\n  setMainPosition: true\n  setNoteLength: false\n  setJudgmentMainPosition: true\n  setBandPosition: True\npoint: {}\n";
    let offset = |x, y, w| LuaSkinOffsetValue { x, y, w, ..Default::default() };
    let offsets = BTreeMap::from([
        ("Qwilight lane position".to_string(), offset(5000, 0, 0)),
        ("Qwilight note width".to_string(), offset(0, 0, 12)),
        ("Qwilight judgment line position".to_string(), offset(0, -40, 0)),
        ("Qwilight combo position".to_string(), offset(0, 25, 0)),
    ]);
    let (_root, eval) = evaluator_with_offsets(yaml, "", BTreeMap::new(), offsets);
    // Undeclared adjustments read 0 even when a value is saved under the same name.
    assert_eq!(global(&eval, "mainPosition"), 1000.0);
    assert_eq!(global(&eval, "noteLength"), 0.0);
    // The removed skin line adjustment is ignored; BMZ LIFT moves it at draw time.
    assert_eq!(global(&eval, "judgmentMainPosition"), 600.0);
    assert_eq!(global(&eval, "judgmentMainPositionV2"), 1.0);
    assert_eq!(global(&eval, "judgmentMainPositionV3"), 0.0);
    assert_eq!(global(&eval, "bandPosition"), -25.0);
    assert_eq!(global(&eval, "hitNotePaintArea"), 0.0);

    let header: SkinDocument =
        serde_json::from_value(eval.definition.header(&[true; CONFIGURE_COUNT], &[])).unwrap();
    let declared: Vec<_> =
        header.offset.iter().map(|o| (o.name.as_str(), o.id, o.x, o.y, o.w)).collect();
    assert_eq!(
        declared,
        [
            ("Qwilight lane position", 1001, true, false, false),
            ("Qwilight combo position", 1006, false, true, false),
        ]
    );
}

#[test]
fn qwilight_unavailable_options_are_hidden_and_read_first_choice_only_for_play_values() {
    let yaml = "format: {}\nframe: {}\nlambda:\n  configure-comment-0: Custom\n  configure-0: Hide, Show\n  configure-comment-1: Color\n  configure-1: Red, Blue\n  configure-available-1: Toggle()\n  configure-comment-2: Broken\n  configure-2: A, B\n  configure-available-2: Missing()\npoint:\n  x: Pick()\n";
    let lua = "function Toggle() return ({'false', 'true'})[configures[1] + 1] end\nfunction Pick() return 10 + configures[2] end\nfunction _GetPick() return tostring(configures[2]) end";
    for (custom, visible, play_value) in [("Hide", false, 10.0), ("Show", true, 11.0)] {
        let options = BTreeMap::from([
            ("Custom".to_string(), custom.to_string()),
            ("Color".to_string(), "Blue".to_string()),
        ]);
        let (_root, eval) = evaluator(yaml, lua, options);
        let header: SkinDocument =
            serde_json::from_value(eval.definition.header(&eval.available, &[])).unwrap();
        let names: Vec<_> = header.property.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.contains(&"Color"), visible, "{custom}");
        // A failing expression keeps the option, so its saved value can be changed back.
        assert!(names.contains(&"Broken"));
        assert_eq!(names.last(), Some(&QWILIGHT_LANE_COVER_PROPERTY));
        // Point values follow Qwilight's per-play state; materials keep the saved choice.
        assert_eq!(eval.number("point", "x", 0.0).unwrap(), play_value);
        assert_eq!(eval.prefix("_GetPick", &[], "").unwrap(), "1");
        assert_eq!(eval.number("point", "x", 0.0).unwrap(), play_value);
    }
}

#[test]
fn qwilight_localized_comments_and_choices_label_language_independent_names() {
    let yaml = "format: {}\nframe: {}\nlambda:\n  configure-comment-0: Note shape\n  configure-comment-0-1042: 노트 모양\n  configure-0: Bar note, Circle note\n  configure-0-1042: 막대 노트, 서클 노트\n  configure-0-1041: 棒\n  configure-comment-1: Gear\n  configure-comment-1-1041: ギア\n  configure-1: Left, Right\npoint: {}\n";
    let (_root, eval) = evaluator(yaml, "", BTreeMap::new());
    let header: SkinDocument =
        serde_json::from_value(eval.definition.header(&[true; CONFIGURE_COUNT], &[])).unwrap();
    assert_eq!(header.property[0].name, "Note shape");
    assert_eq!(header.property[0].item[1].name, "Circle note");
    let labels = &header.property_labels["Note shape"];
    assert_eq!(labels.name.get("ko").map(String::as_str), Some("노트 모양"));
    assert_eq!(labels.name.get("en").map(String::as_str), Some("Note shape"));
    assert!(!labels.name.contains_key("ja"));
    let circle = &labels.items["Circle note"];
    assert_eq!(circle.get("ko").map(String::as_str), Some("서클 노트"));
    // A localized list of another length cannot be matched by index.
    assert!(!circle.contains_key("ja"));
    // The saved key keeps the Japanese comment used before labels existed.
    assert_eq!(header.property[1].name, "ギア");
    assert_eq!(header.property_labels["ギア"].name["en"], "Gear");
}

#[test]
fn qwilight_image_size_reads_png_jpeg_bmp_gif_and_webp_headers() {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(1280_u32.to_be_bytes());
    png.extend(2560_u32.to_be_bytes());
    assert_eq!(customize::image_size(&png), Some((1280, 2560)));
    // APP0 segment, then a baseline SOF0 with height 720 and width 300.
    let jpeg =
        [0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0x02, 0xd0, 0x01, 0x2c, 3];
    assert_eq!(customize::image_size(&jpeg), Some((300, 720)));
    let mut bmp = b"BM".to_vec();
    bmp.resize(18, 0);
    bmp.extend(64_i32.to_le_bytes());
    bmp.extend((-32_i32).to_le_bytes());
    assert_eq!(customize::image_size(&bmp), Some((64, 32)));
    assert_eq!(customize::image_size(b"GIF89a\x10\0\x08\0"), Some((16, 8)));
    let mut webp = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
    webp.extend([99, 0, 0, 49, 0, 0]);
    assert_eq!(customize::image_size(&webp), Some((100, 50)));
    assert_eq!(customize::image_size(b"not an image"), None);
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
fn qwilight_skin_functions_can_override_layout_builtins() {
    let (_root, mut eval) = evaluator(
        "format: {}\nframe: {}\nlambda: {}\npoint:\n  noteLength: 20\n",
        "function P1Length() return P1BuiltLength(1.0) + 4 end\nfunction Wall() return 7 end\nfunction Contents() return 9 end",
        BTreeMap::new(),
    );
    layout::Layout::build(&mut eval, 6).unwrap();
    assert_eq!(eval.numeric("P1BuiltLength(1.0)").unwrap(), 120.0);
    assert_eq!(eval.numeric("P1Length()").unwrap(), 124.0);
    assert_eq!(eval.numeric("P2BuiltLength(0.5)").unwrap(), 60.0);
    assert_eq!(eval.numeric("Wall()").unwrap(), 7.0);
    assert_eq!(eval.numeric("Contents()").unwrap(), 9.0);
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
            let header = load_qwilight_play_header(&context).unwrap();
            let evaluated = load_qwilight_play_header_with_options(&context, &BTreeMap::new())
                .unwrap_or_else(|e| panic!("{} options: {e:#}", path.display()));
            let flags: Vec<_> = header.offset.iter().map(|offset| offset.name.as_str()).collect();
            eprintln!(
                "{}: {} of {} options visible, adjustments {flags:?}",
                path.display(),
                evaluated.property.len() - 1,
                header.property.len() - 1,
            );
            // Every declared adjustment at a non-default value must still load.
            let adjusted: BTreeMap<_, _> = header
                .offset
                .iter()
                .map(|offset| {
                    let value = LuaSkinOffsetValue { x: 30, y: 30, w: 6, h: 4, r: 0, a: 0 };
                    (offset.name.clone(), value)
                })
                .collect();
            for mode in [4, 5, 6, 7, 8, 9, 10, 14] {
                let mut state = LuaLoadRuntimeState::default();
                state.number_values.insert(SKIN_REF_BMZ_KEY_MODE, mode);
                let loaded = load_qwilight_play_skin(&context, &BTreeMap::new(), &state)
                    .unwrap_or_else(|e| panic!("{} {mode}K: {e:#}", path.display()));
                state.offset_values = adjusted.clone();
                let moved = load_qwilight_play_skin(&context, &BTreeMap::new(), &state)
                    .unwrap_or_else(|e| panic!("{} {mode}K adjusted: {e:#}", path.display()));
                if header.offset.iter().any(|o| matches!(o.id, 1001 | 1002)) {
                    let area =
                        |document: &SkinDocument| match &document.note.as_ref().unwrap().dst[0] {
                            bmz_skin_document::SkinDstEntry::Frame(frame) => {
                                (frame.x, frame.y, frame.w, frame.h)
                            }
                            _ => (None, None, None, None),
                        };
                    assert_ne!(
                        area(&loaded.document),
                        area(&moved.document),
                        "{} {mode}K adjustments must move the lanes",
                        path.display()
                    );
                }
                let covers = loaded.document.slider.iter().filter(|s| s.slider_type == 4).count();
                assert!(covers <= 1, "{} {mode}K lane covers", path.display());
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
                    "{} {mode}K: {} sources, {} destinations, {covers} lane cover",
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
