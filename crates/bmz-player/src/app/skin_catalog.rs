use super::*;
use rayon::prelude::*;

/// The skin list, scanned off the window thread. Startup does not wait for it; the settings
/// UI and LR2 skin switching join the scan when they first need the list.
pub(super) struct SkinCatalogState {
    ready: OnceLock<SkinCatalog>,
    worker: std::sync::Mutex<Option<thread::JoinHandle<(SkinCatalog, u128)>>>,
}

impl SkinCatalogState {
    pub(super) fn ready(catalog: SkinCatalog) -> Self {
        let ready = OnceLock::new();
        let _ = ready.set(catalog);
        Self { ready, worker: std::sync::Mutex::new(None) }
    }

    pub(super) fn scan(app_paths: crate::paths::AppPaths) -> Result<Self> {
        let worker = thread::Builder::new()
            .name("skin-catalog".into())
            .spawn(move || {
                let started_at = Instant::now();
                let catalog = scan_skin_catalog(&app_paths);
                let catalog_ms = started_at.elapsed().as_millis();
                tracing::info!(catalog_ms, "skin catalog scanned");
                (catalog, catalog_ms)
            })
            .context("failed to spawn skin catalog worker")?;
        Ok(Self { ready: OnceLock::new(), worker: std::sync::Mutex::new(Some(worker)) })
    }

    pub(super) fn get(&self) -> &SkinCatalog {
        self.ready.get_or_init(|| {
            let started_at = Instant::now();
            let worker = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
            let (catalog, catalog_ms) = match worker.map(thread::JoinHandle::join) {
                Some(Ok(scanned)) => scanned,
                Some(Err(_)) => {
                    tracing::error!("skin catalog worker panicked");
                    (SkinCatalog::default(), 0)
                }
                None => (SkinCatalog::default(), 0),
            };
            tracing::info!(
                catalog_ms,
                wait_ms = started_at.elapsed().as_millis(),
                "skin catalog ready"
            );
            catalog
        })
    }
}

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
    let mut paths = Vec::new();
    collect_skin_candidate_files(
        dir,
        &mut paths,
        &mut crate::directory_scan::DirectoryScan::default(),
    );
    // Headers are independent; Lua headers run their own VM. Collecting in directory order
    // keeps the result identical to a sequential scan before sorting.
    let candidates: Vec<_> = paths
        .par_iter()
        .map(|path| {
            let started_at = Instant::now();
            let candidate =
                load_skin_candidate_with_library_roots(root, path, library_roots, origin);
            tracing::debug!(
                target: "bmz_player::startup_profile",
                path = %path.display(),
                header_us = started_at.elapsed().as_micros(),
                "startup skin catalog candidate"
            );
            if candidate.is_none() {
                tracing::debug!(path = %path.display(), "skipping skin candidate without readable header");
            }
            candidate
        })
        .collect();
    for (skin_type, candidate) in candidates.into_iter().flatten() {
        push_skin_candidate(catalog, skin_type, candidate);
    }
}

fn collect_skin_candidate_files(
    dir: &Path,
    paths: &mut Vec<PathBuf>,
    scan: &mut crate::directory_scan::DirectoryScan,
) {
    let Some(entries) = scan.read_dir_once(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skin_candidate_files(&path, paths, scan);
        } else if is_skin_candidate_file(&path) {
            paths.push(path);
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
    // A list only needs the type and name. LR2 skins report them in `#INFORMATION`, so
    // building their whole document (~30 ms each) is skipped.
    let (skin_type, name) = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
    {
        let info = bmz_skin::load_lr2_csv_skin_info(path).ok()?;
        (info.skin_type, info.name)
    } else {
        let document = load_skin_header_document_with_library_roots(path, library_roots)?;
        (document.skin_type, document.name)
    };
    let relative = path.strip_prefix(root).unwrap_or(path);
    let name = if name.trim().is_empty() {
        relative.file_stem().and_then(|name| name.to_str()).unwrap_or("").to_string()
    } else {
        name
    };
    let stable_path = match origin {
        SkinCandidateOrigin::Bundled => format!("resource:skins/{}", path_to_slash(relative)),
        SkinCandidateOrigin::User => format!("data:skins/{}", path_to_slash(relative)),
        SkinCandidateOrigin::External => path.to_string_lossy().replace('\\', "/"),
    };
    Some((skin_type, SkinCandidate { name, path: stable_path, origin }))
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

/// LR2 Play skin candidates in fallback order: the played key mode first, then
/// 7K and the other LR2 Play types. Lua / JSON skins cannot hand over CONTINUE assets.
pub(super) fn lr2_play_skin_candidates(catalog: &SkinCatalog, key_mode: KeyMode) -> Vec<&str> {
    let preferred = match key_mode {
        KeyMode::K5 => &catalog.play5,
        KeyMode::K9 => &catalog.play9,
        KeyMode::K10 => &catalog.play10,
        KeyMode::K14 => &catalog.play14,
        _ => &catalog.play7,
    };
    let mut paths = Vec::new();
    for candidates in [
        preferred,
        &catalog.play7,
        &catalog.play5,
        &catalog.play14,
        &catalog.play10,
        &catalog.play9,
        &catalog.battle7,
        &catalog.battle5,
    ] {
        let mut group: Vec<&str> = candidates
            .iter()
            .map(|candidate| candidate.path.as_str())
            .filter(|path| is_lr2_skin_path(Path::new(path)))
            .filter(|path| !paths.contains(path))
            .collect();
        group.sort_unstable();
        paths.extend(group);
    }
    paths
}

/// The theme folder of a skin: the first folder under the skin library root.
pub(super) fn skin_theme_dir(path: &Path, library_roots: &[PathBuf]) -> Option<PathBuf> {
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let path = absolute(path);
    library_roots.iter().find_map(|root| {
        let root = absolute(root);
        let theme = path.strip_prefix(&root).ok()?.components().next()?;
        // A skin file directly under the root has no theme folder.
        path.strip_prefix(root.join(theme)).ok().filter(|rest| !rest.as_os_str().is_empty())?;
        Some(root.join(theme))
    })
}

const LR2_PLAY_SKIN_TYPES: &[i32] = &[0, 1, 2, 3, 4, 12, 13];

/// LR2 skin types whose images stay in LR2's shared slots before a scene of
/// `skin_type`, nearest scene first (Select, Decide, Play, Result, Course Result).
fn lr2_continue_provider_types(skin_type: i32) -> &'static [&'static [i32]] {
    const SELECT: &[i32] = &[5];
    const DECIDE: &[i32] = &[6];
    const RESULT: &[i32] = &[7];
    const COURSE_RESULT: &[i32] = &[15];
    match skin_type {
        6 => &[SELECT, RESULT, LR2_PLAY_SKIN_TYPES],
        0..=4 | 12 | 13 => &[DECIDE, SELECT, RESULT, COURSE_RESULT],
        7 => &[DECIDE, SELECT],
        15 => &[RESULT, LR2_PLAY_SKIN_TYPES, DECIDE, SELECT],
        _ => &[],
    }
}

fn collect_lr2_skin_paths(dir: &Path, depth: usize, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                collect_lr2_skin_paths(&path, depth + 1, paths);
            }
        } else if is_lr2_skin_path(&path) {
            paths.push(path);
        }
    }
}

/// Same-theme LR2 skins standing in for the scenes that fill LR2's shared image slots
/// before `skin_path` (BMZ extension). One skin per provider type, first by path.
pub(super) fn lr2_continue_theme_providers(
    skin_path: &Path,
    library_roots: &[PathBuf],
) -> Vec<PathBuf> {
    if !is_lr2_skin_path(skin_path) {
        return Vec::new();
    }
    let Ok(info) = bmz_skin::load_lr2_csv_skin_info(skin_path) else {
        return Vec::new();
    };
    let groups = lr2_continue_provider_types(info.skin_type);
    let Some(theme) =
        (!groups.is_empty()).then(|| skin_theme_dir(skin_path, library_roots)).flatten()
    else {
        return Vec::new();
    };
    let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let own = absolute(skin_path);
    let mut paths = Vec::new();
    collect_lr2_skin_paths(&theme, 0, &mut paths);
    paths.sort();
    let typed: Vec<(PathBuf, i32)> = paths
        .into_iter()
        .filter(|path| absolute(path) != own)
        .filter_map(|path| {
            let skin_type = bmz_skin::load_lr2_csv_skin_info(&path).ok()?.skin_type;
            Some((path, skin_type))
        })
        .collect();
    groups
        .iter()
        .filter_map(|types| {
            typed
                .iter()
                .find(|(_, skin_type)| types.contains(skin_type))
                .map(|(path, _)| path.clone())
        })
        .collect()
}

/// The LR2 Play skin whose CONTINUE assets stand in for the attempt's Play skin
/// (BMZ extension). `None` when the Play skin was an LR2 skin of the Result's theme,
/// or the theme has no LR2 Play skin.
pub(super) fn lr2_continue_fallback_play_skin<'a>(
    result_skin: &Path,
    played_skin: Option<&Path>,
    played_skin_is_lr2: bool,
    candidates: impl IntoIterator<Item = &'a Path>,
    library_roots: &[PathBuf],
) -> Option<PathBuf> {
    let theme = skin_theme_dir(result_skin, library_roots)?;
    if played_skin_is_lr2
        && played_skin.and_then(|path| skin_theme_dir(path, library_roots)).as_ref() == Some(&theme)
    {
        return None;
    }
    candidates
        .into_iter()
        .find(|path| skin_theme_dir(path, library_roots).as_ref() == Some(&theme))
        .map(Path::to_path_buf)
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
