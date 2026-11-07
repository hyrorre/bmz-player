use super::*;

pub(super) fn prepare_pm_chara(
    document: &mut SkinDocument,
    context: &SkinPathContext,
    resolved_files: &BTreeMap<String, String>,
) {
    let mut expanded_sources = Vec::new();
    for index in 0..document.pmchara.len() {
        let (id, source_id, chara_type, color) = {
            let definition = &document.pmchara[index];
            (definition.id.clone(), definition.src.clone(), definition.chara_type, definition.color)
        };
        let Some(source_path) = document
            .source
            .iter()
            .find(|source| source.id == source_id)
            .map(|source| source.path.clone())
        else {
            tracing::warn!(pmchara_id = %id, source_id = %source_id, "PMchara source id is missing");
            continue;
        };
        let Some(resolved) =
            resolve_pm_chara_source_path(context, &source_path, document, resolved_files)
        else {
            tracing::warn!(pmchara_id = %id, source = %source_path, "PMchara source path is missing");
            continue;
        };
        let prefix = format!("__bmz_pmchara:{index}:{id}");
        match bmz_skin::load_pm_chara_with_context(&resolved, context, &prefix, chara_type, color) {
            Ok(loaded) => {
                document.pmchara[index].runtime = Some(loaded.runtime);
                expanded_sources.extend(loaded.sources);
            }
            Err(error) => {
                tracing::warn!(
                    pmchara_id = %id,
                    path = %resolved.display(),
                    %error,
                    "failed to expand PMchara source"
                );
            }
        }
    }
    document.source.extend(expanded_sources);
}

fn resolve_pm_chara_source_path(
    context: &SkinPathContext,
    source_path: &str,
    document: &SkinDocument,
    resolved_files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let normalized = normalize_pm_chara_path(source_path);
    if !normalized.contains('*') {
        return context.resolve_path_with_ascii_case(&normalized)
            .inspect_err(|error| tracing::debug!(path = %normalized, %error, "PMchara path rejected or unresolved"))
            .ok();
    }

    let filepath = document
        .filepath
        .iter()
        .find(|filepath| normalize_pm_chara_path(&filepath.path) == normalized);
    let selected = filepath
        .and_then(|filepath| resolved_files.get(&filepath.name))
        .filter(|selected| !selected.is_empty() && selected.as_str() != RANDOM_FILE_SELECTION)
        .map(String::as_str)
        .or_else(|| {
            filepath
                .filter(|filepath| !filepath.def.is_empty() && filepath.def != "Random")
                .map(|filepath| filepath.def.as_str())
        });
    if let Some(selected) = selected
        && let Some(path) = resolve_pm_chara_selection(context, &normalized, selected)
    {
        return Some(path);
    }

    let candidates = context.wildcard_candidates_with_ascii_case(&normalized).ok()?;
    if let Some(selected) = selected
        && let Some(path) = candidates.iter().find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(selected))
        })
    {
        return Some(path.clone());
    }
    candidates.into_iter().next()
}

fn normalize_pm_chara_path(path: &str) -> String {
    path.split_once('|').map_or(path, |(asset, _)| asset).replace('\\', "/")
}

fn resolve_pm_chara_selection(
    context: &SkinPathContext,
    pattern: &str,
    selected: &str,
) -> Option<PathBuf> {
    let selected = selected.replace('\\', "/");
    if !selected.contains('/')
        && !Path::new(&selected).is_absolute()
        && let Some((prefix, suffix)) = pattern.split_once('*')
    {
        let directory = prefix.rfind('/').map_or("", |slash| &prefix[..=slash]);
        let nested_suffix = if suffix.starts_with('/') { suffix } else { "" };
        let relative = format!("{directory}{selected}{nested_suffix}");
        if let Ok(path) = context.resolve_path_with_ascii_case(&relative) {
            return Some(path);
        }
    }
    context.resolve_path_with_ascii_case(&selected).ok()
}
