use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextureCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FontCacheStatus {
    Hit,
    Miss,
    SkippedInstalled,
    Uncacheable,
    Disabled,
}

/// GPU アップロード済みの 1 ソース。upload worker が `DecodedSource` から生成する。
pub struct PreparedSource {
    pub texture_lease: Option<Arc<()>>,
    pub source_id: String,
    pub path: PathBuf,
    pub texture: SkinTextureId,
    pub prepared: Option<PreparedTexture>,
    pub size: SkinImageSize,
    pub is_video: bool,
    pub cache_key: Option<SkinSourceAssetCacheKey>,
}

/// decode + GPU アップロードまで終わった 1 スキンぶん。upload worker → main で渡す。
/// `PreparedTexture` (= wgpu::Texture/View) は `Send` なのでスレッド間で受け渡せる。
pub struct UploadedSkin {
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub lua_runtime: Option<LuaSkinRuntime>,
    pub fonts: Vec<DecodedFont>,
    pub prepared: Vec<PreparedSource>,
    pub audio_assets: Vec<DecodedSkinAudio>,
    pub decode_stats: SkinDecodeStats,
    pub upload_stats: SkinUploadStats,
}

#[derive(Debug)]
pub(super) struct LuaSkinDrawRuntimeAdapter {
    // `take` before evaluation makes it impossible to hold this mutex while Lua
    // executes. Renderer evaluation is single-threaded; a concurrent/reentrant
    // attempt observes None and safely falls back to false.
    pub(super) runtime: Mutex<Option<LuaSkinRuntime>>,
    bound_state: Mutex<Option<RenderLuaStateKey>>,
}

impl LuaSkinDrawRuntimeAdapter {
    pub(super) fn new(runtime: LuaSkinRuntime) -> Self {
        Self { runtime: Mutex::new(Some(runtime)), bound_state: Mutex::new(None) }
    }

    fn state_is_bound(
        &self,
        state: &SkinDrawState,
        options: &[i32],
        text: &BTreeMap<i32, String>,
    ) -> bool {
        self.bound_state
            .lock()
            .is_ok_and(|key| *key == Some(RenderLuaStateKey::new(state, options, text)))
    }
}

/// Addresses are only compared, never dereferenced. The with_state borrow keeps
/// these objects alive and immutable until the guard removes the key. In
/// particular, a cloned/mutated Select row cannot reuse the outer provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderLuaStateKey {
    state: usize,
    options: usize,
    options_len: usize,
    text: usize,
    thread: std::thread::ThreadId,
}

impl RenderLuaStateKey {
    fn new(state: &SkinDrawState, options: &[i32], text: &BTreeMap<i32, String>) -> Self {
        Self {
            state: std::ptr::from_ref(state) as usize,
            options: options.as_ptr() as usize,
            options_len: options.len(),
            text: std::ptr::from_ref(text) as usize,
            thread: std::thread::current().id(),
        }
    }
}

struct RenderLuaStateGuard<'a> {
    slot: &'a Mutex<Option<RenderLuaStateKey>>,
    previous: Option<RenderLuaStateKey>,
}

impl Drop for RenderLuaStateGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = self.previous;
        }
    }
}

pub(super) struct RenderLuaMainState<'a> {
    pub(super) state: &'a SkinDrawState,
    pub(super) enabled_options: &'a [i32],
    pub(super) text_values: &'a BTreeMap<i32, String>,
}

impl LuaMainState for RenderLuaMainState<'_> {
    fn option(&self, id: i32) -> bool {
        lua_main_state_option(id, self.enabled_options, self.state)
    }

    fn number(&self, id: i32) -> i64 {
        lua_main_state_number(id, self.state)
    }

    fn float(&self, id: i32) -> f64 {
        lua_main_state_float(id, self.state)
    }

    fn text(&self, id: i32) -> String {
        self.text_values.get(&id).cloned().unwrap_or_default()
    }

    fn timer(&self, id: i32) -> Option<i32> {
        lua_main_state_timer(id, self.state)
    }

    fn event_index(&self, id: i32) -> i32 {
        lua_main_state_event_index(id, self.state)
    }

    fn gauge_type(&self) -> i32 {
        self.state.gauge_type
    }

    fn time_us(&self) -> i32 {
        self.state.elapsed_ms.saturating_mul(1_000)
    }

    fn total_play_counts_in_session(&self) -> i64 {
        self.state.player_stats.session_play_count.min(i64::MAX as u64) as i64
    }

    fn total_play_notes_in_session(&self) -> i64 {
        self.state.player_stats.session_play_notes.min(i64::MAX as u64) as i64
    }

    fn score_date_sec_time(&self) -> i64 {
        self.state.score_date_sec
    }

    fn offset(&self, id: i32) -> bmz_skin::LuaSkinOffsetValue {
        let value = self.state.skin_offsets.get(id).unwrap_or_default();
        bmz_skin::LuaSkinOffsetValue {
            x: value.x,
            y: value.y,
            w: value.w,
            h: value.h,
            r: value.r,
            a: value.a,
        }
    }
}

impl SkinLuaDrawRuntime for LuaSkinDrawRuntimeAdapter {
    fn with_state(
        &self,
        state: &SkinDrawState,
        enabled_options: &[i32],
        text_values: &BTreeMap<i32, String>,
        run: &mut dyn FnMut(),
    ) {
        let scope = self
            .runtime
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(LuaSkinRuntime::state_scope));
        let Some(scope) = scope else {
            run();
            return;
        };
        let key = RenderLuaStateKey::new(state, enabled_options, text_values);
        let guard = self.bound_state.lock().ok().and_then(|mut slot| {
            // Do not interleave scope guards from different threads.
            if slot.is_some_and(|bound| bound.thread != key.thread) {
                return None;
            }
            Some(RenderLuaStateGuard { slot: &self.bound_state, previous: slot.replace(key) })
        });
        let Some(guard) = guard else {
            run();
            return;
        };
        let provider = RenderLuaMainState { state, enabled_options, text_values };
        let mut called = false;
        let _ = scope.with_state(&provider, || {
            called = true;
            run();
        });
        drop(guard);
        // If scope setup fails, preserve the ordinary per-callback fallback.
        if !called {
            run();
        }
    }

    fn begin_frame(&self) {
        if let Ok(mut slot) = self.runtime.lock()
            && let Some(runtime) = slot.as_mut()
        {
            runtime.begin_frame();
        }
    }

    fn evaluate_draw(
        &self,
        callback_id: usize,
        state: &SkinDrawState,
        enabled_options: &[i32],
        text_values: &BTreeMap<i32, String>,
    ) -> bool {
        let Some(mut runtime) = self.runtime.lock().ok().and_then(|mut slot| slot.take()) else {
            return false;
        };
        let provider = RenderLuaMainState { state, enabled_options, text_values };
        let result = if self.state_is_bound(state, enabled_options, text_values) {
            runtime.evaluate_draw_in_scope(callback_id)
        } else {
            runtime.evaluate_draw(callback_id, &provider)
        };
        if let Ok(mut slot) = self.runtime.lock() {
            *slot = Some(runtime);
        }
        result
    }

    fn evaluate_number(
        &self,
        callback_id: usize,
        state: &SkinDrawState,
        enabled_options: &[i32],
        text_values: &BTreeMap<i32, String>,
    ) -> Option<f64> {
        let mut runtime = self.runtime.lock().ok().and_then(|mut slot| slot.take())?;
        let provider = RenderLuaMainState { state, enabled_options, text_values };
        let result = if self.state_is_bound(state, enabled_options, text_values) {
            runtime.evaluate_number_in_scope(callback_id)
        } else {
            runtime.evaluate_number(callback_id, &provider)
        };
        if let Ok(mut slot) = self.runtime.lock() {
            *slot = Some(runtime);
        }
        result
    }

    fn evaluate_text(
        &self,
        callback_id: usize,
        state: &SkinDrawState,
        enabled_options: &[i32],
        text_values: &BTreeMap<i32, String>,
    ) -> Option<String> {
        let mut runtime = self.runtime.lock().ok().and_then(|mut slot| slot.take())?;
        let provider = RenderLuaMainState { state, enabled_options, text_values };
        let result = if self.state_is_bound(state, enabled_options, text_values) {
            runtime.evaluate_text_in_scope(callback_id)
        } else {
            runtime.evaluate_text(callback_id, &provider)
        };
        if let Ok(mut slot) = self.runtime.lock() {
            *slot = Some(runtime);
        }
        result
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkinUploadStats {
    pub upload_us: u64,
    pub source_count: usize,
    pub texture_cache_hits: usize,
    pub texture_cache_misses: usize,
    pub texture_cache_uncacheable: usize,
    pub texture_cache_disabled: usize,
    pub video_texture_cache_hits: usize,
    pub video_texture_cache_misses: usize,
    pub video_texture_cache_uncacheable: usize,
    pub video_texture_cache_disabled: usize,
    pub uploaded_source_count: usize,
    pub uploaded_source_bytes: usize,
    pub uploaded_video_source_count: usize,
    pub uploaded_video_source_bytes: usize,
}

/// `DecodedSkin` の全ソースを GPU へアップロードして `UploadedSkin` を返す。
/// upload worker スレッドから呼ぶ (`uploader` は `Renderer::gpu_uploader` の clone)。
pub fn upload_decoded_skin(uploader: &GpuUploader, decoded: DecodedSkin) -> UploadedSkin {
    upload_decoded_skin_with_texture_cache(uploader, decoded, None)
}

pub fn upload_decoded_skin_with_texture_cache(
    uploader: &GpuUploader,
    decoded: DecodedSkin,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> UploadedSkin {
    let upload_start = Instant::now();
    let DecodedSkin {
        kind,
        document,
        lua_runtime,
        fonts,
        sources,
        audio_assets,
        stats: decode_stats,
    } = decoded;
    let mut upload_stats = SkinUploadStats::default();
    let prepared = sources
        .into_iter()
        .filter_map(|source| {
            upload_stats.source_count += 1;
            let DecodedSource { source_id, path, texture, asset, size, cache_key, is_video, texture_lease } =
                source;
            let Some(asset) = asset else {
                upload_stats.texture_cache_hits += 1;
                if is_video {
                    upload_stats.video_texture_cache_hits += 1;
                }
                return Some(PreparedSource {
                    texture_lease,
                    source_id,
                    path,
                    texture,
                    prepared: None,
                    size,
                    is_video,
                    cache_key: None,
                });
            };
            if let Err(error) = asset.validate() {
                tracing::warn!(
                    source_id = %source_id,
                    path = %path.display(),
                    %error,
                    "skipping invalid beatoraja skin source"
                );
                return None;
            }
            match (texture_cache, cache_key.as_ref()) {
                (Some(texture_cache), Some(cache_key)) => {
                    if let Ok(mut cache) = texture_cache.lock()
                        && let Some(cached) = cache.get(cache_key)
                    {
                        upload_stats.texture_cache_hits += 1;
                        if is_video {
                            upload_stats.video_texture_cache_hits += 1;
                        }
                        return Some(PreparedSource {
                            texture_lease: Some(cached.lease),
                            source_id,
                            path,
                            texture: cached.texture,
                            prepared: None,
                            size: cached.size,
                            is_video,
                            cache_key: None,
                        });
                    }
                    upload_stats.texture_cache_misses += 1;
                    if is_video {
                        upload_stats.video_texture_cache_misses += 1;
                    }
                }
                (Some(_), None) => {
                    upload_stats.texture_cache_uncacheable += 1;
                    if is_video {
                        upload_stats.video_texture_cache_uncacheable += 1;
                    }
                }
                (None, _) => {
                    upload_stats.texture_cache_disabled += 1;
                    if is_video {
                        upload_stats.video_texture_cache_disabled += 1;
                    }
                }
            }
            let (texture, texture_lease) = texture_cache
                .and_then(|cache| {
                    cache.lock().ok().map(|mut cache| {
                        let texture = cache.allocate_texture_id(kind);
                        let lease = cache.lease_texture(texture);
                        (texture, Some(lease))
                    })
                })
                .unwrap_or((texture, None));
            let prepared = match uploader.upload(asset.width, asset.height, &asset.pixels) {
                Ok(prepared) => prepared,
                Err(error) => {
                    tracing::warn!(%source_id, path = %path.display(), %error, "skipping unsupported skin texture");
                    return None;
                }
            };
            upload_stats.uploaded_source_count += 1;
            upload_stats.uploaded_source_bytes =
                upload_stats.uploaded_source_bytes.saturating_add(asset.pixels.len());
            if is_video {
                upload_stats.uploaded_video_source_count += 1;
                upload_stats.uploaded_video_source_bytes =
                    upload_stats.uploaded_video_source_bytes.saturating_add(asset.pixels.len());
            }
            Some(PreparedSource {
                texture_lease,
                source_id,
                path,
                texture,
                prepared: Some(prepared),
                size,
                is_video,
                cache_key,
            })
        })
        .collect();
    upload_stats.upload_us = elapsed_us(upload_start);
    UploadedSkin {
        kind,
        document,
        lua_runtime,
        fonts,
        prepared,
        audio_assets,
        decode_stats,
        upload_stats,
    }
}

/// Phase A でデコードした成果物を Renderer に取り込み、scene context を更新する。
/// `default_manifest` は `load_default_skin_into_renderer` で取得した値を渡す。
/// 一括 install するので、PNG/フォント数が多いと 1 フレーム分のコストになる。
/// 起動直後や同期パスではこちらを使うが、ランタイム中はフレーム分散する方が望ましい。
pub fn install_decoded_skin(
    renderer: &mut Renderer,
    decoded: DecodedSkin,
    default_manifest: SkinManifest,
) -> Result<()> {
    let DecodedSkin { kind, document, lua_runtime, fonts, sources, audio_assets: _, stats: _ } =
        decoded;

    for font in fonts {
        install_decoded_font(renderer, font);
    }

    let document_textures: Vec<SkinDocumentTexture> =
        sources.into_iter().filter_map(|source| install_decoded_source(renderer, source)).collect();

    set_decoded_skin_context(
        renderer,
        kind,
        default_manifest,
        document,
        lua_runtime,
        document_textures,
        false,
    );
    Ok(())
}

/// 1 個のフォントを renderer に登録する。フレーム分散インストールから呼ばれる。
pub fn install_decoded_font(renderer: &mut Renderer, font: DecodedFont) -> bool {
    let DecodedFont { stored_id, path, data, cache_key: _ } = font;
    let Some(data) = data else {
        tracing::debug!(
            font_id = %stored_id,
            path = %path.display(),
            "skipping beatoraja skin font install because payload is already installed"
        );
        return false;
    };
    let result: Result<()> = match data {
        DecodedFontData::Vector(bytes) => renderer.install_font_bytes(stored_id.clone(), bytes),
        DecodedFontData::Bitmap(bitmap) => {
            renderer.install_bitmap_font(stored_id.clone(), bitmap);
            Ok(())
        }
    };
    let success = result.is_ok();
    match result {
        Ok(()) => tracing::info!(
            font_id = %stored_id,
            path = %path.display(),
            "loaded beatoraja skin font"
        ),
        Err(error) => tracing::warn!(
            font_id = %stored_id,
            path = %path.display(),
            %error,
            "failed to install beatoraja skin font"
        ),
    }
    success
}

/// 1 個の PNG ソースを renderer にアップロードし、対応する SkinDocumentTexture を返す。
/// アップロードに失敗した場合は None。
pub fn install_decoded_source(
    renderer: &mut Renderer,
    source: DecodedSource,
) -> Option<SkinDocumentTexture> {
    let DecodedSource {
        source_id,
        path,
        texture,
        asset,
        size,
        cache_key: _,
        is_video: _,
        texture_lease: _texture_lease,
    } = source;
    let Some(asset) = asset else {
        tracing::debug!(
            source_id = %source_id,
            texture_id = texture.0,
            path = %path.display(),
            "reusing beatoraja skin source texture"
        );
        return Some(SkinDocumentTexture { source_id, texture, source_size: size });
    };
    let source_size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
    if let Err(error) = renderer.upsert_image_asset(TextureId(texture.0), &asset) {
        tracing::warn!(
            source_id = %source_id,
            texture_id = texture.0,
            path = %path.display(),
            %error,
            "failed to upload beatoraja skin source"
        );
        return None;
    }
    tracing::info!(
        source_id = %source_id,
        texture_id = texture.0,
        path = %path.display(),
        "loaded beatoraja skin source"
    );
    Some(SkinDocumentTexture { source_id, texture, source_size })
}

/// 取り込み済みのフォント/PNG から SkinContext を組み立てて scene context にセットする。
///
/// プレイ中に `play7_files` などだけ変えた場合、`preserve_play_dynamic_timers` を true にすると
/// グルーヴ枠など `timer_observe_boolean` 由来のアニメ経過を維持できる。
pub fn set_decoded_skin_context(
    renderer: &mut Renderer,
    kind: SkinKind,
    default_manifest: SkinManifest,
    document: SkinDocument,
    lua_runtime: Option<LuaSkinRuntime>,
    document_textures: Vec<SkinDocumentTexture>,
    preserve_play_dynamic_timers: bool,
) {
    let mut context =
        SkinContext::from_manifest_and_document(default_manifest, document, document_textures);
    context.set_lua_draw_runtime(lua_runtime.map(|runtime| {
        Arc::new(LuaSkinDrawRuntimeAdapter::new(runtime)) as Arc<dyn SkinLuaDrawRuntime>
    }));
    match kind {
        SkinKind::Play => {
            renderer.set_play_skin_context(context, preserve_play_dynamic_timers);
        }
        SkinKind::Select => renderer.set_select_skin_context(context),
        SkinKind::Decide => renderer.set_decide_skin_context(context),
        SkinKind::Result => renderer.set_result_skin_context(context),
    }
}
