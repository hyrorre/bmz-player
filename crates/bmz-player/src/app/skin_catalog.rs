use super::*;

pub(super) fn scan_skin_catalog(app_paths: &crate::paths::AppPaths) -> SkinCatalog {
    let mut catalog = SkinCatalog::default();
    let resource_skin_root = app_paths.resource_dir.join("skins");
    let data_skin_root = app_paths.data_dir.join("skins");
    let library_roots = app_paths.skin_library_roots();
    scan_skin_catalog_dir(
        &resource_skin_root,
        &resource_skin_root,
        &library_roots,
        SkinCandidateOrigin::Bundled,
        &mut catalog,
    );
    if !same_path(&resource_skin_root, &data_skin_root) {
        scan_skin_catalog_dir(
            &data_skin_root,
            &data_skin_root,
            &library_roots,
            SkinCandidateOrigin::User,
            &mut catalog,
        );
    }
    sort_skin_catalog(&mut catalog);
    catalog
}

pub(super) fn scan_skin_catalog_dir(
    root: &Path,
    dir: &Path,
    library_roots: &[PathBuf],
    origin: SkinCandidateOrigin,
    catalog: &mut SkinCatalog,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_skin_catalog_dir(root, &path, library_roots, origin, catalog);
            continue;
        }
        if !is_skin_candidate_file(&path) {
            continue;
        }
        let started_at = Instant::now();
        let candidate = load_skin_candidate_with_library_roots(root, &path, library_roots, origin);
        tracing::debug!(
            target: "bmz_player::startup_profile",
            path = %path.display(),
            header_us = started_at.elapsed().as_micros(),
            "startup skin catalog candidate"
        );
        match candidate {
            Some((skin_type, candidate)) => push_skin_candidate(catalog, skin_type, candidate),
            None => {
                tracing::debug!(path = %path.display(), "skipping skin candidate without readable header")
            }
        }
    }
}

pub(super) fn skin_defs_from_path(
    app_paths: &crate::paths::AppPaths,
    path: &str,
    play: bool,
) -> SceneSkinDefs {
    let trimmed = path.trim();
    let document = (!trimmed.is_empty())
        .then(|| app_paths.resolve_path_ref(trimmed).ok())
        .flatten()
        .and_then(|path| {
            load_skin_header_document_with_library_roots(&path, &app_paths.skin_library_roots())
        });
    if play {
        SceneSkinDefs::from_play_document(document.as_ref())
    } else {
        SceneSkinDefs::from_document(document.as_ref())
    }
}

pub(super) fn is_skin_candidate_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "json" | "luaskin" | "lr2skin"))
        .unwrap_or(false)
}

#[cfg(test)]
pub(super) fn load_skin_header_document(path: &Path) -> Option<SkinDocument> {
    load_skin_header_document_with_library_roots(path, &[])
}

pub(super) fn load_skin_header_document_with_library_roots(
    path: &Path,
    library_roots: &[PathBuf],
) -> Option<SkinDocument> {
    read_skin_header_document(path, library_roots).ok()
}

pub(super) fn read_skin_header_document(
    path: &Path,
    library_roots: &[PathBuf],
) -> Result<SkinDocument> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin") || ext.eq_ignore_ascii_case("lua"))
    {
        let path_context = bmz_skin::SkinPathContext::new(path, library_roots.iter().cloned())?;
        let loaded = bmz_skin::load_lua_skin_header_value_with_path_context(&path_context)?;
        Ok(serde_json::from_value::<SkinDocument>(loaded.value)?)
    } else if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
    {
        bmz_skin::load_lr2_csv_skin(
            path,
            bmz_skin::SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .map(|loaded| loaded.document)
    } else {
        SkinDocument::load_typed_beatoraja_json(path)?
            .ok_or_else(|| anyhow::anyhow!("unsupported skin document: {}", path.display()))
    }
}

#[cfg(test)]
pub(super) fn load_skin_candidate(
    root: &Path,
    path: &Path,
    origin: SkinCandidateOrigin,
) -> Option<(i32, SkinCandidate)> {
    load_skin_candidate_with_library_roots(root, path, &[root.to_path_buf()], origin)
}

pub(super) fn load_skin_candidate_with_library_roots(
    root: &Path,
    path: &Path,
    library_roots: &[PathBuf],
    origin: SkinCandidateOrigin,
) -> Option<(i32, SkinCandidate)> {
    let document = load_skin_header_document_with_library_roots(path, library_roots)?;
    let relative = path.strip_prefix(root).unwrap_or(path);
    let name = if document.name.trim().is_empty() {
        relative.file_stem().and_then(|name| name.to_str()).unwrap_or("").to_string()
    } else {
        document.name
    };
    let stable_path = match origin {
        SkinCandidateOrigin::Bundled => format!("resource:skins/{}", path_to_slash(relative)),
        SkinCandidateOrigin::User => format!("data:skins/{}", path_to_slash(relative)),
        SkinCandidateOrigin::External => path.to_string_lossy().replace('\\', "/"),
    };
    Some((document.skin_type, SkinCandidate { name, path: stable_path, origin }))
}

pub(super) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub(super) fn path_to_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) const BMZ_SKIN_TYPE_PLAY_2KEYS: i32 = 21;
pub(super) const BMZ_SKIN_TYPE_PLAY_4KEYS: i32 = 22;
pub(super) const BMZ_SKIN_TYPE_PLAY_6KEYS: i32 = 23;
pub(super) const BMZ_SKIN_TYPE_PLAY_8KEYS: i32 = 24;

pub(super) fn push_skin_candidate(
    catalog: &mut SkinCatalog,
    skin_type: i32,
    candidate: SkinCandidate,
) {
    match skin_type {
        0 => catalog.play7.push(candidate),
        1 => catalog.play5.push(candidate),
        2 => catalog.play14.push(candidate),
        3 => catalog.play10.push(candidate),
        4 => catalog.play9.push(candidate),
        12 => catalog.battle7.push(candidate),
        13 => catalog.battle5.push(candidate),
        5 => catalog.select.push(candidate),
        6 => catalog.decide.push(candidate),
        7 => catalog.result.push(candidate),
        15 => catalog.course_result.push(candidate),
        BMZ_SKIN_TYPE_PLAY_4KEYS => catalog.play4.push(candidate),
        BMZ_SKIN_TYPE_PLAY_6KEYS => catalog.play6.push(candidate),
        BMZ_SKIN_TYPE_PLAY_8KEYS => catalog.play8.push(candidate),
        BMZ_SKIN_TYPE_PLAY_2KEYS => {}
        _ => {}
    }
}

pub(super) fn sort_skin_catalog(catalog: &mut SkinCatalog) {
    for candidates in [
        &mut catalog.select,
        &mut catalog.decide,
        &mut catalog.play4,
        &mut catalog.play5,
        &mut catalog.play6,
        &mut catalog.play7,
        &mut catalog.play8,
        &mut catalog.play9,
        &mut catalog.play10,
        &mut catalog.play14,
        &mut catalog.battle5,
        &mut catalog.battle7,
        &mut catalog.result,
        &mut catalog.course_result,
    ] {
        candidates.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
        candidates.dedup_by(|a, b| a.path == b.path);
    }
}
