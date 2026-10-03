use super::*;

const FONT_DXA: &[u8] = include_bytes!("../../../../../bmz-skin-assets/tests/fixtures/font-v3.dxa");

#[test]
fn dxa_font_and_image_decode_without_extraction_and_refresh_caches() {
    let root = unique_test_dir("bmz-dxa-font-image");
    fs::create_dir_all(&root).unwrap();
    let archive = root.join("font.dxa");
    fs::write(&archive, FONT_DXA).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,0,DXA test,Test\n\
        #LR2FONT,font/font.lr2font\n\
        #IMAGE,font/page.bmp\n\
        #SRC_IMAGE,0,0,0,0,1,1,1,1,0,0\n\
        #DST_IMAGE,0,0,0,0,1,1,0,255,255,255,255,1,0,0,0,0,0,0,0,0\n",
    )
    .unwrap();
    let font_path = root.join("font/font.lr2font");
    let image_path = root.join("font/page.bmp");
    let cache = Arc::new(Mutex::new(SkinFontCache::default()));
    let (_, first_status, old_key) = decode_font_with_cache(&font_path, Some(&cache)).unwrap();
    assert_eq!(first_status, FontCacheStatus::Miss);
    let (_, second_status, _) = decode_font_with_cache(&font_path, Some(&cache)).unwrap();
    assert_eq!(second_status, FontCacheStatus::Hit);
    let old_source_key = skin_source_asset_cache_key(&image_path, false).unwrap();
    let decode = |installed| {
        decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &path,
            SkinKind::Play,
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
    let installed = HashMap::from([("play:lr2font-0".into(), old_key.unwrap())]);
    let decoded = decode(Some(installed.clone()));
    assert_eq!(decoded.stats.font_payload_skipped, 1);
    assert_eq!(decoded.sources.len(), 1);
    assert_eq!(decoded.sources[0].asset.as_ref().unwrap().pixels, [255, 0, 0, 255]);

    fs::File::options()
        .write(true)
        .open(&archive)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
        .unwrap();
    assert_ne!(skin_source_asset_cache_key(&image_path, false).unwrap(), old_source_key);
    let decoded = decode(Some(installed));
    assert_eq!(decoded.stats.font_payload_skipped, 0);
    assert_eq!(decoded.stats.font_cache_misses, 1);
    let Some(DecodedFontData::Bitmap(font)) = &decoded.fonts[0].data else {
        panic!("archived bitmap font")
    };
    assert_eq!(font.glyphs[&'A'].width, 1);
    assert_eq!(font.pages[&0].image.pixels, [255, 0, 0, 255]);
    assert!(!root.join("font").exists());
    assert_eq!(fs::read(&archive).unwrap(), FONT_DXA);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dxa_asset_paths_and_bitmap_page_paths_obey_skin_roots() {
    let root = unique_test_dir("bmz-dxa-sandbox");
    let package = root.join("package");
    fs::create_dir_all(&package).unwrap();
    fs::write(root.join("outside.dxa"), FONT_DXA).unwrap();
    fs::write(package.join("inside.dxa"), FONT_DXA).unwrap();
    // The package-root sandbox is the Lua loader's contract. Legacy JSON/CSV
    // callers retain their existing path resolution policy.
    let path = package.join("skin.luaskin");
    fs::write(&path, r#"return {type=0,font={{id="font",path="font.lr2font"}}}"#).unwrap();
    fs::write(package.join("font.lr2font"), "#S,1\n#T,0,../outside/page.bmp\n#R,65,0,0,0,1,1\n")
        .unwrap();
    let context = SkinPathContext::for_entry(&path).unwrap();
    assert!(context.resolve_asset("inside/font.lr2font").is_ok());
    assert!(context.resolve_asset("../outside/font.lr2font").is_err());
    assert!(context.resolve_asset(&root.join("outside/font.lr2font").to_string_lossy()).is_err());
    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();
    assert!(decoded.fonts.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn kcool_decodes_all_archived_fonts_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/KCOOL SKIN (Ver 1.72)");
    let path = root.join("Play/7key.lr2skin");
    if !path.is_file() {
        return;
    }
    for folder in ["barfont", "SystemFont", "title"] {
        assert!(root.join(format!("Font/{folder}.dxa")).is_file());
        assert!(!root.join("Font").join(folder).exists());
    }
    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();
    assert_eq!(decoded.fonts.len(), 3);
    for font in &decoded.fonts {
        let Some(DecodedFontData::Bitmap(bitmap)) = &font.data else {
            panic!("missing bitmap font {}", font.stored_id)
        };
        assert!(bitmap.glyphs.len() > 1000, "{}", font.stored_id);
        assert!(bitmap.pages.len() > 1, "{}", font.stored_id);
        assert!(bitmap.pages.values().all(|page| !page.image.pixels.is_empty()));
        assert!(skin_font_cache_key(&font.path).is_some());
    }
}

#[test]
fn kcool_all_judge_sprites_reach_rendering_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/KCOOL SKIN (Ver 1.72)/Play/7key.lr2skin");
    if !path.is_file() {
        return;
    }
    let files = BTreeMap::from([("Combo".to_owned(), "Combo-1.png".to_owned())]);
    let decoded =
        decode_beatoraja_skin_with_options(&path, SkinKind::Play, &BTreeMap::new(), &files)
            .unwrap();
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect();
    for name in ["PGREAT", "GREAT", "GOOD", "BAD", "POOR"] {
        for time in [0, 40, 100, 499, 500] {
            let items = decoded.document.judge_render_items(name, 123, time, &sources).unwrap();
            assert!(
                matches!(items.first(), Some(SkinRenderItem::Image { rect, tint, uv, .. })
                if rect.width > 0.0 && rect.height > 0.0 && tint.a > 0.0 && uv.width > 0.0 && uv.height > 0.0),
                "{name} at {time}: {items:?}"
            );
        }
    }
}
