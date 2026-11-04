pub(super) fn skin_file_candidates(
    path_context: &SkinPathContext,
    normalized_path: &str,
) -> Vec<PathBuf> {
    path_context.wildcard_candidates(normalized_path).unwrap_or_default()
}

pub(super) fn filename_matches_def(candidate: &str, default_name: &str) -> bool {
    let file_name = Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.eq_ignore_ascii_case(default_name) {
        return true;
    }
    let stem = Path::new(file_name).file_stem().and_then(|stem| stem.to_str()).unwrap_or(file_name);
    if stem.eq_ignore_ascii_case(default_name) {
        return true;
    }
    filepath_def_acronym(default_name).is_some_and(|acronym| {
        let stem_lower = stem.to_ascii_lowercase();
        let acronym_lower = acronym.to_ascii_lowercase();
        stem_lower == acronym_lower || stem_lower.starts_with(&acronym_lower)
    })
}

pub(super) fn filepath_def_acronym(default_name: &str) -> Option<String> {
    if !default_name.contains('-') {
        return None;
    }
    let acronym = default_name
        .split('-')
        .filter_map(|part| part.chars().find(|ch| ch.is_ascii_alphanumeric()))
        .collect::<String>();
    (!acronym.is_empty()).then_some(acronym)
}

pub(super) fn candidate_file_name(candidate: &str) -> String {
    Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or(candidate).to_string()
}

pub(super) fn skin_config_get_path(
    path_context: &SkinPathContext,
    requested: &str,
    skin_files: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let requested_path = strip_beatoraja_asset_filter(requested);

    // ユーザがスキン設定パネルで「ランダム」を選んだときは、候補からロードごとに
    // ランダムに選ぶ (beatoraja のファイル選択 "Random" 相当)。
    let want_random = skin_config_path_uses_random_selection(requested, skin_files);

    // ユーザがスキン設定パネルで選んだファイルを最優先で返す。
    // 選択が存在しない / ファイルが消えている場合は従来通り候補解決へ委ねる。
    if !want_random {
        if let Some(selected) = skin_files.get(&requested.replace('\\', "/"))
            && let Some(path) =
                resolve_selected_skin_path_for_pattern(path_context, requested_path, selected)
        {
            return Ok(path);
        }
        if let Some(path) =
            resolve_selected_skin_path_for_wildcard_child(path_context, requested_path, skin_files)
        {
            return Ok(path);
        }
    }

    if !requested_path.contains('*') {
        // beatoraja returns a sandbox-rooted path even before a history/config
        // file is created. Actual reads still go through existing-path checks.
        return path_context.resolve_path_or_missing(requested_path);
    }
    if requested_path.split_once('*').is_some_and(|(_, suffix)| suffix.contains('*')) {
        bail!("skin_config.get_path supports only one wildcard: {requested}");
    }
    let mut candidates = path_context.wildcard_candidates(requested_path)?;
    if candidates.is_empty() {
        bail!("skin_config path not found: {requested}");
    }
    let index = if want_random { random_skin_file_index(candidates.len()) } else { 0 };
    Ok(candidates.swap_remove(index))
}

pub(super) fn skin_config_path_uses_random_selection(
    requested: &str,
    skin_files: &BTreeMap<String, String>,
) -> bool {
    skin_files
        .get(&requested.replace('\\', "/"))
        .is_some_and(|selected| selected == RANDOM_FILE_SELECTION)
}

pub(super) fn resolve_selected_skin_path_for_wildcard_child(
    path_context: &SkinPathContext,
    requested: &str,
    skin_files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let (requested_prefix, requested_suffix) = requested.split_once('*')?;
    for (configured, selected) in skin_files {
        let (configured_prefix, configured_suffix) = configured.split_once('*')?;
        if requested_prefix != configured_prefix {
            continue;
        }
        let wildcard = wildcard_from_selection(configured_prefix, configured_suffix, selected)?;
        let candidate = format!("{requested_prefix}{wildcard}{requested_suffix}");
        if let Ok(path) = path_context.resolve_path(&candidate) {
            return Some(path);
        }
    }
    None
}

pub(super) fn resolve_selected_skin_path_for_pattern(
    path_context: &SkinPathContext,
    pattern: &str,
    selected: &str,
) -> Option<PathBuf> {
    path_context.resolve_selected_for_pattern(pattern, selected)
}

pub(super) fn wildcard_from_selection<'a>(
    configured_prefix: &str,
    configured_suffix: &str,
    selected: &'a str,
) -> Option<&'a str> {
    selected
        .strip_prefix(configured_prefix)
        .and_then(|rest| rest.strip_suffix(configured_suffix).or(Some(rest)))
        .or_else(|| {
            let name_prefix = configured_prefix.rsplit(['/', '\\']).next().unwrap_or_default();
            selected
                .strip_prefix(name_prefix)
                .and_then(|rest| rest.strip_suffix(configured_suffix).or(Some(rest)))
        })
}

pub(super) fn strip_beatoraja_asset_filter(path: &str) -> &str {
    path.split_once('|').map_or(path, |(asset_path, _)| asset_path)
}

pub(super) fn resolve_skin_io_path(root: &Path, requested: &str) -> Result<PathBuf> {
    let relative = normalize_skin_io_relative_path(requested)?;

    if let Some(path) = resolve_beatoraja_skin_alias(root, &relative) {
        return Ok(path);
    }

    let path = root.join(&relative);
    let canonical = canonicalize_skin_path(&path)?;
    if !canonical.starts_with(root) {
        bail!("io path escapes skin root: {}", canonical.display());
    }
    Ok(canonical)
}

pub(super) fn normalize_skin_io_relative_path(requested: &str) -> Result<String> {
    if requested.contains('\0') {
        bail!("io path contains NUL");
    }
    let relative = requested.replace('\\', "/");
    if relative.starts_with('/') || relative.starts_with("//") {
        bail!("io path escapes skin root: {requested}");
    }
    let mut normalized = Vec::new();
    for (index, component) in relative.split('/').enumerate() {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".."
            || (index == 0
                && component.as_bytes().get(1) == Some(&b':')
                && component.as_bytes().first().is_some_and(u8::is_ascii_alphabetic))
        {
            bail!("io path escapes skin root: {requested}");
        }
        normalized.push(component);
    }
    if normalized.is_empty() {
        bail!("io path is empty");
    }
    Ok(normalized.join("/"))
}

pub(super) fn normalize_virtual_io_files(
    files: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut normalized = BTreeMap::new();
    for (path, source) in files {
        let path = normalize_skin_io_relative_path(path)
            .with_context(|| format!("invalid Lua virtual IO path: {path}"))?;
        if source.len() > LUA_IO_MAX_READ_BYTES {
            bail!("Lua virtual IO file exceeds {} byte limit: {path}", LUA_IO_MAX_READ_BYTES);
        }
        if normalized.insert(path.clone(), source.clone()).is_some() {
            bail!("duplicate normalized Lua virtual IO path: {path}");
        }
    }
    Ok(normalized)
}

pub(super) fn read_skin_io_source(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > LUA_IO_MAX_READ_BYTES as u64 {
        bail!("Lua IO file exceeds {} byte limit: {}", LUA_IO_MAX_READ_BYTES, path.display());
    }
    let source = fs::read(path)?;
    if source.len() > LUA_IO_MAX_READ_BYTES {
        bail!("Lua IO file exceeds {} byte limit: {}", LUA_IO_MAX_READ_BYTES, path.display());
    }
    Ok(source)
}

pub(super) fn record_virtual_io_dependency(
    path: &str,
    source: Option<&str>,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
) {
    if let Some(dependencies) = dependencies
        && let Ok(mut dependencies) = dependencies.lock()
    {
        dependencies.virtual_io_files.insert(path.to_string(), source.map(str::to_string));
    }
}

pub(super) fn mark_load_dependency_opaque(dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>) {
    if let Some(dependencies) = dependencies
        && let Ok(mut dependencies) = dependencies.lock()
    {
        // A missing real file cannot be represented by loaded_files metadata.
        // Avoid caching a branch that could change merely because the file is
        // created after this load.
        dependencies.opaque = true;
    }
}

pub(super) fn resolve_beatoraja_skin_alias(root: &Path, relative: &str) -> Option<PathBuf> {
    let rest = relative.strip_prefix("skin/")?;
    let (skin_name, skin_relative) = rest.split_once('/')?;
    if let Some(canonical) = canonicalize_skin_child(root, skin_relative) {
        return Some(canonical);
    }
    for ancestor in root.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) != Some(skin_name) {
            continue;
        }
        if let Some(canonical) = canonicalize_skin_child(ancestor, skin_relative) {
            return Some(canonical);
        }
    }
    None
}

pub(super) fn canonicalize_skin_child(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = root.join(relative);
    if !path.is_file() {
        return None;
    }
    let Ok(root) = canonicalize_skin_path(root) else {
        return None;
    };
    let Ok(canonical) = canonicalize_skin_path(&path) else {
        return None;
    };
    canonical.starts_with(&root).then_some(canonical)
}

pub(super) fn is_unsupported_json_field_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Function(_)
            | Value::Thread(_)
            | Value::UserData(_)
            | Value::LightUserData(_)
            | Value::Error(_)
            | Value::Other(_)
    )
}

/// beatoraja Lua skin loader が document/header に残すコールバック。
/// BMZ は `.luaskin` 実行結果だけを使い、関数参照自体は JSON 化しない。
pub(super) const SILENTLY_SKIPPED_LOADER_FIELDS: &[&str] =
    &["process", "main", "processHeader", "act"];

pub(super) fn should_silently_skip_loader_field(key: &str, value: &Value) -> bool {
    matches!(value, Value::Function(_)) && SILENTLY_SKIPPED_LOADER_FIELDS.contains(&key)
}

pub(super) fn lua_key_to_json_key(
    key: Value,
    path: &str,
    warnings: &mut Vec<String>,
) -> Result<String> {
    match key {
        Value::String(value) => Ok(value.to_string_lossy()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => {
            warnings.push(format!("unsupported table key converted with debug fallback at {path}"));
            Ok(lua_value_to_log_string(&key))
        }
    }
}
use super::*;
