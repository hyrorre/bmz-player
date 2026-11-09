use super::*;
use crate::screens::select_model::{SelectCollectionCache, VirtualFolderCatalog};

pub(super) struct SelectListRequest {
    pub action: SelectListAction,
    pub app_config: crate::config::app_config::AppConfig,
    pub profile_config: crate::config::profile_config::ProfileConfig,
    pub library_path: PathBuf,
    pub profile_paths: crate::paths::ProfilePaths,
    pub folder_stack: Vec<String>,
    pub selected_index_stack: Vec<usize>,
    pub selected_index: usize,
    pub selected_key: Option<SelectItemKey>,
    pub history: Vec<String>,
    pub mode: SelectModeFilter,
    pub difficulty: SelectDifficultyFilter,
    pub sort: SelectSort,
}

pub(super) struct LoadedSelectList {
    pub prepared: Option<PreparedSelectList>,
    pub virtual_folders: Result<VirtualFolderCatalog, String>,
}

type Completion = (SelectListAction, Result<LoadedSelectList>);

enum SelectListWorker {
    Query(u64, Receiver<Completion>),
    Retire(Receiver<()>),
}

/// At most one query worker and one latest request. Invalidated jobs release
/// their large results on the worker whenever possible; none are joined here.
#[derive(Default)]
pub(super) struct SelectListLoader {
    generation: Arc<AtomicU64>,
    active: Option<SelectListWorker>,
    queued: Option<Box<SelectListRequest>>,
    requested_action: Option<SelectListAction>,
}

impl SelectListLoader {
    pub(super) fn request(&mut self, mut request: SelectListRequest) {
        // Maintenance/option refreshes must not cancel a folder navigation that
        // the user already requested against the still-visible old list.
        if !matches!(request.action, SelectListAction::Enter(_) | SelectListAction::Exit)
            && let Some(action @ (SelectListAction::Enter(_) | SelectListAction::Exit)) =
                &self.requested_action
        {
            request.action = action.clone();
        }
        self.requested_action = Some(request.action.clone());
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.queued = Some(Box::new(request));
        self.start_queued();
    }

    pub(super) fn loading(&self) -> bool {
        self.queued.is_some()
            || matches!(&self.active, Some(SelectListWorker::Query(generation, _))
                if *generation == self.generation.load(Ordering::Relaxed))
    }

    pub(super) fn cancel(&mut self) {
        if self.loading() {
            self.generation.fetch_add(1, Ordering::Relaxed);
            self.queued = None;
            self.requested_action = None;
        }
    }

    pub(super) fn poll(&mut self) -> Option<Completion> {
        let mut completed = None;
        if let Some(active) = self.active.take() {
            match active {
                SelectListWorker::Retire(rx) => {
                    if matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                        self.active = Some(SelectListWorker::Retire(rx));
                    }
                }
                SelectListWorker::Query(generation, rx) => match rx.try_recv() {
                    Ok(result) if generation == self.generation.load(Ordering::Relaxed) => {
                        completed = Some(result);
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        self.active = Some(SelectListWorker::Query(generation, rx))
                    }
                    Err(mpsc::TryRecvError::Disconnected)
                        if generation == self.generation.load(Ordering::Relaxed) =>
                    {
                        completed = Some((
                            SelectListAction::Refresh,
                            Err(anyhow::anyhow!("select list worker disconnected")),
                        ));
                    }
                    Ok(obsolete) => self.retire(obsolete),
                    Err(mpsc::TryRecvError::Disconnected) => {}
                },
            }
        }
        if completed.is_some() {
            self.requested_action = None;
        }
        self.start_queued();
        completed
    }

    /// Reuse the same one-worker budget for destruction. A fresh query waits
    /// behind this short task while the current list stays visible.
    pub(super) fn retire<T: Send + 'static>(&mut self, value: T) {
        debug_assert!(self.active.is_none());
        let (tx, rx) = mpsc::channel();
        if let Err(error) =
            thread::Builder::new().name("select-list-retire".into()).spawn(move || {
                drop(value);
                let _ = tx.send(());
            })
        {
            tracing::warn!(%error, "failed to start select list retirement");
        }
        self.active = Some(SelectListWorker::Retire(rx));
    }

    fn start_queued(&mut self) {
        if self.active.is_some() {
            return;
        }
        let Some(request) = self.queued.take() else {
            return;
        };
        let generation = self.generation.load(Ordering::Relaxed);
        let current = self.generation.clone();
        let (tx, rx) = mpsc::channel();
        let error_tx = tx.clone();
        let action = request.action.clone();
        if let Err(error) = thread::Builder::new().name("select-list".into()).spawn(move || {
            if current.load(Ordering::Relaxed) != generation {
                return;
            }
            let result = load(&request);
            if current.load(Ordering::Relaxed) == generation {
                let _ = tx.send((request.action.clone(), result));
            }
        }) {
            let _ = error_tx.send((action, Err(error.into())));
        }
        self.active = Some(SelectListWorker::Query(generation, rx));
    }
}

fn load(request: &SelectListRequest) -> Result<LoadedSelectList> {
    use crate::storage::{
        collection_db::CollectionDatabase, library_db::LibraryDatabase, score_db::ScoreDatabase,
    };
    let library_db = LibraryDatabase::open_read_only(&request.library_path)?;
    let score_db = ScoreDatabase::open_read_only(&request.profile_paths.score_db)?;
    let collection_db = CollectionDatabase::open_read_only(&request.profile_paths.collection_db)?;
    let source = SelectListSource {
        app_config: &request.app_config,
        profile_config: &request.profile_config,
        library_db: &library_db,
        score_db: &score_db,
        collection_db: &collection_db,
    };
    let virtual_folders = VirtualFolderCatalog::load(&request.profile_paths.root_dir)
        .map_err(|error| format!("{error:#}"));
    let mut collections = SelectCollectionCache::default();
    let mut prepared = prepare_select_list(
        &request.action,
        &request.folder_stack,
        &request.selected_index_stack,
        request.selected_index,
        request.selected_key.clone(),
        |stack| {
            let (mut items, mut mode) = load_items_for_stack(
                SelectListContext {
                    boot: source,
                    collections: &mut collections,
                    virtual_folders: &virtual_folders,
                },
                stack,
                &request.history,
                request.mode,
                request.difficulty,
                request.sort,
            )?;
            if matches!(request.action, SelectListAction::RefreshDownloads) {
                mode = request.mode;
                apply_select_mode_filter(&mut items, mode);
            }
            Ok((items, mode))
        },
    )?;
    if let Some(prepared) = &mut prepared {
        // Source resolution can touch the filesystem and query archive metadata.
        // Table-level loading already resolved these before enrichment.
        let table_level = matches!(
            prepared.folder_stack.last().and_then(|path| parse_table_path(path)),
            Some(TablePath::Level { .. })
        );
        let charts: Vec<_> = prepared
            .items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Chart(row) if !table_level => row.chart.as_ref(),
                _ => None,
            })
            .collect();
        let sources = library_db.registered_chart_sources(&charts).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to resolve select chart sources");
            HashMap::new()
        });
        let replacement_ids: Vec<_> = sources
            .iter()
            .filter_map(|(id, source)| {
                (*id != source.chart.chart_id).then_some(source.chart.chart_id)
            })
            .collect();
        let analyses =
            library_db.chart_analysis_summaries_by_chart_ids(&replacement_ids).unwrap_or_default();
        for item in &mut prepared.items {
            if let SelectItem::Chart(row) = item
                && let Some(chart) = &row.chart
                && let Some(source) = sources.get(&chart.chart_id)
                && source.chart.chart_id != chart.chart_id
            {
                row.has_document = source.chart.has_document;
                row.chart_analysis = analyses.get(&source.chart.chart_id).cloned();
                row.chart = Some(source.chart.clone());
            }
        }
    }
    Ok(LoadedSelectList { prepared, virtual_folders })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(boot: &BootstrappedApp, action: SelectListAction) -> SelectListRequest {
        SelectListRequest {
            action,
            app_config: boot.app_config.clone(),
            profile_config: boot.profile_config.clone(),
            library_path: boot.app_paths.library_db.clone(),
            profile_paths: boot.profile_paths.clone(),
            folder_stack: Vec::new(),
            selected_index_stack: Vec::new(),
            selected_index: 0,
            selected_key: None,
            history: Vec::new(),
            mode: SelectModeFilter::All,
            difficulty: SelectDifficultyFilter::All,
            sort: SelectSort::Title,
        }
    }

    fn complete(loader: &mut SelectListLoader) -> Completion {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(result) = loader.poll() {
                return result;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn list_requests_coalesce_and_keep_navigation_across_refresh() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let mut loader = SelectListLoader::default();
        let (tx, rx) = mpsc::channel();
        loader.active = Some(SelectListWorker::Query(0, rx));
        loader.request(request(&boot, SelectListAction::Enter("bmz-table:".into())));
        loader.request(request(&boot, SelectListAction::Refresh));
        assert!(
            matches!(&loader.queued.as_ref().unwrap().action, SelectListAction::Enter(path) if path == "bmz-table:")
        );
        assert!(
            matches!(loader.active, Some(SelectListWorker::Query(0, _))),
            "only one worker can be active"
        );
        tx.send((SelectListAction::Refresh, Err(anyhow::anyhow!("obsolete")))).unwrap();
        assert!(loader.poll().is_none(), "old errors must not replace the latest request");
        let (action, result) = complete(&mut loader);
        assert!(matches!(action, SelectListAction::Enter(_)));
        assert!(result.unwrap().prepared.is_none(), "empty entry keeps the current view");
        assert!(!loader.loading());
    }

    #[test]
    fn list_cancel_discards_even_a_buffered_completion() {
        let mut loader = SelectListLoader::default();
        let (tx, rx) = mpsc::channel();
        loader.active = Some(SelectListWorker::Query(0, rx));
        tx.send((SelectListAction::Refresh, Err(anyhow::anyhow!("obsolete")))).unwrap();
        loader.cancel();
        assert!(!loader.loading());
        assert!(loader.poll().is_none());
        assert!(matches!(loader.active, Some(SelectListWorker::Retire(_))));
    }

    #[test]
    fn list_query_failure_keeps_a_result_error_and_can_be_retried() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let mut loader = SelectListLoader::default();
        let mut failed = request(&boot, SelectListAction::Refresh);
        failed.library_path = data.paths.data_dir.join("does-not-exist.db");
        loader.request(failed);
        assert!(complete(&mut loader).1.is_err());
        loader.request(request(&boot, SelectListAction::Refresh));
        assert!(complete(&mut loader).1.unwrap().prepared.is_some());
    }
}
