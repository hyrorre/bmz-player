use super::*;

pub(super) fn prepare_pm_chara(
    document: &mut SkinDocument,
    skin_root: &Path,
    resolved_files: &BTreeMap<String, String>,
    path_context: Option<&SkinPathContext>,
) {
    let Some(context) = path_context else { return };
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
        let Some(resolved) = resolve_pm_chara_source_path(
            skin_root,
            path_context,
            &source_path,
            document,
            resolved_files,
        ) else {
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
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    source_path: &str,
    document: &SkinDocument,
    resolved_files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let normalized = source_path.replace('\\', "/");
    let _ = skin_root;
    let path_context = path_context?;
    if !normalized.contains('*') {
        return path_context.resolve_path(&normalized)
            .inspect_err(|error| tracing::debug!(path = %normalized, %error, "PMchara path rejected or unresolved"))
            .ok();
    }

    let filepath =
        document.filepath.iter().find(|filepath| filepath.path.replace('\\', "/") == normalized);
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
        && let Some(path) = path_context.resolve_selected_for_pattern(&normalized, selected)
    {
        return Some(path);
    }

    let candidates = path_context.wildcard_candidates(&normalized).ok()?;
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
