use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::SelectRowKind;

use crate::ln_policy::LnPolicySetting;
use crate::screens::select_model::{
    COURSE_CONTENTS_PATH_PREFIX, COURSE_ROOT_PATH, FAVORITE_ROOT_PATH, SAME_FOLDER_PATH_PREFIX,
    SEARCH_PATH_PREFIX, SelectFolderSummary, SelectItem, TABLE_ROOT_PATH, TablePath,
    VIRTUAL_FOLDER_PATH_PREFIX, parse_same_folder_path, parse_search_query, parse_table_path,
    select_folder_summary_for_rule_mode,
};
use crate::screens::settings_model::CONFIG_ROOT_PATH;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreDatabase;

enum SelectFolderSummaryCacheEntry {
    Loading { view_generation: u64 },
    Ready(Option<SelectFolderSummary>),
    Missing { view_generation: u64 },
}

enum SelectFolderSummaryWorkerCommand {
    SetContext { view_generation: u64, data_generation: u64 },
    Load(SelectFolderSummaryRequest),
}

struct SelectFolderSummaryRequest {
    key: String,
    path: String,
    kind: SelectRowKind,
    active_song_roots: Vec<String>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    view_generation: u64,
    data_generation: u64,
}

struct SelectFolderSummaryResult {
    key: String,
    view_generation: u64,
    data_generation: u64,
    result: std::result::Result<Option<SelectFolderSummary>, String>,
}

/// 選曲フォルダのlamp集計を非同期で読み込み、表示文脈ごとの世代を管理する。
pub(super) struct SelectFolderSummaryRuntime {
    cache: HashMap<String, SelectFolderSummaryCacheEntry>,
    rows_need_cached_summaries: bool,
    request_tx: mpsc::Sender<SelectFolderSummaryWorkerCommand>,
    result_rx: Receiver<SelectFolderSummaryResult>,
    data_generation: u64,
    view_generation: u64,
    view_key: String,
    ln_policy: LnPolicySetting,
    rule_mode: RuleMode,
    active_song_roots: Vec<String>,
}

impl SelectFolderSummaryRuntime {
    /// Viewer mode は Select を表示しないため、DB worker を起動しない空 runtime を使う。
    pub(super) fn disabled(
        folder_stack: &[String],
        ln_policy: LnPolicySetting,
        rule_mode: RuleMode,
    ) -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        drop(request_rx);
        let (result_tx, result_rx) = mpsc::channel();
        drop(result_tx);
        Self {
            cache: HashMap::new(),
            rows_need_cached_summaries: true,
            request_tx,
            result_rx,
            data_generation: 0,
            view_generation: 0,
            view_key: view_key(folder_stack),
            ln_policy,
            rule_mode,
            active_song_roots: Vec::new(),
        }
    }

    pub(super) fn new(
        library_db_path: PathBuf,
        score_db_path: PathBuf,
        folder_stack: &[String],
        ln_policy: LnPolicySetting,
        rule_mode: RuleMode,
    ) -> Result<Self> {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        spawn_worker(library_db_path, score_db_path, request_rx, result_tx)?;
        Ok(Self {
            cache: HashMap::new(),
            rows_need_cached_summaries: true,
            request_tx,
            result_rx,
            data_generation: 0,
            view_generation: 0,
            view_key: view_key(folder_stack),
            ln_policy,
            rule_mode,
            active_song_roots: Vec::new(),
        })
    }

    pub(super) fn list_replaced(&mut self, folder_stack: &[String]) {
        // A refresh of the same folder also installs fresh rows.
        self.rows_need_cached_summaries = true;
        let next_view_key = view_key(folder_stack);
        if next_view_key == self.view_key {
            return;
        }
        self.view_key = next_view_key;
        self.view_generation = self.view_generation.wrapping_add(1);
        self.send_worker_context();
    }

    pub(super) fn invalidate_data(&mut self) {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.cache.clear();
        self.send_worker_context();
    }

    pub(super) fn refresh(
        &mut self,
        items: &mut [SelectItem],
        visible_indices: &[usize],
        ln_policy: LnPolicySetting,
        rule_mode: RuleMode,
        active_song_roots: &[String],
        current_folder: Option<&str>,
    ) {
        if self.active_song_roots != active_song_roots {
            self.active_song_roots = active_song_roots.to_vec();
            self.invalidate_data();
            for item in &mut *items {
                if let SelectItem::Folder { summary, .. } = item {
                    *summary = None;
                }
            }
        }
        self.sync_score_context(items, ln_policy, rule_mode);
        self.poll(items);
        if let Some((path, kind)) = current_folder.and_then(current_folder_summary_target) {
            self.request(path, kind);
        }
        for &index in visible_indices {
            if let Some(SelectItem::Folder { path, kind, summary: None, .. }) = items.get(index) {
                self.request(path, *kind);
            }
        }
    }

    pub(super) fn current_folder_summary(&self, path: &str) -> Option<&SelectFolderSummary> {
        let (path, kind) = current_folder_summary_target(path)?;
        let key = cache_key(path, kind, self.ln_policy, self.rule_mode);
        match self.cache.get(&key)? {
            SelectFolderSummaryCacheEntry::Ready(Some(summary)) => Some(summary),
            _ => None,
        }
    }

    pub(super) fn sync_score_context(
        &mut self,
        items: &mut [SelectItem],
        ln_policy: LnPolicySetting,
        rule_mode: RuleMode,
    ) {
        if ln_policy == self.ln_policy && rule_mode == self.rule_mode {
            return;
        }

        self.ln_policy = ln_policy;
        self.rule_mode = rule_mode;
        self.rows_need_cached_summaries = true;
        self.view_generation = self.view_generation.wrapping_add(1);
        for item in items {
            if let SelectItem::Folder { summary, .. } = item {
                *summary = None;
            }
        }
        self.send_worker_context();
    }

    fn poll(&mut self, items: &mut [SelectItem]) {
        while let Ok(result) = self.result_rx.try_recv() {
            if result.data_generation != self.data_generation {
                continue;
            }
            let entry = match result.result {
                Ok(summary) => {
                    self.rows_need_cached_summaries |= summary.is_some();
                    SelectFolderSummaryCacheEntry::Ready(summary)
                }
                Err(error) => {
                    tracing::warn!(
                        key = %result.key,
                        %error,
                        "select folder lamp summary worker failed"
                    );
                    SelectFolderSummaryCacheEntry::Missing {
                        view_generation: result.view_generation,
                    }
                }
            };
            self.cache.insert(result.key, entry);
        }

        if !std::mem::take(&mut self.rows_need_cached_summaries) {
            return;
        }
        for item in items {
            let SelectItem::Folder { path, kind, summary, .. } = item else {
                continue;
            };
            if summary.is_some() {
                continue;
            }
            let key = cache_key(path, *kind, self.ln_policy, self.rule_mode);
            if let Some(SelectFolderSummaryCacheEntry::Ready(Some(ready))) = self.cache.get(&key) {
                *summary = Some(ready.clone());
            }
        }
    }

    fn request(&mut self, path: &str, kind: SelectRowKind) {
        let key = cache_key(path, kind, self.ln_policy, self.rule_mode);
        match self.cache.get(&key) {
            Some(SelectFolderSummaryCacheEntry::Ready(_)) => return,
            Some(
                SelectFolderSummaryCacheEntry::Loading { view_generation }
                | SelectFolderSummaryCacheEntry::Missing { view_generation },
            ) if *view_generation == self.view_generation => return,
            _ => {}
        }
        self.cache.insert(
            key.clone(),
            SelectFolderSummaryCacheEntry::Loading { view_generation: self.view_generation },
        );
        let _ = self.request_tx.send(SelectFolderSummaryWorkerCommand::Load(
            SelectFolderSummaryRequest {
                key,
                path: path.to_owned(),
                kind,
                active_song_roots: self.active_song_roots.clone(),
                ln_policy_setting: self.ln_policy,
                rule_mode: self.rule_mode,
                view_generation: self.view_generation,
                data_generation: self.data_generation,
            },
        ));
    }

    fn send_worker_context(&self) {
        let _ = self.request_tx.send(SelectFolderSummaryWorkerCommand::SetContext {
            view_generation: self.view_generation,
            data_generation: self.data_generation,
        });
    }
}

fn spawn_worker(
    library_db_path: PathBuf,
    score_db_path: PathBuf,
    request_rx: Receiver<SelectFolderSummaryWorkerCommand>,
    result_tx: mpsc::Sender<SelectFolderSummaryResult>,
) -> Result<()> {
    thread::Builder::new()
        .name("select-folder-lamp".to_string())
        .spawn(move || {
            let mut databases: Option<(LibraryDatabase, ScoreDatabase)> = None;
            let mut pending = VecDeque::<SelectFolderSummaryRequest>::new();
            let mut view_generation = 0;
            let mut data_generation = 0;

            loop {
                if pending.is_empty() {
                    let Ok(command) = request_rx.recv() else {
                        break;
                    };
                    apply_worker_command(
                        command,
                        &mut pending,
                        &mut view_generation,
                        &mut data_generation,
                    );
                }
                while let Ok(command) = request_rx.try_recv() {
                    apply_worker_command(
                        command,
                        &mut pending,
                        &mut view_generation,
                        &mut data_generation,
                    );
                }

                let Some(request) = pending.pop_front() else {
                    continue;
                };
                if request.view_generation != view_generation
                    || request.data_generation != data_generation
                {
                    continue;
                }

                let started_at = Instant::now();
                let result = (|| -> Result<Option<SelectFolderSummary>> {
                    if databases.is_none() {
                        databases = Some((
                            LibraryDatabase::open(&library_db_path)?,
                            ScoreDatabase::open(&score_db_path)?,
                        ));
                    }
                    let (library_db, score_db) = databases.as_ref().expect("databases initialized");
                    if matches!(request.kind, SelectRowKind::Command | SelectRowKind::NewSongFolder)
                    {
                        let profile_root = score_db_path
                            .parent()
                            .context("score database has no profile directory")?;
                        return crate::screens::select_model::virtual_folder_summary(
                            library_db,
                            score_db,
                            profile_root,
                            &request.path,
                            request.ln_policy_setting,
                            request.rule_mode,
                            &request.active_song_roots,
                        )
                        .map(Some);
                    }
                    select_folder_summary_for_rule_mode(
                        library_db,
                        score_db,
                        &request.path,
                        request.kind,
                        request.ln_policy_setting,
                        request.rule_mode,
                    )
                })()
                .map_err(|error| error.to_string());
                tracing::debug!(
                    target: "bmz_player::select_profile",
                    key = %request.key,
                    elapsed_us = started_at.elapsed().as_micros(),
                    "select folder lamp summary loaded"
                );
                if result_tx
                    .send(SelectFolderSummaryResult {
                        key: request.key,
                        view_generation: request.view_generation,
                        data_generation: request.data_generation,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .context("failed to spawn select folder lamp worker")?;
    Ok(())
}

fn apply_worker_command(
    command: SelectFolderSummaryWorkerCommand,
    pending: &mut VecDeque<SelectFolderSummaryRequest>,
    view_generation: &mut u64,
    data_generation: &mut u64,
) {
    match command {
        SelectFolderSummaryWorkerCommand::SetContext {
            view_generation: next_view,
            data_generation: next_data,
        } => {
            *view_generation = next_view;
            *data_generation = next_data;
            pending.retain(|request| {
                request.view_generation == next_view && request.data_generation == next_data
            });
        }
        SelectFolderSummaryWorkerCommand::Load(request) => {
            if request.view_generation == *view_generation
                && request.data_generation == *data_generation
                && !pending.iter().any(|queued| {
                    queued.key == request.key
                        && queued.view_generation == request.view_generation
                        && queued.data_generation == request.data_generation
                })
            {
                pending.push_back(request);
            }
        }
    }
}

fn cache_key(
    path: &str,
    kind: SelectRowKind,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> String {
    // NEW and the other virtual folders share the same summary query.
    let kind = if kind == SelectRowKind::NewSongFolder { SelectRowKind::Command } else { kind };
    format!("{kind:?}\n{}\n{}\n{path}", ln_policy_setting.as_ir_str(), rule_mode.as_str())
}

fn current_folder_summary_target(path: &str) -> Option<(&str, SelectRowKind)> {
    if path.starts_with(TABLE_ROOT_PATH) {
        return match parse_table_path(path) {
            Some(TablePath::Table { .. } | TablePath::Level { .. }) => {
                Some((path, SelectRowKind::TableFolder))
            }
            _ => None,
        };
    }
    if path.starts_with(SEARCH_PATH_PREFIX) {
        return parse_search_query(path).map(|_| (path, SelectRowKind::SearchFolder));
    }
    if path.starts_with(SAME_FOLDER_PATH_PREFIX) {
        return parse_same_folder_path(path).map(|folder| (folder, SelectRowKind::Folder));
    }
    if path.starts_with(VIRTUAL_FOLDER_PATH_PREFIX) {
        return Some((path, SelectRowKind::Command));
    }
    if path.is_empty()
        || path == COURSE_ROOT_PATH
        || path.starts_with(COURSE_CONTENTS_PATH_PREFIX)
        || path.starts_with(FAVORITE_ROOT_PATH)
        || path.starts_with(CONFIG_ROOT_PATH)
    {
        return None;
    }
    Some((path, SelectRowKind::Folder))
}

fn view_key(folder_stack: &[String]) -> String {
    folder_stack.join("\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        key: &str,
        view_generation: u64,
        data_generation: u64,
    ) -> SelectFolderSummaryRequest {
        SelectFolderSummaryRequest {
            key: key.to_string(),
            path: key.to_string(),
            kind: SelectRowKind::TableFolder,
            active_song_roots: Vec::new(),
            ln_policy_setting: LnPolicySetting::AutoLn,
            rule_mode: RuleMode::Beatoraja,
            view_generation,
            data_generation,
        }
    }

    #[test]
    fn cache_key_separates_score_contexts() {
        let base = cache_key(
            "bmz-table:https://example.com\n1",
            SelectRowKind::TableFolder,
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
        );
        let cn = cache_key(
            "bmz-table:https://example.com\n1",
            SelectRowKind::TableFolder,
            LnPolicySetting::AutoCn,
            RuleMode::Beatoraja,
        );
        let dx = cache_key(
            "bmz-table:https://example.com\n1",
            SelectRowKind::TableFolder,
            LnPolicySetting::AutoLn,
            RuleMode::Dx,
        );

        assert_ne!(base, cn);
        assert_ne!(base, dx);
    }

    #[test]
    fn open_folder_reuses_its_row_summary_and_follows_navigation_and_score_context() {
        let path = "bmz-filter:new/today";
        let ln = LnPolicySetting::AutoLn;
        let rule = RuleMode::Beatoraja;
        let mut runtime = SelectFolderSummaryRuntime::disabled(&[], ln, rule);
        let (request_tx, requests) = mpsc::channel();
        let (results, result_rx) = mpsc::channel();
        runtime.request_tx = request_tx;
        runtime.result_rx = result_rx;
        let mut rows = [SelectItem::Folder {
            path: path.to_owned(),
            name: "Today".to_owned(),
            kind: SelectRowKind::NewSongFolder,
            summary: None,
        }];
        runtime.refresh(&mut rows, &[0], ln, rule, &[], None);
        let SelectFolderSummaryWorkerCommand::Load(request) = requests.try_recv().unwrap() else {
            panic!("expected a request for the NEW folder row");
        };
        runtime.list_replaced(&[path.to_owned()]);
        let summary = SelectFolderSummary { lamp_counts: [3; 11], score_rank_counts: [4; 9] };
        results
            .send(SelectFolderSummaryResult {
                key: request.key,
                view_generation: request.view_generation,
                data_generation: request.data_generation,
                result: Ok(Some(summary.clone())),
            })
            .unwrap();
        runtime.refresh(&mut [], &[], ln, rule, &[], Some(path));
        assert_eq!(runtime.current_folder_summary(path), Some(&summary));
        assert_eq!(runtime.current_folder_summary("bmz-filter:new/yesterday"), None);
        assert!(matches!(
            requests.try_recv(),
            Ok(SelectFolderSummaryWorkerCommand::SetContext { .. })
        ));
        assert!(requests.try_recv().is_err(), "entering the folder must reuse its row summary");

        // The list loader can replace rows without changing the folder stack.
        // Cached results also belong on off-screen rows before any cursor move.
        runtime.list_replaced(&[path.to_owned()]);
        runtime.refresh(&mut rows, &[], ln, rule, &[], None);
        assert!(
            matches!(&rows[0], SelectItem::Folder { summary: Some(value), .. } if value == &summary)
        );

        runtime.refresh(&mut [], &[], ln, RuleMode::Dx, &[], Some(path));
        assert_eq!(runtime.current_folder_summary(path), None);
        assert!(matches!(
            requests.try_recv(),
            Ok(SelectFolderSummaryWorkerCommand::SetContext { .. })
        ));
        assert!(matches!(requests.try_recv(), Ok(SelectFolderSummaryWorkerCommand::Load(_))));
        runtime.invalidate_data();
        runtime.sync_score_context(&mut [], ln, rule);
        assert_eq!(runtime.current_folder_summary(path), None);
    }

    #[test]
    fn worker_drops_old_views_and_deduplicates_requests() {
        let mut pending = VecDeque::new();
        let mut view_generation = 0;
        let mut data_generation = 0;

        apply_worker_command(
            SelectFolderSummaryWorkerCommand::Load(request("sl0", 0, 0)),
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        apply_worker_command(
            SelectFolderSummaryWorkerCommand::Load(request("sl0", 0, 0)),
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        apply_worker_command(
            SelectFolderSummaryWorkerCommand::Load(request("sl1", 0, 0)),
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        assert_eq!(pending.len(), 2);

        apply_worker_command(
            SelectFolderSummaryWorkerCommand::SetContext { view_generation: 1, data_generation: 0 },
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        assert!(pending.is_empty());

        apply_worker_command(
            SelectFolderSummaryWorkerCommand::Load(request("sl1", 1, 0)),
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        apply_worker_command(
            SelectFolderSummaryWorkerCommand::SetContext { view_generation: 1, data_generation: 1 },
            &mut pending,
            &mut view_generation,
            &mut data_generation,
        );
        assert!(pending.is_empty());
    }
}
