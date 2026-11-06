use super::*;

#[test]
fn lr2_missing_page_can_be_cached_and_reappearing_page_invalidates_it() {
    let root = tempfile::tempdir().unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))
        .save(root.path().join("p0.tga"))
        .unwrap();
    fs::write(
        root.path().join("font.lr2font"),
        "#S,1\n#T,0,p0.tga\n#T,1,missing.tga\n#R,65,0,0,0,1,1\n#R,66,1,0,0,1,1\n",
    )
    .unwrap();
    let entry = root.path().join("skin.json");
    fs::write(&entry, r#"{"type":5,"font":[{"id":"test","path":"font.lr2font"}]}"#).unwrap();
    let cache = Arc::new(Mutex::new(SkinFontCache::default()));
    let load = |installed| {
        decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &entry,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
            None,
            None,
            Some(cache.clone()),
            installed,
        )
        .unwrap()
    };
    let first = load(None);
    assert_eq!(first.fonts.len(), 1);
    assert_eq!(first.stats.font_cache_misses, 1);
    let old_key = first.fonts[0].cache_key.clone().unwrap();
    let installed = HashMap::from([(first.fonts[0].stored_id.clone(), old_key.clone())]);
    let Some(DecodedFontData::Bitmap(font)) = &first.fonts[0].data else { panic!("bitmap font") };
    assert_eq!(font.pages.len(), 1);
    assert!(!font.glyphs.contains_key(&'B'));
    assert_eq!(load(None).stats.font_cache_hits, 1);
    assert_eq!(load(Some(installed.clone())).stats.font_payload_skipped, 1);
    image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]))
        .save(root.path().join("missing.tga"))
        .unwrap();
    let restored = load(Some(installed));
    assert_eq!(restored.stats.font_cache_misses, 1);
    assert_ne!(restored.fonts[0].cache_key.as_ref(), Some(&old_key));
    let Some(DecodedFontData::Bitmap(font)) = &restored.fonts[0].data else {
        panic!("bitmap font")
    };
    assert_eq!(font.pages.len(), 2);
    assert!(font.glyphs.contains_key(&'B'));
}

#[test]
#[cfg(unix)]
fn bitmap_page_decode_and_cache_use_the_validated_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(outside.join("child")).unwrap();
    std::os::unix::fs::symlink(outside.join("child"), root.join("link")).unwrap();
    let safe = [0, 255, 0, 255];
    image::RgbaImage::from_pixel(1, 1, image::Rgba(safe)).save(root.join("page.png")).unwrap();
    // An unchecked read of link/../page.png would read this outside file.
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(outside.join("page.png"))
        .unwrap();
    for (extension, definition) in [
        (
            "fnt",
            "info size=1\ncommon lineHeight=1 base=1 scaleW=1 scaleH=1\npage id=0 file=link/../page.png\nchar id=65 x=0 y=0 width=1 height=1 xoffset=0 yoffset=0 xadvance=1 page=0\n",
        ),
        ("lr2font", "#S,1\n#T,0,link/../page.png\n#R,65,0,0,0,1,1\n"),
    ] {
        let font = root.join(format!("font.{extension}"));
        fs::write(&font, definition).unwrap();
        let entry = root.join("skin.json");
        fs::write(
            &entry,
            serde_json::json!({"type":5,"font":[{"id":"test","path":format!("font.{extension}")}]})
                .to_string(),
        )
        .unwrap();
        let context = SkinPathContext::new(&entry, [root.clone()]).unwrap();
        let key = skin_font_cache_key_with_context(&font, &context).unwrap();
        assert_eq!(key.pages[0].0, root.join("page.png").canonicalize().unwrap());
        let cache = Arc::new(Mutex::new(SkinFontCache::default()));
        for expected_status in [FontCacheStatus::Miss, FontCacheStatus::Hit] {
            let (decoded, status, _) =
                decode_font_with_cache_key(&font, &context, Some(&cache), Some(key.clone()))
                    .unwrap();
            assert_eq!(status, expected_status);
            let DecodedFontData::Bitmap(decoded) = decoded else { panic!("bitmap font") };
            assert_eq!(decoded.pages[&0].image.pixels, safe);
            assert_eq!(decoded.pages[&0].path, key.pages[0].0);
        }
        let decoded = decode_beatoraja_skin(&entry, SkinKind::Select).unwrap();
        assert_eq!(decoded.fonts.len(), 1);
        let Some(DecodedFontData::Bitmap(font)) = &decoded.fonts[0].data else {
            panic!("bitmap payload")
        };
        assert_eq!(font.pages[&0].image.pixels, safe);
    }
}

#[test]
fn lr2_cache_keys_change_when_shared_root_permission_changes() {
    let root = tempfile::tempdir().unwrap();
    let theme = root.path().join("theme");
    let shared = root.path().join("shared");
    fs::create_dir(&theme).unwrap();
    fs::create_dir(&shared).unwrap();
    let entry = theme.join("skin.lr2skin");
    fs::write(&entry, "#INFORMATION,5,Cache,Test\n").unwrap();
    let narrow = SkinPathContext::new(&entry, [theme.clone()]).unwrap();
    let broad = SkinPathContext::new(&entry, [theme, shared]).unwrap();
    assert_ne!(
        skin_document_cache_key(&entry, SkinKind::Select, Some(&narrow)),
        skin_document_cache_key(&entry, SkinKind::Select, Some(&broad))
    );
    assert!(
        load_skin_document_with_path_context(
            &entry,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn every_format_resolves_sources_under_explicit_shared_roots() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir_all(root.join("theme")).unwrap();
    fs::create_dir_all(root.join("shared")).unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
        .save(root.join("shared/pixel.png"))
        .unwrap();
    fs::copy(root.join("shared/pixel.png"), temp.path().join("outside.png")).unwrap();
    let outside = temp.path().join("outside.png").to_string_lossy().replace('\\', "/");
    for (name, content) in [
        ("skin.json", serde_json::json!({"type":5,"source":[{"id":"safe","path":"../shared/pixel.png"},{"id":"outside","path":outside}]}).to_string()),
        ("skin.lua", format!("return {{type=5,source={{{{id='safe',path='../shared/pixel.png'}},{{id='outside',path='{outside}'}}}}}}")),
        ("skin.lr2skin", format!("#INFORMATION,5,Boundary,Test\n#RESOLUTION,0\n#IMAGE,../shared/pixel.png\n#IMAGE,{outside}\n")),
    ] {
        let path = root.join("theme").join(name);
        fs::write(&path, content).unwrap();
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            skin_path: &path, kind: SkinKind::Select,
            options: &BTreeMap::new(), files: &BTreeMap::new(), runtime_state: &LuaLoadRuntimeState::default(),
            library_roots: std::slice::from_ref(&root), pinned_sources: None,
            document_cache: None, source_cache: None, texture_cache: None, font_cache: None, installed_fonts: None,
        }).unwrap();
        assert_eq!(decoded.sources.len(), 1, "{name}");
    }
}

#[test]
#[cfg(unix)]
fn linked_assets_wildcards_font_pages_and_pmchara_share_the_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    fs::create_dir_all(root.join("parts")).unwrap();
    let entry = root.join("skin.json");
    fs::write(&entry, "{}").unwrap();
    fs::write(root.join("parts/inside.png"), []).unwrap();
    fs::write(temp.path().join("outside.png"), []).unwrap();
    std::os::unix::fs::symlink(temp.path().join("outside.png"), root.join("parts/outside.png"))
        .unwrap();
    let context = SkinPathContext::new(&entry, [root.clone()]).unwrap();
    assert!(context.resolve_asset("parts/outside.png").is_err());
    assert_eq!(
        context.wildcard_candidates("parts/*.png").unwrap(),
        vec![root.join("parts/inside.png").canonicalize().unwrap()]
    );
    assert!(
        resolve_json_skin_asset_path_with_context(
            &root,
            None,
            "parts/inside.png",
            &serde_json::from_str("{}").unwrap(),
            &BTreeMap::new()
        )
        .is_none()
    );
    for (name, good, bad) in [
        (
            "font.fnt",
            "page id=0 file=\"parts/inside.png\"\n",
            "page id=0 file=\"parts/outside.png\"\n",
        ),
        ("font.lr2font", "#T,0,parts/inside.png\n", "#T,0,parts/outside.png\n"),
    ] {
        let path = root.join(name);
        fs::write(&path, good).unwrap();
        assert_eq!(
            bmz_render::bitmap_font::bitmap_font_page_paths_with_context(&path, &context)
                .unwrap()
                .len(),
            1
        );
        fs::write(&path, bad).unwrap();
        assert!(
            bmz_render::bitmap_font::bitmap_font_page_paths_with_context(&path, &context).is_err()
        );
    }
    fs::write(temp.path().join("outside.chp"), "invalid chp").unwrap();
    std::os::unix::fs::symlink(temp.path().join("outside.chp"), root.join("parts/outside.chp"))
        .unwrap();
    assert!(
        bmz_skin::load_pm_chara_with_context(&root.join("parts/outside.chp"), &context, "pm", 0, 0)
            .is_err()
    );
    assert!(
        bmz_skin::load_pm_chara_with_context(&root.join("parts"), &context, "pm", 0, 0).is_err()
    );
}
