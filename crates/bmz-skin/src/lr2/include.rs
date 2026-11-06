use super::*;

pub(super) enum ResolvedInclude {
    File(PathBuf),
    Missing(PathBuf),
}

pub(super) fn resolve(
    builder: &mut CsvBuilder<'_>,
    current_path: &Path,
    raw: &str,
) -> Result<ResolvedInclude> {
    let cleaned = clean_lr2_asset_path(raw);
    let normalized = normalize_lr2_asset_path(raw);
    // CUSTOMFILE is authoritative before random wildcard selection.
    let selected = if normalized.contains('*') {
        builder.resolve_source_path(raw)
    } else {
        normalized.clone()
    };
    if let Some(relative) = cleaned.strip_prefix("LR2files/Theme/")
        && Path::new(relative).components().all(|c| matches!(c, std::path::Component::Normal(_)))
        && let Some(themes) = builder.skin_root.parent()
    {
        // Keep an explicit sibling theme. For our own theme, first apply the
        // CUSTOMFILE selection, which is relative to the entry directory.
        if selected == normalized
            && let Some(path) = candidate(builder, themes.join(relative))
        {
            return Ok(ResolvedInclude::File(path));
        }
    }
    for base in [
        builder.skin_root.clone(),
        builder.skin_file_dir.clone(),
        current_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
    ] {
        if let Some(path) = candidate(builder, base.join(&selected)) {
            return Ok(ResolvedInclude::File(path));
        }
    }
    let missing = current_path.parent().unwrap_or_else(|| Path::new(".")).join(selected);
    let policy = builder.path_context.policy();
    if let Some(name) = missing.file_name()
        && name.to_string_lossy().contains('*')
    {
        // Windows rejects '*' in metadata queries. Validate the parent chain
        // and keep the pattern only for diagnostics. A literal '*' file on Unix
        // must not be opened after candidate validation has rejected it.
        let parent =
            policy.resolve_or_missing(missing.parent().unwrap_or_else(|| Path::new(".")))?;
        return Ok(ResolvedInclude::Missing(parent.join(name)));
    }
    policy.resolve_or_missing(&missing).map(ResolvedInclude::Missing)
}

fn candidate(builder: &mut CsvBuilder<'_>, pattern: PathBuf) -> Option<PathBuf> {
    let name = pattern.file_name()?.to_str()?;
    let Some((prefix, suffix)) = name.split_once('*') else {
        return builder.path_context.policy().resolve(&pattern).ok().filter(|path| path.is_file());
    };
    if suffix.contains('*') {
        return None;
    }
    if let Some(path) = builder.include_choices.get(&pattern) {
        return Some(path.clone());
    }
    let prefix = prefix.to_ascii_lowercase();
    let suffix = suffix.to_ascii_lowercase();
    let directory = builder.path_context.policy().resolve(pattern.parent()?).ok()?;
    let mut matches = fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(&prefix)
                && name.ends_with(&suffix)
        })
        .filter_map(|entry| builder.path_context.policy().resolve(&entry.path()).ok())
        .collect::<Vec<_>>();
    matches.sort();
    if matches.is_empty() {
        return None;
    }
    let index = match crate::random_file_index(matches.len()) {
        Ok(index) => index,
        Err(error) => {
            builder.warn(format!("lr2 include randomness: {error}"));
            return None;
        }
    };
    let path = matches.swap_remove(index);
    builder.include_choices.insert(pattern, path.clone());
    Some(path)
}
