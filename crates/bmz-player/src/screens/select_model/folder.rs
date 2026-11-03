use super::*;

/// Loads the items shown inside `folder_path`, following beatoraja's `FolderBar`.
///
/// A folder that contains charts is a song folder: it lists only its own charts.
/// Otherwise, subfolders that contain charts are flattened into this level, and
/// subfolders without charts are listed as folders only when no subfolder has charts.
pub fn load_select_items_in_folder(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn load_select_items_in_folder_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode_with_table_order(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        rule_mode,
        &[],
    )
}

pub fn load_select_items_in_folder_for_rule_mode_with_table_order(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode_with_filters(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        None,
        None,
    )
}

pub fn load_select_items_in_folder_for_rule_mode_with_filters(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_song_roots: Option<&[String]>,
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let folder_key = folder_path.replace('\\', "/");
    let own_charts = library_db.list_charts_in_folders(&[folder_key.as_str()])?;
    // beatorajaは譜面を含むフォルダの下を走査しないので、曲フォルダには自分の譜面だけを並べる。
    // 同一曲フォルダもbeatorajaの `SameFolderBar` と同じく、ここで自分の譜面だけになる。
    let (folders, mut all_charts) = if own_charts.is_empty() {
        let descendants = sorted_descendant_folders(library_db, &folder_key)?;
        let listing = beatoraja_folder_listing(&folder_key, &descendants);
        let chart_paths: Vec<&str> = listing.chart_folders.iter().map(String::as_str).collect();
        (listing.folders, library_db.list_charts_in_folders(&chart_paths)?)
    } else {
        (Vec::new(), own_charts)
    };
    retain_active_charts(&mut all_charts, active_song_roots);

    let chart_items = chart_items_with_enrichment(
        library_db,
        score_db,
        all_charts,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        active_table_sources,
    )?;

    let mut items = Vec::with_capacity(folders.len() + chart_items.len());
    for (path, name) in folders {
        if !folder_intersects_active_song_roots(&path, active_song_roots) {
            continue;
        }
        items.push(SelectItem::Folder { path, name, kind: SelectRowKind::Folder, summary: None });
    }
    items.extend(chart_items);

    Ok(items)
}

/// Chart folders reachable from `folder_path` by browsing with the same rules as
/// [`load_select_items_in_folder`]. Folder summaries and folder autoplay use this,
/// so they count exactly the charts the player can open from the folder.
pub fn reachable_chart_folders(
    library_db: &LibraryDatabase,
    folder_path: &str,
) -> Result<Vec<String>> {
    let folder_key = folder_path.replace('\\', "/");
    if library_db.folder_has_charts(&folder_key)? {
        return Ok(vec![folder_key]);
    }
    let descendants = sorted_descendant_folders(library_db, &folder_key)?;
    let mut chart_folders = Vec::new();
    collect_reachable_chart_folders(&folder_key, &descendants, &mut chart_folders);
    Ok(chart_folders)
}

fn collect_reachable_chart_folders(
    folder_key: &str,
    descendants: &[String],
    out: &mut Vec<String>,
) {
    let listing = beatoraja_folder_listing(folder_key, descendants_under(descendants, folder_key));
    out.extend(listing.chart_folders);
    for (path, _) in listing.folders {
        collect_reachable_chart_folders(&path, descendants, out);
    }
}

fn sorted_descendant_folders(
    library_db: &LibraryDatabase,
    folder_key: &str,
) -> Result<Vec<String>> {
    let mut descendants = library_db.list_descendant_folder_paths(folder_key)?;
    descendants.sort_unstable();
    Ok(descendants)
}

/// 昇順の譜面フォルダから、`folder_key` より下にあるものを返す。
fn descendants_under<'a>(sorted: &'a [String], folder_key: &str) -> &'a [String] {
    // `/` (0x2F) の次は `0` (0x30)。`prefix..end` は `prefix` で始まる範囲になる。
    let prefix = format!("{folder_key}/");
    let end = format!("{folder_key}0");
    let start = sorted.partition_point(|path| path.as_str() < prefix.as_str());
    let stop = sorted.partition_point(|path| path.as_str() < end.as_str());
    &sorted[start..stop]
}

/// 譜面の無いフォルダの直下に並べるもの。
struct FolderListing {
    /// (path, 表示名)
    folders: Vec<(String, String)>,
    /// 譜面を直接並べる曲フォルダ
    chart_folders: Vec<String>,
}

/// beatorajaの `FolderBar` と同じ規則で、譜面の無い `folder_key` の直下に並べるものを決める。
///
/// - 譜面を含む子フォルダは曲フォルダとして譜面を直接並べ、その下は扱わない
///   （beatorajaは譜面を含むフォルダの下を走査しない）。
/// - 曲フォルダが1つでもあれば譜面だけを並べ、譜面の無い子フォルダは並べない。
/// - 書庫の中にはフォルダの行を作らない（beatorajaは書庫を1つの曲フォルダとして登録する）。
fn beatoraja_folder_listing(folder_key: &str, descendants: &[String]) -> FolderListing {
    let prefix = format!("{folder_key}/");
    // child name -> 子フォルダ自身に譜面があるか
    let mut children: std::collections::BTreeMap<&str, bool> = std::collections::BTreeMap::new();
    for descendant in descendants {
        let Some(rest) = descendant.strip_prefix(&prefix) else { continue };
        let (child, deeper) = match rest.split_once('/') {
            Some((head, tail)) => (head, !tail.is_empty()),
            None => (rest, false),
        };
        if child.is_empty() {
            continue;
        }
        *children.entry(child).or_insert(false) |= !deeper;
    }

    let mut folders = Vec::new();
    let mut chart_folders = Vec::new();
    for (name, has_charts) in children {
        let mut child_path = format!("{prefix}{name}");
        let mut name = name.to_string();
        let mut has_charts = has_charts;
        // `pack.zip!/song/...` の書庫フォルダ1段は、展開済みの `song/` と同じに見せる。
        if !has_charts
            && is_archive_folder_name(&name)
            && let Some((inner, inner_has_charts)) =
                single_archive_subfolder(descendants, &child_path)
        {
            child_path = format!("{child_path}/{inner}");
            name = inner;
            has_charts = inner_has_charts;
        }
        if has_charts {
            chart_folders.push(child_path);
        } else {
            let name = archive_display_name(&name).map_or(name.clone(), str::to_owned);
            folders.push((child_path, name));
        }
    }
    if !chart_folders.is_empty() || folder_key.split('/').any(is_archive_folder_name) {
        folders.clear();
    }
    // 表示順は COLLATE NOCASE 相当。BTreeMap は code-point 順なので lowercase 比較に揃え直す。
    folders.sort_by_key(|(_, name)| name.to_lowercase());
    FolderListing { folders, chart_folders }
}

/// `pack.zip!` のような書庫のフォルダ名か。`!` の前がZIP / RAR / 7zの拡張子の場合だけ。
fn is_archive_folder_name(name: &str) -> bool {
    name.strip_suffix('!')
        .is_some_and(|archive| crate::chart_source::is_archive_file(std::path::Path::new(archive)))
}

/// 選曲画面に残る書庫フォルダは、展開したフォルダと同じく拡張子と `!` を除いて表示する。
fn archive_display_name(name: &str) -> Option<&str> {
    if !is_archive_folder_name(name) {
        return None;
    }
    let archive = name.strip_suffix('!')?;
    bmz_song_resource::archive_stem(archive)
}

/// 書庫フォルダの直下に譜面が無く、フォルダが1つだけなら、その名前と、
/// そのフォルダ自身に譜面があるかを返す。
fn single_archive_subfolder(descendants: &[String], archive_path: &str) -> Option<(String, bool)> {
    let prefix = format!("{archive_path}/");
    let mut inner: Option<&str> = None;
    let mut has_charts = false;
    for descendant in descendants {
        if descendant == archive_path {
            // 書庫の直下にも譜面がある。
            return None;
        }
        let Some(rest) = descendant.strip_prefix(&prefix) else { continue };
        let (head, tail) = rest.split_once('/').unwrap_or((rest, ""));
        if head.is_empty() {
            continue;
        }
        match inner {
            Some(existing) if existing != head => return None,
            _ => inner = Some(head),
        }
        has_charts |= tail.is_empty();
    }
    inner.map(|inner| (inner.to_string(), has_charts))
}
