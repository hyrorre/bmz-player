use crate::skin_loader::*;

pub(in crate::skin_loader) fn skin_document_cache_key(
    path: &Path,
    kind: SkinKind,
    path_context: Option<&SkinPathContext>,
) -> Option<SkinDocumentCacheKey> {
    let metadata = fs::metadata(path).ok()?;
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some(SkinDocumentCacheKey {
        path,
        kind,
        library_roots: path_context
            .map_or_else(Vec::new, |context| context.library_roots().to_vec()),
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

/// beatoraja の設定ファイルを読む Lua スキン向けの、個人情報を含まない読取専用設定。
///
/// ホスト側の beatoraja 設定や BMZ の入力割当は公開せず、入力監視は BMZ のイベント処理を
/// 正とする。各 mode の空設定は WMII の設定探索を安全に完了させるためだけに供給する。
pub(in crate::skin_loader) fn lua_compat_virtual_io_files() -> BTreeMap<String, String> {
    const PLAYER_CONFIG: &str = concat!(
        "{",
        "\"mode5\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode7\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode9\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode10\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode14\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode24\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode24double\":{\"keyboard\":{},\"controller\":[],\"midi\":{}}",
        "}",
    );
    BTreeMap::from([
        ("config_sys.json".to_string(), "{\"playername\":\"bmz\"}".to_string()),
        ("player/bmz/config_player.json".to_string(), PLAYER_CONFIG.to_string()),
    ])
}

pub(in crate::skin_loader) fn lua_virtual_io_files(
    runtime_state: &LuaLoadRuntimeState,
) -> BTreeMap<String, String> {
    let mut files = lua_compat_virtual_io_files();
    files.extend(runtime_state.virtual_io_files.clone());
    files
}

pub(in crate::skin_loader) fn lr2_document_dependency_fingerprint(
    skin_path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    dependencies: &SkinLoadDependencies,
) -> Result<SkinDocumentDependencyFingerprint> {
    let option_values = bmz_skin::load_lr2_csv_skin_dependency_option_values(
        skin_path,
        options,
        dependencies.option_values.keys().copied(),
    )?;
    let file_values = dependencies
        .files
        .iter()
        .map(|name| (name.clone(), files.get(name).cloned().unwrap_or_default()))
        .collect();
    let loaded_files = current_loaded_file_dependencies(&dependencies.loaded_files)
        .context("failed to inspect lr2 skin loaded file dependencies")?;
    Ok(SkinDocumentDependencyFingerprint {
        lua_runtime_mode: bmz_skin::LuaSkinRuntimeMode::Auto,
        number_values: BTreeMap::new(),
        text_values: BTreeMap::new(),
        option_values,
        event_index_values: BTreeMap::new(),
        offset_values: BTreeMap::new(),
        offset_id_values: BTreeMap::new(),
        file_values,
        loaded_files,
        virtual_io_files: BTreeMap::new(),
    })
}

pub(in crate::skin_loader) fn document_dependency_fingerprint(
    document: &SkinDocument,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    dependencies: &SkinLoadDependencies,
) -> Option<SkinDocumentDependencyFingerprint> {
    let enabled_options = enabled_options_from_selections(document, options);
    let property_ops = document_property_ops(document);
    let number_values = dependencies
        .number_values
        .keys()
        .map(|ref_id| {
            let value = runtime_state.number_values.get(ref_id).copied().unwrap_or_default();
            (*ref_id, value)
        })
        .collect();
    let text_values = dependencies
        .text_values
        .keys()
        .map(|ref_id| {
            let value = runtime_state.text_values.get(ref_id).cloned().unwrap_or_default();
            (*ref_id, value)
        })
        .collect();
    let option_values = dependencies
        .option_values
        .keys()
        .map(|option_id| {
            let value = if property_ops.contains(option_id) {
                enabled_options.contains(option_id)
            } else {
                runtime_state.option_values.get(option_id).copied().unwrap_or(false)
            };
            (*option_id, value)
        })
        .collect();
    let event_index_values = dependencies
        .event_index_values
        .keys()
        .map(|event_id| {
            let value = runtime_state.event_index_values.get(event_id).copied().unwrap_or_default();
            (*event_id, value)
        })
        .collect();
    let file_values = dependencies
        .files
        .iter()
        .map(|name| (name.clone(), files.get(name).cloned().unwrap_or_default()))
        .collect();
    let loaded_files = current_loaded_file_dependencies(&dependencies.loaded_files).ok()?;
    let virtual_files = lua_virtual_io_files(runtime_state);
    let virtual_io_files = dependencies
        .virtual_io_files
        .keys()
        .map(|path| (path.clone(), virtual_files.get(path).cloned()))
        .collect();
    Some(SkinDocumentDependencyFingerprint {
        lua_runtime_mode: runtime_state.runtime_mode,
        number_values,
        text_values,
        option_values,
        event_index_values,
        offset_values: runtime_state.offset_values.clone(),
        offset_id_values: runtime_state.offset_id_values.clone(),
        file_values,
        loaded_files,
        virtual_io_files,
    })
}

pub(in crate::skin_loader) fn document_property_ops(document: &SkinDocument) -> HashSet<i32> {
    document.property.iter().flat_map(|property| property.item.iter().map(|item| item.op)).collect()
}

pub(in crate::skin_loader) fn current_loaded_file_dependencies(
    loaded_files: &BTreeMap<PathBuf, SkinLoadedFileDependency>,
) -> Result<BTreeMap<PathBuf, SkinLoadedFileDependency>> {
    let mut result = BTreeMap::new();
    for path in loaded_files.keys() {
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read loaded lua skin file: {}", path.display()))?;
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        result.insert(
            path,
            SkinLoadedFileDependency { modified: metadata.modified().ok(), len: metadata.len() },
        );
    }
    Ok(result)
}

pub(in crate::skin_loader) fn load_skin_video_first_frame_rgba(
    path: &Path,
) -> Result<RgbaImageAsset> {
    let frame = bmz_video::decode_first_frame(path)
        .with_context(|| format!("failed to decode first video frame: {}", path.display()))?;
    Ok(RgbaImageAsset { width: frame.width, height: frame.height, pixels: frame.rgba })
}

pub(in crate::skin_loader) struct LoadedSkinDocumentForDecode {
    pub(in crate::skin_loader) dependencies: SkinLoadDependencies,
    pub(in crate::skin_loader) document: SkinDocument,
    pub(in crate::skin_loader) lua_runtime: Option<LuaSkinRuntime>,
    pub(in crate::skin_loader) files: BTreeMap<String, String>,
    pub(in crate::skin_loader) cache_status: DocumentCacheStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::skin_loader) enum DocumentCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

#[cfg(test)]
pub(in crate::skin_loader) fn load_skin_document(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
) -> Result<LoadedSkinDocumentForDecode> {
    load_skin_document_with_path_context(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        document_cache,
        None,
    )
}

pub(in crate::skin_loader) fn load_skin_document_with_path_context(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
    path_context: Option<&SkinPathContext>,
) -> Result<LoadedSkinDocumentForDecode> {
    if is_lr2_skin_path(skin_path)
        && let Some(document_cache) = document_cache.as_ref()
        && let Some(key) = skin_document_cache_key(skin_path, kind, None)
    {
        if let Ok(mut cache) = document_cache.lock()
            && let Some((mut document, mut resolved_files)) =
                cache.get_lr2(&key, skin_path, options, files)
        {
            for (name, selected) in files {
                resolved_files.insert(name.clone(), selected.clone());
            }
            document.user_selected_options =
                Some(enabled_options_from_selections(&document, options));
            return Ok(LoadedSkinDocumentForDecode {
                dependencies: cache
                    .entries
                    .last()
                    .map(|entry| entry.dependencies.clone())
                    .unwrap_or_default(),
                document,
                lua_runtime: None,
                files: resolved_files,
                cache_status: DocumentCacheStatus::Hit,
            });
        }
        let mut loaded = load_skin_document_uncached_with_path_context(
            skin_path,
            kind,
            options,
            files,
            runtime_state,
            path_context,
        )?;
        loaded.cache_status = DocumentCacheStatus::Miss;
        if let Ok(mut cache) = document_cache.lock()
            && let Ok(fingerprint) =
                lr2_document_dependency_fingerprint(skin_path, options, files, &loaded.dependencies)
        {
            cache.insert_lr2(
                key,
                fingerprint,
                loaded.document.clone(),
                loaded.files.clone(),
                loaded.dependencies.clone(),
            );
        }
        return Ok(LoadedSkinDocumentForDecode {
            dependencies: loaded.dependencies,
            document: loaded.document,
            lua_runtime: loaded.lua_runtime,
            files: loaded.files,
            cache_status: loaded.cache_status,
        });
    }
    if is_lua_skin_path(skin_path)
        && let Some(document_cache) = document_cache.as_ref()
        && let Some(key) = skin_document_cache_key(skin_path, kind, path_context)
    {
        if let Ok(mut cache) = document_cache.lock()
            && let Some((mut document, mut resolved_files)) =
                cache.get_lua(&key, options, files, runtime_state)
        {
            for (name, selected) in files {
                resolved_files.insert(name.clone(), selected.clone());
            }
            document.user_selected_options =
                Some(enabled_options_from_selections(&document, options));
            return Ok(LoadedSkinDocumentForDecode {
                dependencies: cache
                    .entries
                    .last()
                    .map(|entry| entry.dependencies.clone())
                    .unwrap_or_default(),
                document,
                lua_runtime: None,
                files: resolved_files,
                cache_status: DocumentCacheStatus::Hit,
            });
        }
        let mut loaded = load_skin_document_uncached_with_path_context(
            skin_path,
            kind,
            options,
            files,
            runtime_state,
            path_context,
        )?;
        loaded.cache_status = DocumentCacheStatus::Miss;
        if let Ok(mut cache) = document_cache.lock()
            && let Some(fingerprint) = document_dependency_fingerprint(
                &loaded.document,
                options,
                files,
                runtime_state,
                &loaded.dependencies,
            )
        {
            cache.insert_lua(
                key,
                fingerprint,
                loaded.document.clone(),
                loaded.files.clone(),
                loaded.dependencies.clone(),
            );
        }
        return Ok(LoadedSkinDocumentForDecode {
            dependencies: loaded.dependencies,
            document: loaded.document,
            lua_runtime: loaded.lua_runtime,
            files: loaded.files,
            cache_status: loaded.cache_status,
        });
    }

    let cache_status = if document_cache.is_some() {
        DocumentCacheStatus::Uncacheable
    } else {
        DocumentCacheStatus::Disabled
    };
    let mut loaded = load_skin_document_uncached_with_path_context(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        path_context,
    )?;
    loaded.cache_status = cache_status;
    Ok(LoadedSkinDocumentForDecode {
        dependencies: loaded.dependencies,
        document: loaded.document,
        lua_runtime: loaded.lua_runtime,
        files: loaded.files,
        cache_status: loaded.cache_status,
    })
}

pub(in crate::skin_loader) struct LoadedSkinDocumentWithDependencies {
    pub(in crate::skin_loader) document: SkinDocument,
    pub(in crate::skin_loader) lua_runtime: Option<LuaSkinRuntime>,
    pub(in crate::skin_loader) files: BTreeMap<String, String>,
    pub(in crate::skin_loader) dependencies: SkinLoadDependencies,
    pub(in crate::skin_loader) cache_status: DocumentCacheStatus,
}

#[cfg(test)]
pub(in crate::skin_loader) fn load_skin_document_uncached(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<LoadedSkinDocumentWithDependencies> {
    load_skin_document_uncached_with_path_context(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        None,
    )
}

pub(in crate::skin_loader) fn load_skin_document_uncached_with_path_context(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    path_context: Option<&SkinPathContext>,
) -> Result<LoadedSkinDocumentWithDependencies> {
    let (mut document, lua_runtime, mut resolved_files, dependencies) =
        if is_lua_skin_path(skin_path) {
            // Lua スキンはオプション選択 (名前 -> 選択肢名) とファイル選択
            // (filepath 定義名 -> 相対パス) をそのまま渡す。
            let virtual_io_files = lua_virtual_io_files(runtime_state);
            let fallback_context;
            let path_context = match path_context {
                Some(path_context) => path_context,
                None => {
                    fallback_context = SkinPathContext::for_entry(skin_path)?;
                    &fallback_context
                }
            };
            let loaded = bmz_skin::load_lua_skin_with_path_context(
                path_context,
                options,
                files,
                runtime_state,
                &virtual_io_files,
            )
            .with_context(|| format!("failed to load lua skin: {}", skin_path.display()))?;
            for warning in loaded.warnings {
                tracing::warn!(
                    path = %skin_path.display(),
                    kind = ?kind,
                    warning = %warning.message,
                    "lua skin load warning"
                );
            }
            (loaded.document, loaded.lua_runtime, loaded.files, loaded.dependencies)
        } else if is_lr2_skin_path(skin_path) {
            let loaded =
                bmz_skin::load_lr2_csv_skin(skin_path, decode_skin_kind(kind), options, files)
                    .with_context(|| {
                        format!("failed to load lr2 csv skin: {}", skin_path.display())
                    })?;
            for warning in loaded.warnings {
                tracing::warn!(
                    path = %skin_path.display(),
                    kind = ?kind,
                    warning = %warning.message,
                    "lr2 csv skin load warning"
                );
            }
            (loaded.document, None, BTreeMap::new(), loaded.dependencies)
        } else {
            let document = bmz_skin::load_beatoraja_json_skin_with_defaults(skin_path)
                .with_context(|| {
                    format!("failed to load beatoraja json skin: {}", skin_path.display())
                })?;
            if options.is_empty() {
                (document, None, BTreeMap::new(), SkinLoadDependencies::default())
            } else {
                // JSON スキンは property 定義から選択肢の op コード列を組み立て、
                // それを有効オプションとして再デコードする。
                let enabled = enabled_options_from_selections(&document, options);
                let document = bmz_skin::load_beatoraja_json_skin(skin_path, &enabled)
                    .with_context(|| {
                        format!(
                            "failed to load beatoraja json skin with options: {}",
                            skin_path.display()
                        )
                    })?;
                (document, None, BTreeMap::new(), SkinLoadDependencies::default())
            }
        };
    for (name, selected) in files {
        resolved_files.insert(name.clone(), selected.clone());
    }
    // レンダー時の `enabled_options()` がユーザ選択を反映するように、
    // 選択値から算出した op コード列を document に格納する。
    // (選択が空でもデフォルト計算結果と同じになるため、常に設定して問題ない)
    document.user_selected_options = Some(enabled_options_from_selections(&document, options));
    Ok(LoadedSkinDocumentWithDependencies {
        document,
        lua_runtime,
        files: resolved_files,
        dependencies,
        cache_status: DocumentCacheStatus::Disabled,
    })
}

/// property 定義とユーザ選択 (オプション名 -> 選択肢名) から、JSON スキンの
/// 有効オプション (op コード列) を組み立てる。
///
/// 選択が無い property は `def` (空なら先頭 item) の op を使う。
pub(crate) fn enabled_options_from_selections(
    document: &SkinDocument,
    selections: &BTreeMap<String, String>,
) -> Vec<i32> {
    let options = document
        .property
        .iter()
        .filter_map(|property| {
            let selected = selected_property_item(property, selections)
                .or_else(|| default_property_item(property));
            selected.map(|item| item.op)
        })
        .collect();
    document.with_internal_enabled_options(options)
}

pub(in crate::skin_loader) fn selected_property_item<'a>(
    property: &'a bmz_render::skin::SkinPropertyDef,
    selections: &BTreeMap<String, String>,
) -> Option<&'a bmz_render::skin::SkinPropertyItemDef> {
    let value = selections.get(&property.name)?;
    if let Ok(op) = value.parse::<i32>() {
        return property.item.iter().find(|item| item.op == op);
    }
    property.item.iter().find(|item| &item.name == value)
}

pub(in crate::skin_loader) fn default_property_item(
    property: &bmz_render::skin::SkinPropertyDef,
) -> Option<&bmz_render::skin::SkinPropertyItemDef> {
    property
        .item
        .iter()
        .find(|item| !property.def.is_empty() && item.name == property.def)
        .or_else(|| property.item.first())
}

pub(in crate::skin_loader) fn decode_skin_kind(kind: SkinKind) -> DecodeSkinKind {
    match kind {
        SkinKind::Play => DecodeSkinKind::Play,
        SkinKind::Select => DecodeSkinKind::Select,
        SkinKind::Decide => DecodeSkinKind::Decide,
        SkinKind::Result => DecodeSkinKind::Result,
    }
}

pub fn is_decodable_skin_path(path: &Path) -> bool {
    is_json_skin_path(path) || is_lua_skin_path(path) || is_lr2_skin_path(path)
}

pub fn is_json_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

pub fn is_lua_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin") || ext.eq_ignore_ascii_case("lua"))
}

pub fn is_lr2_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
}
