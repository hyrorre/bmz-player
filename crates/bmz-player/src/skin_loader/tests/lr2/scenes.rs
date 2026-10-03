use super::*;

#[test]
fn lr2_result_cache_follows_clear_assets_and_canvas_selection() {
    let root = unique_test_dir("bmz-lr2-result-cache");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("result.lr2skin");
    fs::write(&path, "#INFORMATION,7,test,test\n#IF,90\n#IMAGE,clear.png\n#ELSE\n#IMAGE,fail.png\n#ENDIF\n#SRC_IMAGE,0,0,0,0,10,10,1,1,0,0\n#DST_IMAGE,0,0,0,0,10,10\n").unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));
    let load = |clear: bool, options: &BTreeMap<String, String>| {
        let runtime = LuaLoadRuntimeState {
            option_values: BTreeMap::from([(90, clear), (91, !clear)]),
            ..Default::default()
        };
        load_skin_document(
            &path,
            SkinKind::Result,
            options,
            &BTreeMap::new(),
            &runtime,
            Some(cache.clone()),
        )
        .unwrap()
    };
    let first = load(true, &BTreeMap::new());
    assert_eq!(first.document.source[0].path, "clear.png");
    assert_eq!(load(true, &BTreeMap::new()).cache_status, DocumentCacheStatus::Hit);
    let fail = load(false, &BTreeMap::new());
    assert_eq!(fail.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(fail.document.source[0].path, "fail.png");
    let hd = load(false, &BTreeMap::from([("LR2 Resolution (BMZ)".into(), "1280x720".into())]));
    assert_eq!(hd.cache_status, DocumentCacheStatus::Miss);
    assert_eq!((hd.document.w, hd.document.h), (1280, 720));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_added_scene_assets_decode_through_app_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    for (relative, kind) in [
        ("LR2/Select/select.lr2skin", SkinKind::Select),
        ("WMIX_HD/select/select.lr2skin", SkinKind::Select),
        ("Seraphic/Select/[+]select.lr2skin", SkinKind::Select),
        ("ECBE/Select/select.lr2skin", SkinKind::Select),
        ("LR2/Result/result.lr2skin", SkinKind::Result),
        ("WMIX_HD/result/WMIX_RESULT.lr2skin", SkinKind::Result),
        ("WMIX_HD/courseresult/courseresult.lr2skin", SkinKind::Result),
        ("Seraphic/Result/[+]result.lr2skin", SkinKind::Result),
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            eprintln!("SKIP missing {relative}");
            continue;
        }
        let options = if relative.starts_with("WMIX_HD") {
            BTreeMap::from([("LR2 Resolution (BMZ)".into(), "1280x720".into())])
        } else {
            BTreeMap::new()
        };
        let runtime_state = LuaLoadRuntimeState {
            option_values: BTreeMap::from([(90, true), (91, false)]),
            ..Default::default()
        };
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            skin_path: &path,
            kind,
            options: &options,
            files: &BTreeMap::new(),
            runtime_state: &runtime_state,
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
            library_roots: std::slice::from_ref(&root),
            pinned_sources: None,
        })
        .unwrap_or_else(|error| panic!("{relative}: {error:#}"));
        assert!(
            decoded.sources.iter().any(|source| source.source_id == "0"),
            "{relative}: primary atlas was not decoded"
        );
        eprintln!("{relative}: {} decoded image sources", decoded.sources.len());
        if relative == "LR2/Select/select.lr2skin" {
            assert!(
                decoded.sources.iter().any(|source| source.source_id == "2"),
                "sibling Decide atlas"
            );
        }
    }
}
