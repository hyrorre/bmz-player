use super::*;

#[test]
fn songlist_fonts_materials_and_fallbacks_use_the_normal_loader() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bmz-render/testdata/fonts");
    let root = unique_test_dir("songlist-fonts");
    fs::create_dir_all(&root).unwrap();
    fs::copy(fixtures.join("bmz-test-latin.ttf"), root.join("font.ttf")).unwrap();
    fs::copy(fixtures.join("bmz-test-symbols.ttf"), root.join("fallback.ttf")).unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
        .save(root.join("page.png"))
        .unwrap();
    fs::write(root.join("font.fnt"),"info face=Test size=1\ncommon lineHeight=1 base=1 scaleW=1 scaleH=1\npage id=0 file=\"page.png\"\nchar id=65 x=0 y=0 width=1 height=1 xoffset=0 yoffset=0 xadvance=1 page=0\n").unwrap();
    fs::write(root.join("font.lr2font"), "#S,1\n#T,0,page.png\n#R,65,0,0,0,1,1\n").unwrap();
    let skin = serde_json::json!({
        "type":5,"font":[
            {"id":"vector","path":"font.ttf","fallback":["fallback.ttf"]},
            {"id":"bitmap","path":"font.fnt"},{"id":"lr2","path":"font.lr2font"}],
        "source":[{"id":"atlas","path":"page.png"}],
        "image":[{"id":"rank","src":"atlas","w":1,"h":1}],
        "songlist":{"id":"list",
            "scorerank":[{"mode":"text","text":{"font":"vector"}},{"mode":"image","images":["rank"]}],
            "minbp":{"mode":"text","text":{"font":"bitmap"}},
            "mincb":{"mode":"text","text":{"font":"lr2"}},
            "rivaldiff":{"mode":"digits","digits":{"src":"atlas","divx":12,"divy":2,"digit":5}}},
        "destination":[{"id":"list"}]
    });
    let path = root.join("select.json");
    fs::write(&path, serde_json::to_vec(&skin).unwrap()).unwrap();
    let document: SkinDocument = serde_json::from_value(skin).unwrap();
    assert!(required_skin_source_ids(&document).contains("atlas"));
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Default::default(),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let list = decoded.document.songlist.as_ref().unwrap();
    for (entry, id) in
        [(&list.scorerank[0], "vector"), (&list.minbp[0], "bitmap"), (&list.mincb[0], "lr2")]
    {
        let bmz_render::skin::SongListFieldRender::Text { text, .. } = &entry.render else {
            panic!()
        };
        assert_eq!(text.font, format!("select:{id}"));
        let font = decoded.fonts.iter().find(|f| f.stored_id == text.font).unwrap();
        assert!(font.data.is_some());
    }
    assert!(
        decoded
            .fonts
            .iter()
            .any(|f| f.stored_id == "select:vector" && f.fallbacks == ["select:vector#fallback0"])
    );
    assert!(
        decoded.fonts.iter().any(|f| f.stored_id == "select:bitmap"
            && matches!(&f.data, Some(DecodedFontData::Bitmap(_))))
    );
}
