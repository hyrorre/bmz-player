use super::*;

pub(super) fn is_supported_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "otf" | "ttc" | "fnt" | "lr2font")
    )
}

pub(super) fn is_bitmap_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("fnt" | "lr2font")
    )
}

#[cfg(test)]
pub(super) fn resolve_json_skin_source_path(
    skin_root: &Path,
    source_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path(skin_root, source_path, document, files)
}

pub(super) fn resolve_json_skin_source_path_with_context(
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    source_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path_with_context(skin_root, path_context, source_path, document, files)
}

#[cfg(test)]
pub(super) fn resolve_json_skin_asset_path(
    skin_root: &Path,
    asset_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path_with_context(skin_root, None, asset_path, document, files)
}

pub(super) fn resolve_json_skin_asset_path_with_context(
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    asset_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let normalized = asset_path.replace('\\', "/");
    if !normalized.contains('*') {
        if let Some(path_context) = path_context {
            return path_context.resolve_asset(&normalized).ok();
        }
        return Some(resolve_case_insensitive_path(&skin_root.join(normalized)));
    }

    let filepath =
        document.filepath.iter().find(|filepath| filepath.path.replace('\\', "/") == normalized);

    // 0. ユーザが明示的に「ランダム」を選んだときは、def を無視して候補から
    //    ランダムに選ぶ (beatoraja のファイル選択 "Random" 相当)。
    if let Some(filepath) = filepath
        && files.get(&filepath.name).is_some_and(|selected| selected == RANDOM_FILE_SELECTION)
    {
        return resolve_wildcard_path_with_context(skin_root, path_context, &normalized, None);
    }

    // 1. パスが filepath 定義と完全一致するときは、選択ファイルをそのまま使う。
    if let Some(filepath) = filepath
        && let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        && let Some(path) = resolve_selected_skin_file_for_pattern_with_context(
            skin_root,
            path_context,
            &filepath.path,
            selected,
        )
    {
        return Some(path);
    }

    // 2. 完全一致しなくても、filepath 定義の `*` が asset_path の `*` と同じ
    //    位置に来るなら、選択値からワイルドカード部分を抽出して埋め込む
    //    (例: 定義 `custom/laser/*` で選択 `custom/laser/veryshort` のとき、
    //         ソース `custom/laser/*/main.png` を `custom/laser/veryshort/main.png` へ)。
    if let Some(substituted) = substitute_filepath_choice(&normalized, &document.filepath, files) {
        let candidate = if let Some(path_context) = path_context {
            path_context.resolve_asset(&substituted).ok()
        } else {
            Some(resolve_case_insensitive_path(&skin_root.join(&substituted)))
        };
        // A default such as "default" may identify the stem of default.png.
        // If direct substitution misses, retain the sandboxed wildcard fallback.
        if let Some(candidate) = candidate.filter(|candidate| bmz_skin_assets::is_file(candidate)) {
            return Some(candidate);
        }
    }

    // `def` が空、または beatoraja のランダム指定 ("Random") のときは具体的な
    // 優先ファイルを持たず、候補からランダムに選ぶ。
    let preferred = filepath.and_then(|filepath| {
        (!filepath.def.is_empty() && filepath.def != "Random").then_some(filepath.def.as_str())
    });
    resolve_wildcard_path_with_context(skin_root, path_context, &normalized, preferred)
}

/// filepath 定義のワイルドカードと一致するユーザ選択値を `asset_path` の
/// ワイルドカード位置に埋め込んだ相対パスを返す。
///
/// 一致条件: `asset_path` と filepath 定義の `path` が `*` 直前の prefix を
/// 共有していること。選択値からも同じ prefix（および suffix）を剥がして
/// ワイルドカード相当の文字列を取り出し、`asset_path` の `*` を置換する。
pub(super) fn substitute_filepath_choice(
    asset_path: &str,
    filepaths: &[SkinFilepathDef],
    files: &BTreeMap<String, String>,
) -> Option<String> {
    let (asset_before, asset_after) = asset_path.split_once('*')?;
    for filepath in filepaths {
        let def_path = filepath.path.replace('\\', "/");
        let Some((def_prefix, def_suffix)) = def_path.split_once('*') else {
            continue;
        };
        if def_prefix != asset_before {
            continue;
        }
        let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        else {
            continue;
        };
        let selected = selected.replace('\\', "/");
        let wildcard_value = selected
            .strip_prefix(def_prefix)
            .and_then(|stripped| stripped.strip_suffix(def_suffix).or(Some(stripped)))
            .or_else(|| {
                selected
                    .strip_prefix(def_prefix.rsplit('/').next().unwrap_or_default())
                    .and_then(|stripped| stripped.strip_suffix(def_suffix).or(Some(stripped)))
            })?;
        return Some(format!("{asset_before}{wildcard_value}{asset_after}"));
    }
    None
}

/// ユーザ選択のスキンルート相対パスを解決する。
/// 絶対パスやスキンルート外への脱出を含む選択は無効として `None` を返す。
pub(super) fn resolve_selected_skin_file(skin_root: &Path, selected: &str) -> Option<PathBuf> {
    use std::path::Component;

    let relative = Path::new(selected);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    let candidate = resolve_case_insensitive_path(&skin_root.join(relative));
    candidate.is_file().then_some(candidate)
}

pub(super) fn resolve_selected_skin_file_for_pattern_with_context(
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    pattern: &str,
    selected: &str,
) -> Option<PathBuf> {
    if let Some(path_context) = path_context {
        return path_context
            .resolve_selected_for_pattern(pattern, selected)
            .filter(|path| path.is_file());
    }
    if let Some(path) = resolve_selected_skin_file(skin_root, selected) {
        return Some(path);
    }
    let pattern = strip_beatoraja_asset_filter(pattern).replace('\\', "/");
    let star = pattern.find('*')?;
    let prefix = &pattern[..star];
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let directory = &prefix[..slash];
    resolve_selected_skin_file(skin_root, &format!("{directory}{}", selected.replace('\\', "/")))
}

pub(super) fn resolve_wildcard_path_with_context(
    skin_root: &Path,
    path_context: Option<&SkinPathContext>,
    pattern: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path_context) = path_context {
        let candidates = path_context
            .wildcard_candidates(pattern)
            .ok()?
            .into_iter()
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        if let Some(preferred) = preferred
            && let Some(candidate) = candidates.iter().find(|path| {
                let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
                let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default();
                let parent_name = path
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                file_name.eq_ignore_ascii_case(preferred)
                    || stem.eq_ignore_ascii_case(preferred)
                    || parent_name.eq_ignore_ascii_case(preferred)
            })
        {
            return Some(candidate.clone());
        }
        return choose_wildcard_candidate(candidates);
    }
    let pattern = strip_beatoraja_asset_filter(pattern);
    let star = pattern.find('*')?;
    let (prefix, suffix_with_star) = pattern.split_at(star);
    let suffix = &suffix_with_star[1..];
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let (directory, filename_prefix) = prefix.split_at(slash);
    let directory = skin_root.join(directory);

    if let Some(suffix) = suffix.strip_prefix('/') {
        return resolve_wildcard_directory_path(&directory, filename_prefix, suffix, preferred);
    }

    let candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            starts_with_ignore_ascii_case(file_name, filename_prefix)
                && ends_with_ignore_ascii_case(file_name, suffix)
        })
        .collect::<Vec<_>>();
    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default();
            file_name.eq_ignore_ascii_case(preferred) || stem.eq_ignore_ascii_case(preferred)
        })
    {
        return Some(candidate.clone());
    }

    choose_wildcard_candidate(candidates)
}

pub(super) fn resolve_case_insensitive_path(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let parent = resolve_case_insensitive_path(parent);
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return parent.join(file_name);
    };
    entries
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(file_name))
        })
        .map(|entry| entry.path())
        .unwrap_or_else(|| parent.join(file_name))
}

pub(super) fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

pub(super) fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

pub(super) fn strip_beatoraja_asset_filter(pattern: &str) -> &str {
    pattern.split_once('|').map_or(pattern, |(path, _)| path)
}

/// beatoraja のファイル選択カスタマイズで「ランダム」を表す番兵値。
/// `def == "Random"` や、設定パネルでユーザが明示的にランダムを選んだとき、
/// `files` マップにこの文字列が入る。具体ファイル名と衝突しないよう beatoraja
/// 同様 "Random" を用いる。
pub(crate) const RANDOM_FILE_SELECTION: &str = "Random";

/// ワイルドカードのマッチ候補から 1 つを選ぶ。
///
/// beatoraja の `SkinLoader.getPath` は、ユーザ選択 (filemap) が無いワイルドカードを
/// ロードごとに `Math.random()` でランダム解決する。`def == "Random"` の filepath も
/// 同様にランダムへ展開される (`SkinHeader.setSkinConfigProperty`)。これに合わせ、
/// `preferred` (具体的な def 値 / ユーザ選択) が候補に無いときはランダムに選ぶ。
pub(super) fn choose_wildcard_candidate(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    if candidates.len() <= 1 {
        return candidates.into_iter().next();
    }
    let index = random_wildcard_index(candidates.len());
    candidates.into_iter().nth(index)
}

/// ロードごとにOS乱数で抽選する。取得失敗時は警告して先頭候補を使う。
pub(super) fn random_wildcard_index(len: usize) -> usize {
    debug_assert!(len > 0);
    bmz_skin::random_file_index(len).unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to select random skin file; using first candidate");
        0
    })
}

pub(super) fn required_skin_source_ids(document: &SkinDocument) -> HashSet<&str> {
    let destination_ids = destination_ids(document);
    let image_sources = document
        .image
        .iter()
        .map(|image| (image.id.as_str(), image.src.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut required = HashSet::new();

    for image in &document.image {
        if destination_ids.contains(image.id.as_str()) {
            required.insert(image.src.as_str());
        }
    }
    for imageset in &document.imageset {
        if destination_ids.contains(imageset.id.as_str()) {
            for image_id in &imageset.images {
                if let Some(source_id) = image_sources.get(image_id.as_str()) {
                    required.insert(*source_id);
                }
            }
        }
    }
    for value in &document.value {
        if destination_ids.contains(value.id.as_str()) {
            required.insert(value.src.as_str());
        }
    }
    for slider in &document.slider {
        if destination_ids.contains(slider.id.as_str()) {
            required.insert(slider.src.as_str());
        }
    }
    for graph in &document.graph {
        if destination_ids.contains(graph.id.as_str()) {
            required.insert(graph.src.as_str());
        }
    }
    for cover in document.hidden_cover.iter().chain(&document.lift_cover) {
        if destination_ids.contains(cover.id.as_str()) {
            required.insert(cover.src.as_str());
        }
    }
    if let Some(note) = &document.note {
        for image_id in note
            .note
            .iter()
            .chain(note.lnstart.iter())
            .chain(note.lnend.iter())
            .chain(note.lnbody.iter())
            .chain(note.lnactive.iter())
            .chain(note.hcnstart.iter())
            .chain(note.hcnend.iter())
            .chain(note.hcnbody.iter())
            .chain(note.hcnactive.iter())
            .chain(note.hcndamage.iter())
            .chain(note.hcnreactive.iter())
            .chain(note.mine.iter())
            .chain(note.hidden.iter())
            .chain(note.processed.iter())
        {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    if let Some(gauge) = &document.gauge {
        for image_id in &gauge.nodes {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    for gauge in &document.gauges {
        for image_id in &gauge.nodes {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    for judge in &document.judge {
        for destination in judge.images.iter().chain(judge.numbers.iter()) {
            if let Some(source_id) = image_sources.get(destination.id.as_str()) {
                required.insert(*source_id);
            }
            if let Some(value) = document.value.iter().find(|value| value.id == destination.id) {
                required.insert(value.src.as_str());
            }
        }
    }
    for pmchara in &document.pmchara {
        if !destination_ids.contains(pmchara.id.as_str()) {
            continue;
        }
        if let Some(runtime) = &pmchara.runtime {
            for layer in &runtime.motions {
                required.insert(layer.source_id.as_str());
            }
        }
    }

    required
}

pub(super) fn destination_ids(document: &SkinDocument) -> HashSet<&str> {
    let mut ids = HashSet::new();
    for entry in &document.destination {
        match entry {
            DestinationListEntry::Single(destination) => {
                ids.insert(destination.id.as_str());
            }
            DestinationListEntry::Conditional { destinations, .. } => {
                for destination in destinations {
                    ids.insert(destination.id.as_str());
                }
            }
        }
    }
    ids
}

pub(super) fn resolve_wildcard_directory_path(
    directory: &Path,
    directory_prefix: &str,
    suffix: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(directory_prefix))
        })
        .map(|path| path.join(suffix))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == preferred)
        })
    {
        return Some(candidate.clone());
    }

    choose_wildcard_candidate(candidates)
}
