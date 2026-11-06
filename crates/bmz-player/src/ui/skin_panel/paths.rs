pub(in crate::ui) struct SkinUiPathContext {
    package: Option<bmz_skin::SkinPathContext>,
}

pub(in crate::ui) struct SkinUiAccess<'a> {
    pub paths: &'a AppPaths,
    pub grants: &'a BTreeMap<String, crate::config::profile_config::SkinAccessGrant>,
}

/// スキン設定 UI で使う package-aware path context のスロット別キャッシュ。
///
/// Lua skin の context 構築は entry と library root の canonicalize を伴うため、
/// 同じスキンを表示している間は毎フレーム作り直さない。
#[derive(Default)]
pub(in crate::ui) struct SkinUiPathCache {
    entries: BTreeMap<&'static str, (String, Option<SkinUiPathContext>)>,
    access: BTreeMap<String, crate::config::profile_config::SkinAccessGrant>,
    roots: Vec<PathBuf>,
}

impl SkinUiPathCache {
    pub(in crate::ui) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(in crate::ui) fn get_or_resolve(
        &mut self,
        slot: SkinSlot,
        app_paths: &AppPaths,
        skin_path: &str,
        access: &BTreeMap<String, crate::config::profile_config::SkinAccessGrant>,
    ) -> Option<&SkinUiPathContext> {
        // Compare configured names without canonicalizing roots every frame.
        let roots = vec![app_paths.data_dir.join("skins"), app_paths.resource_dir.join("skins")];
        if self.access != *access || self.roots != roots {
            self.clear();
            self.access.clone_from(access);
            self.roots = roots;
        }
        let key = slot.defs_header_id();
        let needs_refresh =
            self.entries.get(key).is_none_or(|(cached_path, _)| cached_path != skin_path);
        if needs_refresh {
            let context = skin_root_path(app_paths, skin_path, access);
            self.entries.insert(key, (skin_path.to_string(), context));
        }
        self.entries.get(key).and_then(|(_, context)| context.as_ref())
    }
}

impl SkinUiPathContext {
    #[cfg(test)]
    pub(in crate::ui) fn legacy(root: &Path) -> Self {
        Self { package: bmz_skin::SkinPathContext::for_directory(root).ok() }
    }

    #[cfg(test)]
    pub(in crate::ui) fn package(_root: &Path, package: bmz_skin::SkinPathContext) -> Self {
        Self { package: Some(package) }
    }
}

/// スキンパス文字列から設定 UI 用のパス context を得る。
pub(in crate::ui) fn skin_root_path(
    app_paths: &AppPaths,
    skin_path: &str,
    access: &BTreeMap<String, crate::config::profile_config::SkinAccessGrant>,
) -> Option<SkinUiPathContext> {
    let trimmed = skin_path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = app_paths.resolve_path_ref(trimmed).ok()?;
    let context = app_paths.skin_path_context_with_access(access, &path).ok()?;
    let package = Some(context);
    Some(SkinUiPathContext { package })
}

pub(in crate::ui) fn glob_candidates_for_skin(
    context: &SkinUiPathContext,
    pattern: &str,
) -> Vec<String> {
    match &context.package {
        Some(package) => package.wildcard_candidate_values(pattern).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// `pattern` (スキンルート相対、末尾要素にワイルドカード `*` を 1 個まで) に
/// マッチするファイルの相対パス一覧を返す。
///
/// beatoraja の `path|filter|` 形式の `|...|` 接尾辞 (lanecover などの
/// アセット用途タグ) は対象ファイル名には含まれないので、列挙前に取り除く。
#[cfg(test)]
pub(in crate::ui) fn glob_candidates(root: &Path, pattern: &str) -> Vec<String> {
    bmz_skin::SkinPathContext::for_directory(root)
        .and_then(|context| context.wildcard_candidate_values(pattern))
        .unwrap_or_default()
}

pub(in crate::ui) fn normalize_filepath_selection(
    selected: &str,
    candidates: &[String],
) -> Option<String> {
    if selected.is_empty() || selected == RANDOM_FILE_SELECTION {
        return None;
    }
    let normalized = selected.replace('\\', "/");
    if candidates.iter().any(|candidate| candidate == &normalized) {
        return (normalized != selected).then_some(normalized);
    }
    if normalized.contains('/') {
        return None;
    }
    candidates
        .iter()
        .find(|candidate| {
            filepath_selection_label(candidate).eq_ignore_ascii_case(normalized.as_str())
        })
        .cloned()
}

pub(in crate::ui) fn filepath_selection_label(value: &str) -> &str {
    let slash = value.rfind('/').into_iter().chain(value.rfind('\\')).max();
    match slash {
        Some(index) if index + 1 < value.len() => &value[index + 1..],
        _ => value,
    }
}

fn filepath_selection_index(selected: &str, candidates: &[String]) -> Option<usize> {
    if selected == RANDOM_FILE_SELECTION {
        return Some(0);
    }
    let normalized = normalize_filepath_selection(selected, candidates)
        .unwrap_or_else(|| selected.replace('\\', "/"));
    candidates.iter().position(|candidate| candidate == &normalized).map(|index| index + 1)
}

/// ファイル選択は「ランダム」を先頭とし、その後ろに glob 候補を並べる。
pub(in crate::ui) fn previous_filepath_selection(
    selected: &str,
    candidates: &[String],
) -> Option<String> {
    let index = filepath_selection_index(selected, candidates)?.checked_sub(1)?;
    if index == 0 {
        Some(RANDOM_FILE_SELECTION.to_string())
    } else {
        candidates.get(index - 1).cloned()
    }
}

pub(in crate::ui) fn next_filepath_selection(
    selected: &str,
    candidates: &[String],
) -> Option<String> {
    let index = filepath_selection_index(selected, candidates)?.checked_add(1)?;
    candidates.get(index - 1).cloned()
}

/// property の既定選択肢名。beatoraja と同じく `def` が item name と一致する
/// ときだけ採用し、未指定/不一致なら先頭 item を使う。
pub(in crate::ui) fn property_default(prop: &SkinPropertyDef) -> String {
    prop.item
        .iter()
        .find(|item| !prop.def.is_empty() && item.name == prop.def)
        .or_else(|| prop.item.first())
        .map(|item| item.name.clone())
        .unwrap_or_default()
}

pub(in crate::ui) fn property_selection_is_valid(prop: &SkinPropertyDef, selected: &str) -> bool {
    property_selection_index(prop, selected).is_some()
}

fn property_selection_index(prop: &SkinPropertyDef, selected: &str) -> Option<usize> {
    prop.item.iter().position(|item| item.name == selected).or_else(|| {
        selected.parse::<i32>().ok().and_then(|op| prop.item.iter().position(|item| item.op == op))
    })
}

pub(in crate::ui) fn previous_property_selection<'a>(
    prop: &'a SkinPropertyDef,
    selected: &str,
) -> Option<&'a str> {
    let index = property_selection_index(prop, selected)?.checked_sub(1)?;
    Some(prop.item.get(index)?.name.as_str())
}

pub(in crate::ui) fn next_property_selection<'a>(
    prop: &'a SkinPropertyDef,
    selected: &str,
) -> Option<&'a str> {
    let index = property_selection_index(prop, selected)?.checked_add(1)?;
    Some(prop.item.get(index)?.name.as_str())
}

pub(in crate::ui) fn filepath_default(
    filepath: &SkinFilepathDef,
    candidates: &[String],
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    // def が "Random" のときは具体ファイルへ固定せず、ランダム番兵を既定にする
    // (beatoraja の def="Random" 相当)。
    if filepath.def.eq_ignore_ascii_case(RANDOM_FILE_SELECTION) {
        return Some(RANDOM_FILE_SELECTION.to_string());
    }
    if !filepath.def.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, &filepath.def))
    {
        return Some(candidate.clone());
    }
    if filepath.def.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, "default"))
    {
        return Some(candidate.clone());
    }
    candidates.first().cloned()
}

pub(in crate::ui) fn filename_matches_def(candidate: &str, def: &str) -> bool {
    let file_name = Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.eq_ignore_ascii_case(def) {
        return true;
    }
    let stem = Path::new(file_name).file_stem().and_then(|stem| stem.to_str()).unwrap_or(file_name);
    if stem.eq_ignore_ascii_case(def) {
        return true;
    }
    filepath_def_acronym(def).is_some_and(|acronym| {
        let stem_lower = stem.to_ascii_lowercase();
        let acronym_lower = acronym.to_ascii_lowercase();
        stem_lower == acronym_lower || stem_lower.starts_with(&acronym_lower)
    })
}

pub(in crate::ui) fn filepath_def_acronym(def: &str) -> Option<String> {
    if !def.contains('-') {
        return None;
    }
    let acronym = def
        .split('-')
        .filter_map(|part| part.chars().find(|ch| ch.is_ascii_alphanumeric()))
        .collect::<String>();
    (!acronym.is_empty()).then_some(acronym)
}
use super::*;
