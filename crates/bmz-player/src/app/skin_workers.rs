use super::*;

/// One immutable chart-property result per preload generation. The chart worker
/// publishes it before WAV loading; failed/cancelled work also releases waiters.
#[derive(Clone, Default)]
pub(super) struct PlaySkinChartState(Arc<PlaySkinChartStateInner>);

#[derive(Default)]
struct PlaySkinChartStateInner {
    length: std::sync::Mutex<Option<std::result::Result<i64, String>>>,
    ready: std::sync::Condvar,
}

impl PlaySkinChartState {
    pub(super) fn finish(&self, length: std::result::Result<i64, String>) {
        let mut value = self.0.length.lock().unwrap_or_else(|error| error.into_inner());
        if value.is_none() {
            *value = Some(length);
            self.0.ready.notify_all();
        }
    }

    pub(super) fn wait(&self) -> std::result::Result<i64, String> {
        let mut value = self.0.length.lock().unwrap_or_else(|error| error.into_inner());
        while value.is_none() {
            value = self.0.ready.wait(value).unwrap_or_else(|error| error.into_inner());
        }
        value.as_ref().unwrap().clone()
    }
}

#[derive(Clone)]
pub(super) struct SkinDecodeRequest {
    pub(super) pinned_sources: BTreeMap<String, String>,
    pub(super) generation: u64,
    pub(super) path: PathBuf,
    pub(super) kind: SkinKind,
    options: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
    pub(super) runtime_state: bmz_skin::LuaLoadRuntimeState,
    library_roots: Vec<PathBuf>,
    installed_font_cache: HashMap<String, SkinFontCacheKey>,
    /// Resolved on the worker and merged into `runtime_state.lr2_continue_assets`.
    lr2_continue_fallback: Option<Lr2ContinueFallback>,
    play_chart_state: Option<PlaySkinChartState>,
}

impl SkinDecodeRequest {
    pub(super) fn new(
        generation: u64,
        path: PathBuf,
        kind: SkinKind,
        options: BTreeMap<String, String>,
        files: BTreeMap<String, String>,
        runtime_state: bmz_skin::LuaLoadRuntimeState,
    ) -> Self {
        Self {
            pinned_sources: BTreeMap::new(),
            generation,
            path,
            kind,
            options,
            files,
            runtime_state,
            library_roots: Vec::new(),
            installed_font_cache: HashMap::new(),
            lr2_continue_fallback: None,
            play_chart_state: None,
        }
    }

    pub(super) fn with_lr2_continue_fallback(
        mut self,
        fallback: Option<Lr2ContinueFallback>,
    ) -> Self {
        self.lr2_continue_fallback = fallback;
        self
    }

    pub(super) fn with_library_roots(mut self, library_roots: Vec<PathBuf>) -> Self {
        self.library_roots = library_roots;
        self
    }

    pub(super) fn with_play_chart_state(mut self, state: Option<PlaySkinChartState>) -> Self {
        self.play_chart_state = state;
        self
    }

    pub(super) fn cancel_play_chart_wait(&self) {
        if let Some(state) = &self.play_chart_state {
            state.finish(Err("skin decode cancelled".into()));
        }
    }

    pub(super) fn reuse_installed_fonts(mut self, pipeline: &SkinPipelineRuntime) -> Self {
        self.installed_font_cache.clone_from(&pipeline.installed_font_cache);
        self
    }
}

pub(super) fn spawn_skin_decode(pipeline: &SkinPipelineRuntime, mut request: SkinDecodeRequest) {
    request.runtime_state.screen_size = pipeline.screen_size;
    request.runtime_state.file_store = Some(pipeline.lua_file_store.clone());
    pipeline.record_screen_request(request.clone());
    let SkinDecodeRequest {
        pinned_sources,
        generation,
        path,
        kind,
        options,
        files,
        mut runtime_state,
        library_roots,
        installed_font_cache,
        lr2_continue_fallback,
        play_chart_state,
    } = request;
    let tx = pipeline.decode_tx.clone();
    let source_cache = pipeline.source_asset_cache.clone();
    let document_cache = pipeline.document_cache.clone();
    let texture_cache = pipeline.gpu_texture_cache.clone();
    let font_cache = pipeline.font_cache.clone();
    let send_path = path.clone();
    let queued_at = Instant::now();
    let worker = thread::Builder::new()
        .name(format!("skin-decode-{:?}", kind))
        .spawn(move || {
            let chart_length = play_chart_state.map(|state| state.wait()).transpose();
            let decode_started_at = Instant::now();
            let result = (|| {
                if let Some(length) = chart_length.map_err(anyhow::Error::msg)? {
                    apply_chart_length_lua_load_state(&mut runtime_state, Some(length));
                }
                if let Some(fallback) = lr2_continue_fallback {
                    match crate::skin_loader::resolve_lr2_continue_assets(
                        &fallback.play_skin,
                        &fallback.runtime_state,
                        Some(document_cache.clone()),
                    ) {
                        Ok(assets) => {
                            runtime_state.lr2_continue_assets = merge_lr2_continue_assets(
                                assets,
                                &runtime_state.lr2_continue_assets,
                            )
                            .into();
                        }
                        Err(error) => tracing::warn!(
                            play_skin = %fallback.play_skin.display(),
                            error = %format_error_chain(&error),
                            "failed to resolve LR2 CONTINUE fallback assets"
                        ),
                    }
                }
                for provider in lr2_continue_theme_providers(&path, &library_roots) {
                    match crate::skin_loader::resolve_lr2_continue_assets(
                        &provider,
                        &bmz_skin::LuaLoadRuntimeState::default(),
                        Some(document_cache.clone()),
                    ) {
                        Ok(assets) => {
                            runtime_state.lr2_continue_assets = fill_empty_lr2_continue_assets(
                                assets,
                                &runtime_state.lr2_continue_assets,
                            )
                            .into();
                        }
                        Err(error) => tracing::warn!(
                            provider = %provider.display(),
                            error = %format_error_chain(&error),
                            "failed to resolve same-theme LR2 CONTINUE assets"
                        ),
                    }
                }
                decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
                    pinned_sources: Some(&pinned_sources),
                    skin_path: &path,
                    kind,
                    options: &options,
                    files: &files,
                    runtime_state: &runtime_state,
                    library_roots: &library_roots,
                    document_cache: Some(document_cache),
                    source_cache: Some(source_cache),
                    texture_cache: Some(texture_cache),
                    font_cache: Some(font_cache),
                    installed_fonts: Some(installed_font_cache),
                })
            })();
            let decode_finished_at = Instant::now();
            let _ = tx.send(PendingSkinResult {
                generation,
                path: send_path,
                kind,
                queued_at,
                decode_started_at,
                decode_finished_at,
                result,
            });
        })
        .expect("failed to spawn skin decode thread");
    pipeline.track_decode_worker(worker);
}

/// upload worker のループ。decode 結果を受け取り、GPU アップロードして main へ返す。
/// decode 側の sender が全て drop されるとループを抜ける (アプリ終了時)。
pub(super) fn skin_upload_worker(
    decode_rx: Receiver<PendingSkinResult>,
    upload_tx: mpsc::SyncSender<PendingUploadResult>,
    uploader: bmz_render::renderer::GpuUploader,
    texture_cache: SharedSkinGpuTextureCache,
    event_proxy: EventLoopProxy<AppUserEvent>,
) {
    while let Ok(PendingSkinResult {
        generation,
        path,
        kind,
        queued_at,
        decode_started_at,
        decode_finished_at,
        result,
    }) = decode_rx.recv()
    {
        let upload_started_at = Instant::now();
        let uploaded = result.map(|decoded| {
            upload_decoded_skin_with_texture_cache(&uploader, decoded, Some(&texture_cache))
        });
        let upload_finished_at = Instant::now();
        if upload_tx
            .send(PendingUploadResult {
                generation,
                path,
                kind,
                queued_at,
                decode_started_at,
                decode_finished_at,
                upload_started_at,
                upload_finished_at,
                uploaded,
            })
            .is_err()
        {
            // main 側受信端が drop された (アプリ終了)。
            break;
        }
        let _ = event_proxy.send_event(AppUserEvent::SkinUpload { sent_at: Instant::now() });
    }
}
