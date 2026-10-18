use super::*;

/// Loads folders and charts immediately under `folder_path`.
/// Non-leaf folders are listed first, followed by charts.
/// Leaf folders (subfolders that contain charts but no further subfolders) are
/// flattened: their charts appear directly at this level instead of as a folder entry.
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
    // 子孫 folder_path を 1 回だけ引き、直下の子と各子が leaf かどうかを
    // Rust 側で集計する。`/` 区切り後の最初のセグメントが「直下の子の名前」、
    // それより深いセグメントが残っていれば leaf でない。
    let folder_key = folder_path.replace('\\', "/");
    let prefix_len = folder_key.len() + 1; // including the trailing '/'
    let descendants = library_db.list_descendant_folder_paths(&folder_key)?;

    // child_name -> has_grandchild (= 非 leaf)
    let mut child_state: std::collections::BTreeMap<String, bool> =
        std::collections::BTreeMap::new();
    for descendant in &descendants {
        let Some(rest) = descendant.get(prefix_len..) else { continue };
        let (child, deeper) = match rest.split_once('/') {
            Some((head, tail)) => (head, !tail.is_empty()),
            None => (rest, false),
        };
        if child.is_empty() {
            continue;
        }
        let entry = child_state.entry(child.to_string()).or_insert(false);
        if deeper {
            *entry = true;
        }
    }

    let mut non_leaf_folders: Vec<(String, String)> = Vec::new();
    let mut leaf_folder_paths: Vec<String> = Vec::new();
    for (name, has_grandchild) in child_state {
        let mut child_path = format!("{folder_key}/{name}");
        let mut name = name;
        let mut has_grandchild = has_grandchild;
        // `pack.zip!/song/...` の書庫フォルダ1段は、展開済みの `song/` と同じに見せる。
        if has_grandchild
            && is_archive_folder_name(&name)
            && let Some((inner, inner_deeper)) = single_archive_subfolder(&descendants, &child_path)
        {
            child_path = format!("{child_path}/{inner}");
            name = inner;
            has_grandchild = inner_deeper;
        }
        if has_grandchild {
            non_leaf_folders.push((child_path, name));
        } else {
            leaf_folder_paths.push(child_path);
        }
    }
    // 表示順は元実装に合わせ COLLATE NOCASE 相当。BTreeMap は code-point 順
    // なので、ここで lowercase 比較に揃え直す。
    non_leaf_folders.sort_by_key(|(_, name)| name.to_lowercase());

    // 親フォルダ自身 + leaf 子フォルダ群の charts を 1 つのプリペアド
    // ステートメントで取得する。
    let mut fetch_paths: Vec<&str> = Vec::with_capacity(1 + leaf_folder_paths.len());
    fetch_paths.push(folder_key.as_str());
    fetch_paths.extend(leaf_folder_paths.iter().map(String::as_str));
    let mut all_charts = library_db.list_charts_in_folders(&fetch_paths)?;
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

    let mut items = Vec::with_capacity(non_leaf_folders.len() + chart_items.len());
    for (path, name) in non_leaf_folders {
        if !folder_intersects_active_song_roots(&path, active_song_roots) {
            continue;
        }
        items.push(SelectItem::Folder { path, name, kind: SelectRowKind::Folder, summary: None });
    }
    items.extend(chart_items);

    Ok(items)
}

/// `pack.zip!` のような書庫のフォルダ名か。`!` の前がZIP / RAR / 7zの拡張子の場合だけ。
fn is_archive_folder_name(name: &str) -> bool {
    name.strip_suffix('!')
        .is_some_and(|archive| crate::chart_source::is_archive_file(std::path::Path::new(archive)))
}

/// 書庫フォルダの直下に譜面が無く、フォルダが1つだけなら、その名前と、
/// さらに下にフォルダがあるかを返す。
fn single_archive_subfolder(descendants: &[String], archive_path: &str) -> Option<(String, bool)> {
    let prefix = format!("{archive_path}/");
    let mut inner: Option<&str> = None;
    let mut deeper = false;
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
        deeper |= !tail.is_empty();
    }
    inner.map(|inner| (inner.to_string(), deeper))
}
