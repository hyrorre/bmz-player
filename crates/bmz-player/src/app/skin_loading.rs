use super::*;

/// 起動時のスキンロード処理。
///
/// - default skin は必ず一度だけ renderer にインストールする。
/// - select skin はwindow/surface初期化と並行してdecodeし、初回描画前にinstallする。
/// - decide / result skin はバックグラウンドスレッドで Phase A (decode) を実行。
///   完了したものは main thread の `try_recv` で順次 Phase B (install) する。
/// - 空パスは同梱スキンを使い、非対応のパスは警告ログのみ。
/// - プレイスキンは決定画面でチャートの key_mode から個別に decode するためここでは扱わない。
pub(super) fn load_initial_skin_textures(
    renderer: &mut Renderer,
    app_paths: &crate::paths::AppPaths,
    pipeline: &SkinPipelineRuntime,
    generation: u64,
    frontend: FrontendLuaLoadInputs<'_>,
    skin: &SkinConfig,
    load_frontend_skins: bool,
    lua_runtime_mode: bmz_skin::LuaSkinRuntimeMode,
    experimental_detail_options: bool,
) -> (Option<SkinManifest>, HashMap<SkinKind, Vec<ActiveSkinVideoSource>>, bool, bool, bool) {
    tracing::info!("startup initial skin texture load begin");
    // Frontend payload decoding overlaps window/surface initialization. The
    // initial Select is installed before the first frame in ensure_window.
    let mut pending_select = false;
    let mut pending_decide = false;
    let mut pending_result = false;
    let skin_video_sources = HashMap::new();

    let decide_trimmed = skin.decide.trim().to_string();
    let result_trimmed = skin.result.trim().to_string();

    if load_frontend_skins {
        let decide_path = if decide_trimmed.is_empty() {
            default_skin_document_path_from_paths(app_paths, SkinKind::Decide)
        } else {
            match app_paths.resolve_path_ref(&decide_trimmed) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        path = %decide_trimmed,
                        error = %format_error_chain(&error),
                        "failed to resolve decide skin path; ignoring"
                    );
                    PathBuf::new()
                }
            }
        };
        if !decide_path.as_os_str().is_empty() && is_decodable_skin_path(&decide_path) {
            let mut runtime_state = frontend.lua_runtime_state(None);
            apply_decide_chart_lua_load_state(&mut runtime_state, &RenderSnapshot::default());
            spawn_skin_decode(
                pipeline,
                SkinDecodeRequest::new(
                    generation,
                    decide_path,
                    SkinKind::Decide,
                    if decide_trimmed.is_empty() {
                        BTreeMap::new()
                    } else {
                        skin.decide_options.clone()
                    },
                    if decide_trimmed.is_empty() {
                        BTreeMap::new()
                    } else {
                        skin.decide_files.clone()
                    },
                    lua_runtime_state_with_mode(
                        lua_runtime_state_with_skin_offsets(runtime_state, &skin.decide_offsets),
                        lua_runtime_mode,
                    ),
                )
                .with_skin_access(app_paths, skin),
            );
            pending_decide = true;
        }
    }
    if load_frontend_skins {
        let result_path = if result_trimmed.is_empty() {
            default_skin_document_path_from_paths(app_paths, SkinKind::Result)
        } else {
            match app_paths.resolve_path_ref(&result_trimmed) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        path = %result_trimmed,
                        error = %format_error_chain(&error),
                        "failed to resolve result skin path; ignoring"
                    );
                    PathBuf::new()
                }
            }
        };
        if !result_path.as_os_str().is_empty() && is_decodable_skin_path(&result_path) {
            spawn_skin_decode(
                pipeline,
                SkinDecodeRequest::new(
                    generation,
                    result_path,
                    SkinKind::Result,
                    if result_trimmed.is_empty() {
                        BTreeMap::new()
                    } else {
                        skin.result_options.clone()
                    },
                    if result_trimmed.is_empty() {
                        BTreeMap::new()
                    } else {
                        skin.result_files.clone()
                    },
                    lua_runtime_state_with_mode(
                        lua_runtime_state_with_skin_offsets(
                            lua_runtime_state_for_result(
                                false,
                                frontend.ir_name,
                                false,
                                false,
                                KeyMode::default(),
                                BTreeMap::new(),
                                frontend.player_name,
                            ),
                            &skin.result_offsets,
                        ),
                        lua_runtime_mode,
                    ),
                )
                .with_skin_access(app_paths, skin),
            );
            pending_result = true;
        }
    }

    let default_skin_started_at = Instant::now();
    tracing::info!("startup default skin manifest load begin");
    let default_manifest = match load_default_skin_into_renderer_from_paths(renderer, app_paths) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            tracing::warn!(
                error = %format_error_chain(&error),
                "failed to load default skin; using fallback drawing"
            );
            None
        }
    };
    tracing::info!(
        load_ms = default_skin_started_at.elapsed().as_millis(),
        loaded = default_manifest.is_some(),
        "startup default skin manifest load complete"
    );

    // Select payload decoding overlaps window creation and surface attachment.
    let select_trimmed = skin.select.trim();
    if load_frontend_skins {
        let select_path = if select_trimmed.is_empty() {
            Ok(default_skin_document_path_from_paths(app_paths, SkinKind::Select))
        } else {
            app_paths.resolve_path_ref(select_trimmed)
        };
        let empty_options = BTreeMap::new();
        let empty_files = BTreeMap::new();
        let active_select_options =
            if select_trimmed.is_empty() { &empty_options } else { &skin.select_options };
        let active_select_options =
            select_skin_options(active_select_options, experimental_detail_options);
        let active_select_files =
            if select_trimmed.is_empty() { &empty_files } else { &skin.select_files };
        match select_path {
            Ok(path) if is_decodable_skin_path(&path) => {
                tracing::info!(
                    kind = ?SkinKind::Select,
                    path = %path.display(),
                    "startup asynchronous select skin load begin"
                );
                spawn_skin_decode(
                    pipeline,
                    SkinDecodeRequest::new(
                        generation,
                        path,
                        SkinKind::Select,
                        active_select_options,
                        active_select_files.clone(),
                        lua_runtime_state_with_mode(
                            lua_runtime_state_with_skin_offsets(
                                frontend.lua_runtime_state(None),
                                &skin.select_offsets,
                            ),
                            lua_runtime_mode,
                        ),
                    )
                    .with_skin_access(app_paths, skin),
                );
                pending_select = true;
            }
            Ok(path) => {
                tracing::warn!(
                    path = %path.display(),
                    "select skin path is not a supported beatoraja skin file; ignoring"
                );
            }
            Err(error) => {
                tracing::warn!(
                    path = %select_trimmed,
                    error = %format_error_chain(&error),
                    "failed to resolve select skin path; ignoring"
                );
            }
        }
    }

    if load_frontend_skins && !result_trimmed.is_empty() {
        match app_paths.resolve_path_ref(&result_trimmed) {
            Ok(path) if !is_decodable_skin_path(&path) => {
                tracing::warn!(
                    path = %path.display(),
                    "result skin path is not a supported beatoraja skin file; ignoring"
                );
            }
            _ => {}
        }
    }

    if load_frontend_skins && !decide_trimmed.is_empty() {
        match app_paths.resolve_path_ref(&decide_trimmed) {
            Ok(path) if !is_decodable_skin_path(&path) => {
                tracing::warn!(
                    path = %path.display(),
                    "decide skin path is not a supported beatoraja skin file; ignoring"
                );
            }
            _ => {}
        }
    }

    (default_manifest, skin_video_sources, pending_select, pending_decide, pending_result)
}

pub(super) fn reload_skin_textures(
    app_paths: &crate::paths::AppPaths,
    pipeline: &mut SkinPipelineRuntime,
    request: SkinReloadRequest,
    frontend: FrontendLuaLoadInputs<'_>,
    skin: &SkinConfig,
    lua_runtime_mode: bmz_skin::LuaSkinRuntimeMode,
    experimental_detail_options: bool,
    select_last_played_at: Option<i64>,
    frontend_chart: Option<&RenderSnapshot>,
) -> (bool, bool, bool) {
    let mut pending_select = false;
    let mut pending_decide = false;
    let mut pending_result = false;

    for (enabled, kind) in [
        (request.select, SkinKind::Select),
        (request.decide, SkinKind::Decide),
        (request.result, SkinKind::Result),
    ] {
        if !enabled {
            continue;
        }
        let generation = pipeline.generations.bump(kind);
        let Some(load) = frontend_skin_load(
            app_paths,
            kind,
            frontend,
            skin,
            lua_runtime_mode,
            experimental_detail_options,
            select_last_played_at,
            frontend_chart,
        ) else {
            continue;
        };
        spawn_frontend_skin_decode(app_paths, pipeline, skin, generation, kind, load);
        match kind {
            SkinKind::Select => pending_select = true,
            SkinKind::Decide => pending_decide = true,
            SkinKind::Result => pending_result = true,
            SkinKind::Play => unreachable!("play skin handled via spawn_play_skin_decode_for"),
        }
    }

    (pending_select, pending_decide, pending_result)
}

/// The path, options and load-time values a Select / Decide / startup Result decode
/// is requested with. Equal loads read the same values, so one decoded ahead of a
/// scene entry stands in for the entry's own load.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FrontendSkinLoad {
    pub(super) path: PathBuf,
    pub(super) options: BTreeMap<String, String>,
    pub(super) files: BTreeMap<String, String>,
    pub(super) runtime_state: bmz_skin::LuaLoadRuntimeState,
}

pub(super) fn frontend_skin_load(
    app_paths: &crate::paths::AppPaths,
    kind: SkinKind,
    frontend: FrontendLuaLoadInputs<'_>,
    skin: &SkinConfig,
    lua_runtime_mode: bmz_skin::LuaSkinRuntimeMode,
    experimental_detail_options: bool,
    select_last_played_at: Option<i64>,
    frontend_chart: Option<&RenderSnapshot>,
) -> Option<FrontendSkinLoad> {
    let (path_text, options, files, offsets) = match kind {
        SkinKind::Select => (
            skin.select.as_str(),
            &skin.select_options,
            &skin.select_files,
            skin.select_offsets.as_slice(),
        ),
        SkinKind::Decide => (
            skin.decide.as_str(),
            &skin.decide_options,
            &skin.decide_files,
            skin.decide_offsets.as_slice(),
        ),
        SkinKind::Result => (
            skin.result.as_str(),
            &skin.result_options,
            &skin.result_files,
            skin.result_offsets.as_slice(),
        ),
        SkinKind::Play => unreachable!("play skin handled via spawn_play_skin_decode_for"),
    };
    let trimmed = path_text.trim();
    let path = if trimmed.is_empty() {
        default_skin_document_path_from_paths(app_paths, kind)
    } else {
        match app_paths.resolve_path_ref(trimmed) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(
                    path = %trimmed,
                    kind = ?kind,
                    error = %format_error_chain(&error),
                    "failed to resolve skin path; ignoring"
                );
                return None;
            }
        }
    };
    if !is_decodable_skin_path(&path) {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            "skin path is not a supported beatoraja skin file; ignoring"
        );
        return None;
    }
    let skin_options = if trimmed.is_empty() { BTreeMap::new() } else { options.clone() };
    let mut runtime_state = frontend.lua_runtime_state(if kind == SkinKind::Select {
        select_last_played_at
    } else {
        None
    });
    if kind == SkinKind::Select {
        apply_chart_hashes_lua_load_state(
            &mut runtime_state,
            frontend_chart.and_then(|chart| chart.chart_md5),
            frontend_chart.and_then(|chart| chart.chart_sha256),
        );
    }
    if kind == SkinKind::Decide {
        let no_chart = RenderSnapshot::default();
        apply_decide_chart_lua_load_state(&mut runtime_state, frontend_chart.unwrap_or(&no_chart));
    }
    Some(FrontendSkinLoad {
        path,
        options: if kind == SkinKind::Select {
            select_skin_options(&skin_options, experimental_detail_options)
        } else {
            skin_options
        },
        files: if trimmed.is_empty() { BTreeMap::new() } else { files.clone() },
        runtime_state: lua_runtime_state_with_mode(
            lua_runtime_state_with_skin_offsets(runtime_state, offsets),
            lua_runtime_mode,
        ),
    })
}

pub(super) fn spawn_frontend_skin_decode(
    app_paths: &crate::paths::AppPaths,
    pipeline: &SkinPipelineRuntime,
    skin: &SkinConfig,
    generation: u64,
    kind: SkinKind,
    load: FrontendSkinLoad,
) {
    spawn_skin_decode(
        pipeline,
        SkinDecodeRequest::new(
            generation,
            load.path,
            kind,
            load.options,
            load.files,
            load.runtime_state,
        )
        .with_skin_access(app_paths, skin)
        .reuse_installed_fonts(pipeline),
    );
}

/// Profile values that Select / Decide skins read while loading.
#[derive(Clone, Copy)]
pub(super) struct FrontendLuaLoadInputs<'a> {
    pub(super) player_name: &'a str,
    pub(super) ir_name: Option<&'a str>,
    pub(super) player_stats: &'a PlayerStatsSnapshot,
}

impl FrontendLuaLoadInputs<'_> {
    pub(super) fn lua_runtime_state(
        self,
        last_played_at: Option<i64>,
    ) -> bmz_skin::LuaLoadRuntimeState {
        let mut runtime_state = lua_runtime_state_for_frontend_last_play(
            self.player_name,
            self.ir_name,
            last_played_at,
        );
        apply_player_stats_lua_load_state(&mut runtime_state, self.player_stats);
        runtime_state
    }
}

pub(super) fn select_skin_options(
    options: &BTreeMap<String, String>,
    enabled: bool,
) -> BTreeMap<String, String> {
    let mut options = options.clone();
    // Owned by the application, never persisted as a skin customization.
    options.insert("bmz_detail_options".into(), if enabled { "1" } else { "0" }.into());
    options
}

#[cfg(test)]
pub(super) fn apply_json_skin_sync(
    renderer: &mut Renderer,
    pipeline: &SkinPipelineRuntime,
    app_paths: &crate::paths::AppPaths,
    path: &Path,
    kind: SkinKind,
    default_manifest: Option<&SkinManifest>,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &bmz_skin::LuaLoadRuntimeState,
) -> Vec<ActiveSkinVideoSource> {
    let Some(manifest) = default_manifest else {
        pipeline.record_load_result(path, Some("default skin manifest is unavailable".into()));
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            "skipping skin install because default skin manifest is unavailable"
        );
        return Vec::new();
    };
    let library_roots = app_paths.skin_library_roots();
    let mut runtime_state = runtime_state.clone();
    runtime_state.screen_size = renderer.render_target_size();
    runtime_state.file_store = Some(pipeline.lua_file_store.clone());
    pipeline.record_screen_request(
        SkinDecodeRequest::new(
            pipeline.generation(kind),
            path.to_path_buf(),
            kind,
            options.clone(),
            files.clone(),
            runtime_state.clone(),
        )
        .with_library_roots(library_roots.clone()),
    );
    let decode_started_at = Instant::now();
    tracing::info!(
        ?kind,
        path = %path.display(),
        "synchronous skin decode begin"
    );
    let decoded = match decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: path,
        kind,
        options,
        files,
        runtime_state: &runtime_state,
        library_roots: &library_roots,
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    }) {
        Ok(decoded) => decoded,
        Err(error) => {
            pipeline.record_load_result(path, Some(format!("{error:#}")));
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                error = %format_error_chain(&error),
                "failed to decode beatoraja skin"
            );
            return Vec::new();
        }
    };
    tracing::info!(
        ?kind,
        path = %path.display(),
        decode_ms = decode_started_at.elapsed().as_millis(),
        document_us = decoded.stats.document_us,
        font_decode_us = decoded.stats.font_decode_us,
        source_decode_us = decoded.stats.source_decode_us,
        source_count = decoded.stats.decoded_source_count,
        "startup synchronous skin decode complete"
    );
    let video_sources = skin_video_sources_from_decoded(&decoded);
    let load_dependencies = decoded.load_dependencies.clone();
    let install_started_at = Instant::now();
    tracing::info!(
        ?kind,
        path = %path.display(),
        "synchronous skin install begin"
    );
    if let Err(error) = install_decoded_skin(renderer, decoded, manifest.clone()) {
        pipeline.record_load_result(path, Some(format!("{error:#}")));
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            error = %format_error_chain(&error),
            "failed to install beatoraja skin"
        );
        return Vec::new();
    }
    tracing::info!(
        ?kind,
        install_ms = install_started_at.elapsed().as_millis(),
        "startup synchronous skin install complete"
    );
    pipeline.record_load_result(path, None);
    pipeline.record_screen_dependencies(kind, load_dependencies);
    video_sources
}

#[cfg(test)]
#[path = "skin_loading/startup_tests.rs"]
mod startup_tests;
