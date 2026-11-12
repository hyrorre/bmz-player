use super::*;
use bmz_render::scene::SelectRowKind;

/// フォルダ連続オートプレイでたどるフォルダの深さの上限。
const AUTOPLAY_FOLDER_MAX_DEPTH: usize = 16;

/// フォルダ連続オートプレイで中身をたどれる行か。
///
/// 曲フォルダに加え、難易度表・検索・仮想フォルダなど、選曲画面で開くと譜面が並ぶ
/// フォルダを対象にする。コース・コマンド・設定などの行は対象外。
pub(in crate::app) fn autoplay_folder_row_supported(kind: SelectRowKind) -> bool {
    matches!(
        kind,
        SelectRowKind::Folder
            | SelectRowKind::TableFolder
            | SelectRowKind::SearchFolder
            | SelectRowKind::Container
    )
}

/// フォルダ連続オートプレイで流す譜面を、選曲画面で辿れる順に集める。
///
/// `items` の譜面行はそのまま、フォルダ行は選曲画面で開いたときの一覧から
/// 再帰的に集める。曲フォルダは配下の譜面フォルダを DB から直接引く。
/// 同じ譜面は最初の1回だけ流す。
pub(in crate::app) fn autoplay_folder_chart_ids<'a>(
    boot: &SelectListSource<'_>,
    virtual_folders: &Result<crate::screens::select_model::VirtualFolderCatalog, String>,
    items: impl IntoIterator<Item = &'a SelectItem>,
) -> Result<Vec<i64>> {
    let mut collector = AutoplayFolderCollector {
        boot,
        virtual_folders,
        chart_ids: Vec::new(),
        seen_charts: HashSet::new(),
        visited_folders: HashSet::new(),
    };
    collector.collect_items(items, 0)?;
    Ok(collector.chart_ids)
}

struct AutoplayFolderCollector<'b, 'c> {
    boot: &'b SelectListSource<'c>,
    virtual_folders: &'b Result<crate::screens::select_model::VirtualFolderCatalog, String>,
    chart_ids: Vec<i64>,
    seen_charts: HashSet<i64>,
    visited_folders: HashSet<String>,
}

impl AutoplayFolderCollector<'_, '_> {
    fn collect_items<'a>(
        &mut self,
        items: impl IntoIterator<Item = &'a SelectItem>,
        depth: usize,
    ) -> Result<()> {
        for item in items {
            match item {
                SelectItem::Chart(row) => {
                    if let Some(chart) = &row.chart {
                        self.push_chart(chart.chart_id);
                    }
                }
                SelectItem::Folder { path, kind, .. } if autoplay_folder_row_supported(*kind) => {
                    self.collect_folder(path, *kind, depth)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_folder(&mut self, path: &str, kind: SelectRowKind, depth: usize) -> Result<()> {
        if depth >= AUTOPLAY_FOLDER_MAX_DEPTH || !self.visited_folders.insert(path.to_string()) {
            return Ok(());
        }
        if kind == SelectRowKind::Folder && self.collect_song_folder(path)? {
            return Ok(());
        }
        let items = build_select_items_for_stack(
            self.boot,
            self.virtual_folders,
            std::slice::from_ref(&path.to_string()),
            &[],
        )?;
        self.collect_items(&items, depth + 1)
    }

    /// 曲フォルダ配下の譜面を DB から集める。譜面が見つからなければ false。
    fn collect_song_folder(&mut self, path: &str) -> Result<bool> {
        let library_db = self.boot.library_db;
        let folder_paths = match reachable_chart_folders(library_db, path) {
            Ok(folder_paths) => folder_paths,
            Err(error) => {
                tracing::warn!(folder = %path, %error, "failed to list autoplay folder charts");
                vec![path.to_string()]
            }
        };
        let folder_refs: Vec<&str> = folder_paths.iter().map(String::as_str).collect();
        let charts = library_db.list_charts_in_folders(&folder_refs)?;
        if charts.is_empty() {
            return Ok(false);
        }
        for chart in charts {
            self.push_chart(chart.chart_id);
        }
        Ok(true)
    }

    fn push_chart(&mut self, chart_id: i64) {
        if self.seen_charts.insert(chart_id) {
            self.chart_ids.push(chart_id);
        }
    }
}
