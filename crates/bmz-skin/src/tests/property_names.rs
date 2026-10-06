use super::*;

#[test]
fn named_main_state_properties_preserve_dependencies_and_runtime_closure_state() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let root = unique_test_dir("named-properties");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("skin.lua");
        fs::write(
            &path,
            r#"
            local s = require('main_state')
            local number = s.number
            local loaded_number = number('score')
            local loaded_text = s.text('player')
            local loaded_option = s.option('!bgaoff')
            local count = 0
            return {type=5, value={{id='counter', value=function()
                count = count + 1
                assert(number('71') == number('score'))
                assert(number('unknown') == 0 and number('return 123') == 0)
                assert(s.float_number('music_progress') == s.float_number(6))
                assert(s.float_number('score_rate') == s.float_number(1102))
                assert(s.float_number('unknown') == 0)
                assert(s.text('10') == s.text('title') and s.text('unknown') == '')
                assert(not s.option('!unknown') and not s.option('!!unknown'))
                return number('score') + count
            end}}, text={{id='text', value=function()
                return s.text('title') .. tostring(count)
            end}}, destination={{id='counter', draw=function()
                return s.option('bgaon') and not s.option('!bgaon') and s.option('!!bgaon')
                    and s.option('-41') and s.option('41')
            end}}}
        "#,
        )
        .unwrap();
        let mut loaded = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                runtime_mode: mode,
                number_values: BTreeMap::from([(71, 77)]),
                text_values: BTreeMap::from([(2, "load player".into())]),
                option_values: BTreeMap::from([(40, false)]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.dependencies.number_values.get(&71), Some(&77));
        assert_eq!(
            loaded.dependencies.text_values.get(&2).map(String::as_str),
            Some("load player")
        );
        assert_eq!(loaded.dependencies.option_values.get(&40), Some(&false));
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let callback = |path: &str| {
            (0..runtime.callback_count())
                .find(|&id| runtime.callback_path(id) == Some(path))
                .unwrap()
        };
        let number = callback("$.value[1].value");
        let text = callback("$.text[1].value");
        let draw = callback("$.destination[1].draw");
        let mut state = TestLuaMainState {
            numbers: BTreeMap::from([(0, 999), (71, 100)]),
            floats: BTreeMap::from([(6, 0.25), (1102, 0.75)]),
            texts: BTreeMap::from([(0, "wrong".into()), (10, "runtime".into())]),
            options: BTreeMap::from([(41, true), (-41, true)]),
            ..Default::default()
        };
        assert_eq!(runtime.evaluate_number(number, &state), Some(101.0));
        assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("runtime1"));
        assert!(runtime.evaluate_draw(draw, &state));
        state.numbers.insert(71, 200);
        state.options.insert(41, false);
        assert_eq!(runtime.evaluate_number(number, &state), Some(202.0));
        assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("runtime2"));
        assert!(!runtime.evaluate_draw(draw, &state));
        assert_eq!(runtime.failure_log_count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn lua_conversion_preserves_typed_names_and_each_condition_in_json() {
    let root = unique_test_dir("named-property-conversion");
    fs::create_dir_all(&root).unwrap();
    let input = root.join("skin.lua");
    let output = root.join("skin.json");
    fs::write(&input, r#"return {
        type=5,
        value={{id='number', value='score'}},
        text={{id='text', value='title'}},
        imageset={{id='set', ref=330, value='score', images={}}},
        slider={{id='slider', value='music_progress'}},
        graph={{id='graph', value='music_progress'}},
        destination={
            {id='number', op={41, '!bgaoff', 'option(40) or option(41)', 'number(71) > 0'}, draw='bgaon'},
            {id='legacy', op={41, -40}}
        }
    }"#).unwrap();
    let warnings =
        convert_lua_skin_to_json_file(&input, &output, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let document = load_beatoraja_json_skin_with_defaults(&output).unwrap();
    use bmz_skin_document::{DestinationListEntry, PropertyFamily};
    assert_eq!(
        document.value[0].value.as_ref().unwrap().resolve_id(PropertyFamily::Integer),
        Some(71)
    );
    assert_eq!(
        document.text[0].value.as_ref().unwrap().resolve_id(PropertyFamily::String),
        Some(10)
    );
    assert_eq!(document.imageset[0].ref_id, 330);
    assert_eq!(document.imageset[0].value, document.value[0].value);
    assert_eq!(
        document.slider[0].value.as_ref().unwrap().resolve_id(PropertyFamily::Rate),
        Some(6)
    );
    assert_eq!(document.graph[0].value, document.slider[0].value);
    let DestinationListEntry::Single(named) = &document.destination[0] else {
        panic!("destination")
    };
    assert_eq!(named.op, [41, -40]);
    assert_eq!(named.op_expr.as_ref(), ["option(40) or option(41)", "number(71) > 0"]);
    assert_eq!(named.draw, "bgaon");
    let DestinationListEntry::Single(legacy) = &document.destination[1] else {
        panic!("destination")
    };
    assert_eq!(legacy.op, [41, -40]);
    assert!(legacy.op_expr.is_empty());
    fs::remove_dir_all(root).unwrap();
}
