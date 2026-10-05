use crate::skin_loader::*;

mod font;
mod pm_chara;
mod source;

use font::decode_skin_fonts;
use pm_chara::prepare_pm_chara;
use source::decode_skin_sources;

/// beatoraja JSON skin の document/フォント/PNG ソースを並列にデコードする。
/// Renderer には触らないので Send-safe で、別スレッドからも呼べる。
pub fn decode_beatoraja_skin(skin_path: &Path, kind: SkinKind) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options(skin_path, kind, &BTreeMap::new(), &BTreeMap::new())
}

/// `decode_beatoraja_skin` のカスタマイズオプション / ファイル選択付き版。
///
/// `options` はオプション名 -> 選択肢名の対応。JSON スキンは選択肢の `op`
/// コード列へ、Lua スキンはそのまま渡して展開する。
///
/// `files` は filepath 定義名 -> 選択ファイルのスキンルート相対パスの対応。
/// Lua スキンは `skin_config.get_path` の解決へ、JSON スキンは `source` /
/// `font` のワイルドカード解決へ反映する。
pub fn decode_beatoraja_skin_with_options(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state(
        skin_path,
        kind,
        options,
        files,
        &LuaLoadRuntimeState::default(),
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state_and_source_cache(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        None,
        None,
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state_and_source_cache(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    source_cache: Option<SharedSkinSourceAssetCache>,
    font_cache: Option<SharedSkinFontCache>,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        None,
        source_cache,
        None,
        font_cache,
        None,
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
    source_cache: Option<SharedSkinSourceAssetCache>,
    texture_cache: Option<SharedSkinGpuTextureCache>,
    font_cache: Option<SharedSkinFontCache>,
    installed_fonts: Option<HashMap<String, SkinFontCacheKey>>,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        library_roots: &[],
        document_cache,
        source_cache,
        texture_cache,
        font_cache,
        installed_fonts,
    })
}

/// Skin decode pipeline に渡す依存を一つにまとめたrequest。
///
/// 公開互換APIは従来の引数列を維持し、内部ではこの型を介してcacheや
/// runtime stateの追加・変更を局所化する。
pub struct BeatorajaSkinDecodeRequest<'a> {
    pub pinned_sources: Option<&'a BTreeMap<String, String>>,
    pub skin_path: &'a Path,
    pub kind: SkinKind,
    pub options: &'a BTreeMap<String, String>,
    pub files: &'a BTreeMap<String, String>,
    pub runtime_state: &'a LuaLoadRuntimeState,
    pub document_cache: Option<SharedSkinDocumentCache>,
    pub source_cache: Option<SharedSkinSourceAssetCache>,
    pub texture_cache: Option<SharedSkinGpuTextureCache>,
    pub font_cache: Option<SharedSkinFontCache>,
    pub installed_fonts: Option<HashMap<String, SkinFontCacheKey>>,
    pub library_roots: &'a [PathBuf],
}

pub fn decode_beatoraja_skin_request(
    request: BeatorajaSkinDecodeRequest<'_>,
) -> Result<DecodedSkin> {
    let BeatorajaSkinDecodeRequest {
        pinned_sources,
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        document_cache,
        source_cache,
        texture_cache,
        font_cache,
        installed_fonts,
        library_roots,
    } = request;
    let path_context = if is_lua_skin_path(skin_path) {
        Some(SkinPathContext::new(skin_path, library_roots.iter().cloned())?)
    } else {
        None
    };
    let document_start = Instant::now();
    let LoadedSkinDocumentForDecode {
        dependencies: load_dependencies,
        mut document,
        lua_runtime,
        files: resolved_files,
        cache_status,
    } = load_skin_document_with_path_context(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        document_cache,
        path_context.as_ref(),
    )?;
    let document_us = elapsed_us(document_start);
    if kind == SkinKind::Select
        && (options.get("bmz_detail_options").map(String::as_str) != Some("1")
            || !document.bmz_detail_options_numbers)
    {
        document.bmz_detail_options = 0;
        document.bmz_detail_options_close = false;
    }
    if let Some(pinned) = pinned_sources {
        for source in &mut document.source {
            if let Some(path) = pinned.get(&source.path) {
                source.path.clone_from(path);
            }
        }
    }
    // フォント ID は scene 横断的に Renderer のグローバルマップに登録されるので、
    // play / select / result で同じ "0" 等が衝突する。namespace を付与して隔離する。
    // text 定義の font 参照側も同じ namespace を付ける。
    let font_namespace = kind.font_namespace();
    for text in &mut document.text {
        if !text.font.is_empty() {
            text.font = format!("{}:{}", font_namespace, text.font);
        }
    }
    let skin_root = skin_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    prepare_pm_chara(&mut document, &skin_root, &resolved_files, path_context.as_ref());
    let audio_assets = decode_skin_audio_assets(kind, &skin_root, path_context.as_ref(), &document);
    let required_sources: HashSet<String> =
        required_skin_source_ids(&document).into_iter().map(str::to_string).collect();
    let warn_missing_required = kind.warn_missing_required_sources();

    let decoded_fonts = decode_skin_fonts(
        &document,
        &skin_root,
        &resolved_files,
        path_context.as_ref(),
        font_namespace,
        font_cache.as_ref(),
        installed_fonts.as_ref(),
    );

    let decoded_sources = decode_skin_sources(
        &document,
        &skin_root,
        &resolved_files,
        path_context.as_ref(),
        &required_sources,
        warn_missing_required,
        source_cache.as_ref(),
        texture_cache.as_ref(),
    );
    let decoded_pairs = decoded_sources.pairs;

    let mut stats = SkinDecodeStats {
        document_us,
        document_cache_hits: usize::from(cache_status == DocumentCacheStatus::Hit),
        document_cache_misses: usize::from(cache_status == DocumentCacheStatus::Miss),
        document_cache_uncacheable: usize::from(cache_status == DocumentCacheStatus::Uncacheable),
        document_cache_disabled: usize::from(cache_status == DocumentCacheStatus::Disabled),
        font_count: decoded_fonts.count,
        font_decode_us: decoded_fonts.decode_us,
        font_payload_skipped: decoded_fonts.payload_skipped,
        font_cache_hits: decoded_fonts.cache_hits,
        font_cache_misses: decoded_fonts.cache_misses,
        font_cache_uncacheable: decoded_fonts.cache_uncacheable,
        font_cache_disabled: decoded_fonts.cache_disabled,
        source_task_count: decoded_sources.task_count,
        source_decode_us: decoded_sources.decode_us,
        ..Default::default()
    };
    for decoded in &decoded_pairs {
        stats.decoded_source_count += 1;
        if let Some(asset) = &decoded.asset {
            stats.decoded_source_bytes =
                stats.decoded_source_bytes.saturating_add(asset.pixels.len());
        }
        if matches!(decoded.texture_status, Some(TextureCacheStatus::Hit)) {
            stats.source_texture_cache_hits += 1;
            let bytes = (decoded.size.width.max(0.0) as usize)
                .saturating_mul(decoded.size.height.max(0.0) as usize)
                .saturating_mul(4);
            stats.source_texture_cache_hit_bytes =
                stats.source_texture_cache_hit_bytes.saturating_add(bytes);
            if decoded.is_video {
                stats.video_source_texture_cache_hits += 1;
                stats.video_source_texture_cache_hit_bytes =
                    stats.video_source_texture_cache_hit_bytes.saturating_add(bytes);
            }
        }
        match (decoded.is_video, &decoded.source_status, &decoded.texture_status) {
            (_, None, None) => stats.builtin_source_count += 1,
            (true, None, Some(TextureCacheStatus::Hit)) => stats.video_source_count += 1,
            (false, None, Some(TextureCacheStatus::Hit)) => stats.image_source_count += 1,
            (true, Some(_), _) => stats.video_source_count += 1,
            (false, Some(_), _) => stats.image_source_count += 1,
            (_, None, Some(_)) => {}
        }
        match decoded.source_status {
            Some(SourceCacheStatus::Hit) => {
                stats.source_cache_hits += 1;
                if decoded.is_video {
                    stats.video_source_cache_hits += 1;
                }
            }
            Some(SourceCacheStatus::Miss) => {
                stats.source_cache_misses += 1;
                if decoded.is_video {
                    stats.video_source_cache_misses += 1;
                }
            }
            Some(SourceCacheStatus::Uncacheable) => {
                stats.source_cache_uncacheable += 1;
                if decoded.is_video {
                    stats.video_source_cache_uncacheable += 1;
                }
            }
            Some(SourceCacheStatus::Disabled) => {
                stats.source_cache_disabled += 1;
                if decoded.is_video {
                    stats.video_source_cache_disabled += 1;
                }
            }
            None => {}
        }
    }

    let mut next_texture_id = kind.first_texture_id();
    let sources: Vec<DecodedSource> = decoded_pairs
        .into_iter()
        .map(|decoded| {
            let texture = decoded.cached_texture.unwrap_or_else(|| {
                let texture = SkinTextureId(next_texture_id);
                next_texture_id += 1;
                texture
            });
            DecodedSource {
                source_id: decoded.source_id,
                path: decoded.path,
                texture,
                asset: decoded.asset,
                size: decoded.size,
                cache_key: decoded.cache_key,
                is_video: decoded.is_video,
                texture_lease: decoded.texture_lease,
            }
        })
        .collect();

    Ok(DecodedSkin {
        load_dependencies,
        kind,
        document,
        lua_runtime,
        fonts: decoded_fonts.fonts,
        sources,
        audio_assets,
        stats,
    })
}
