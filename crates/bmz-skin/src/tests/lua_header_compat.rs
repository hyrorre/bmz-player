use super::*;

#[test]
fn lua_header_properties_use_beatoraja_scalar_coercions() {
    let root = TestDir::created("bmz-lua-header-coercions");
    let entry = root.join("select.luaskin");
    fs::write(
        &entry,
        r#"
        local header = {type=5, property={
            {name='Direction', item={{name='Normal', op=920}, {name='Reverse', op='縦'}}},
            {name='Label', item={{name='Bottom', op=955}, {name='Top', op=956}}, def=955},
            {name='Numbers', item={{name='A', op='923.9'}, {name='B', op=-2.9},
                {name='C', op=false}, {name='D', op={}}, {name='E', op='0x20'}}},
        }, filepath={{name='Image', path='*.png', def=123}}}
        if skin_config then
            assert(skin_config.option.Direction == 920)
            assert(skin_config.option.Label == 955)
        end
        return header
        "#,
    )
    .unwrap();
    let header = load_lua_skin_header_value(&entry).unwrap();
    let header: SkinDocument = serde_json::from_value(header.value).unwrap();
    assert_eq!(header.property[0].item[1].op, 0);
    assert_eq!(header.property[1].def, "955");
    assert_eq!(header.filepath[0].def, "123");
    assert_eq!(
        header.property[2].item.iter().map(|item| item.op).collect::<Vec<_>>(),
        [923, -2, 0, 0, 32]
    );
    let loaded =
        load_lua_skin(&entry, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.document.property, header.property);
}

#[test]
fn installed_social_skin_headers_decode_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    for name in ["select", "result", "result_course"] {
        let entry = root.join(format!("SocialSkin/lua/{name}.luaskin"));
        if !entry.is_file() {
            continue;
        }
        let context = SkinPathContext::new(&entry, [root.clone()]).unwrap();
        let loaded = load_lua_skin_header_value_with_path_context(&context).unwrap();
        let document: SkinDocument = serde_json::from_value(loaded.value).unwrap();
        assert_eq!(
            document.skin_type,
            if name == "select" {
                5
            } else if name == "result" {
                7
            } else {
                15
            }
        );
    }
}
