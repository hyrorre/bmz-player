use super::*;
use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::Instant;

use bmz_core::time::TimeUs;
use bmz_render::plan::{DrawCommand, DrawPlan};
use bmz_render::renderer::Renderer;
use bmz_render::scene::{AppSceneSnapshot, SelectRowSnapshot, SelectSnapshot};
use bmz_render::skin::{
    DestinationListEntry, DynamicTimerRuntime, SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION,
    SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER, SKIN_REF_BMZ_SELECT_SESSION_MODE, SkinContext,
    SkinDocumentRenderExt, SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinManifest,
    SkinRenderItem, SkinTextState,
};

fn test_app_paths() -> AppPaths {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    AppPaths::from_dirs(data.clone(), data.clone(), data.join("cache"), data.join("logs"))
}

fn filepath_def(name: &str, path: &str, def: &str) -> SkinFilepathDef {
    SkinFilepathDef {
        category: String::new(),
        name: name.to_string(),
        path: path.to_string(),
        def: def.to_string(),
    }
}

fn test_font_cache_key(path: &str) -> SkinFontCacheKey {
    SkinFontCacheKey {
        path: PathBuf::from(path),
        modified: None,
        len: 0,
        is_bitmap: false,
        pages: Vec::new(),
    }
}

fn unique_test_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    path
}

#[path = "tests/cache.rs"]
mod cache;
#[path = "tests/detail_options.rs"]
mod detail_options;
#[path = "tests/document.rs"]
mod document;
#[path = "tests/ecfn_detail_options.rs"]
mod ecfn_detail_options;
#[path = "tests/lr2.rs"]
mod lr2;
#[path = "tests/lua.rs"]
mod lua;
#[path = "tests/luxe_detail_options.rs"]
mod luxe_detail_options;
#[path = "tests/mz_detail_options.rs"]
mod mz_detail_options;
#[path = "tests/paths.rs"]
mod paths;
#[path = "tests/pm_chara.rs"]
mod pm_chara;
