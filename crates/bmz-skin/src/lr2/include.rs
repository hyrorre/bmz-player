use super::*;

pub(super) fn resolve(builder: &mut CsvBuilder<'_>, current_path: &Path, raw: &str) -> PathBuf {
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
            return path;
        }
    }
    for base in [
        builder.skin_root.clone(),
        builder.skin_file_dir.clone(),
        current_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
    ] {
        if let Some(path) = candidate(builder, base.join(&selected)) {
            return path;
        }
    }
    current_path.parent().unwrap_or_else(|| Path::new(".")).join(selected)
}

fn candidate(builder: &mut CsvBuilder<'_>, pattern: PathBuf) -> Option<PathBuf> {
    let name = pattern.file_name()?.to_str()?;
    let Some((prefix, suffix)) = name.split_once('*') else {
        return pattern.is_file().then_some(pattern);
    };
    if suffix.contains('*') {
        return None;
    }
    if let Some(path) = builder.include_choices.get(&pattern) {
        return Some(path.clone());
    }
    let prefix = prefix.to_ascii_lowercase();
    let suffix = suffix.to_ascii_lowercase();
    let mut matches = fs::read_dir(pattern.parent()?)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(&prefix)
                && name.ends_with(&suffix)
        })
        .map(|entry| entry.path())
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
