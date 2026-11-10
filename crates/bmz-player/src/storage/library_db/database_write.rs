use super::*;

impl LibraryDatabase {
    /// Every registered chart path with its [`scope_key`], loaded once per scan so that
    /// pruning many roots neither re-reads nor re-normalizes the whole table per root.
    pub(crate) fn chart_path_index(&self) -> Result<ChartPathIndex> {
        let mut stmt = self.conn.prepare("SELECT id, path FROM chart_files")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .map(|row| {
                row.map(|(id, path)| {
                    let key = scope_key(&path);
                    IndexedChartPath { id, path, key }
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ChartPathIndex { rows })
    }

    /// Only remove confirmed missing files within a successfully scanned scope.
    /// Do not infer deletion from timestamps: unchanged imports keep old timestamps.
    ///
    /// A registration that any root of this scan discovered is present and kept. A
    /// chart deleted after its discovery is removed by the next scan, not by a second
    /// stat of every chart here. For the rest, a complete native discovery
    /// (`omission_proves_absence`) shows that the registration is gone or outside the
    /// scope, such as an unchosen link alias; hidden entries and other root scopes can
    /// still retain it. Otherwise (an index such as Everything may lag) only the file
    /// system or a decoded archive index decides, and only `Ok(false)` proves absence.
    pub(crate) fn prune_missing_chart_files(
        &mut self,
        index: &mut ChartPathIndex,
        request: &PruneRequest<'_>,
    ) -> Result<usize> {
        use rayon::prelude::*;

        let root_path = library_path_key(request.root);
        let root = RootScope::new(&root_path, request.recursive);
        let root_key = scope_key(&root_path);
        let mut missing = Vec::new();
        let mut files_to_check = Vec::new();
        // Only a successfully decoded generation may prove that an entry vanished.
        // Rechecking the generation also protects against rewrites after the scan.
        let mut archive_entries: HashMap<PathBuf, Option<std::collections::HashSet<String>>> =
            HashMap::new();
        let mut containers_gone: HashMap<PathBuf, bool> = HashMap::new();
        // A root cannot protect its own omitted entries. A root enclosing this one
        // (a configured library above a partial scan) knows nothing about this scope
        // that the scan did not just read, so it cannot keep a confirmed absence.
        // Any other scope (an offline, disabled, unscanned or nested root) can.
        let mut enclosing_roots = Vec::new();
        let mut other_roots = Vec::new();
        for other in request.protected_roots {
            let other = RootScope::of(other);
            if other.is_root_key(&root_key) {
                continue;
            }
            if other.contains_key(&root_key) {
                enclosing_roots.push(other);
            } else {
                other_roots.push(other);
            }
        }
        for IndexedChartPath { id, path, key } in &index.rows {
            if !root.contains_key(key) || request.discovered_any.contains(path) {
                continue;
            }
            let locator = crate::chart_source::ChartLocator::parse(Path::new(path));
            let exact_archive = root.is_root_key(key)
                && matches!(locator, Ok(crate::chart_source::ChartLocator::Archive { .. }));
            if !exact_archive && other_roots.iter().any(|other| other.contains_key(key)) {
                // An offline, disabled, or unscanned root cannot prove absence.
                continue;
            }
            // The enclosing root's own scan may have chosen a different alias, or
            // kept a hidden chart, so only the file system or a decoded archive
            // index may remove its registrations, never the omission alone.
            let enclosed =
                !exact_archive && enclosing_roots.iter().any(|other| other.contains_key(key));
            if request.omission_proves_absence && !enclosed {
                // The hidden setting only restricts new discovery. Keep existing
                // hidden files, as before this scan optimization.
                if request.skip_hidden
                    && let Ok(locator) = &locator
                {
                    let container = locator.container_path();
                    match container.try_exists() {
                        Err(_) => continue,
                        Ok(true) => {
                            if crate::storage::scan::hidden_path_in_root(
                                Path::new(&root_path),
                                container,
                            ) != Some(false)
                            {
                                continue;
                            }
                            if let crate::chart_source::ChartLocator::Archive { entry, .. } =
                                locator
                            {
                                let hidden = !cfg!(windows)
                                    && entry.split('/').any(|part| part.starts_with('.'));
                                if hidden {
                                    let current = crate::song_archive::inspect(
                                        container,
                                        &Default::default(),
                                    )
                                    .ok();
                                    let generation: Option<String> = self.conn.query_row(
                                            "SELECT generation FROM song_archive_scans WHERE path = ?1",
                                            [library_path_key(container)], |row| row.get(0),
                                        ).ok();
                                    if current.as_ref().is_none_or(|index| {
                                        generation.as_deref()
                                            != Some(index.generation.fingerprint.as_str())
                                            || index.entries.iter().any(|item| {
                                                !item.is_directory && item.name == *entry
                                            })
                                    }) {
                                        continue;
                                    }
                                }
                            }
                        }
                        Ok(false) => {}
                    }
                }
                missing.push(*id);
                continue;
            }
            match locator {
                Ok(crate::chart_source::ChartLocator::File(file)) => {
                    files_to_check.push((*id, file))
                }
                Ok(crate::chart_source::ChartLocator::Archive { container, entry }) => {
                    let gone = *containers_gone
                        .entry(container.clone())
                        .or_insert_with(|| matches!(container.try_exists(), Ok(false)));
                    let vanished = gone
                        || archive_entries
                            .entry(container.clone())
                            .or_insert_with(|| {
                                let index =
                                    crate::song_archive::inspect(&container, &Default::default())
                                        .ok()?;
                                let generation: String = self
                                    .conn
                                    .query_row(
                                        "SELECT generation FROM song_archive_scans WHERE path = ?1",
                                        [library_path_key(&container)],
                                        |row| row.get(0),
                                    )
                                    .ok()?;
                                (generation == index.generation.fingerprint).then(|| {
                                    index
                                        .entries
                                        .into_iter()
                                        .filter(|entry| !entry.is_directory)
                                        .map(|entry| entry.name)
                                        .collect()
                                })
                            })
                            .as_ref()
                            .is_some_and(|entries| !entries.contains(&entry));
                    if vanished {
                        missing.push(*id);
                    }
                }
                Err(_) => {}
            }
        }
        // One stat per undiscovered registration, in parallel for slow or remote volumes.
        missing.extend(
            files_to_check
                .par_iter()
                .filter(|(_, file)| matches!(file.try_exists(), Ok(false)))
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
        );
        if missing.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        inherit_first_seen_at(&tx, &missing)?;
        delete_chart_files(&tx, &missing)?;
        tx.commit()?;
        let removed: std::collections::HashSet<i64> = missing.iter().copied().collect();
        index.rows.retain(|row| !removed.contains(&row.id));
        Ok(missing.len())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        configure_connection(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_read_only(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(std::time::Duration::from_millis(100))?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub(crate) fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// トランザクションを管理せずにチャートをupsertする。
    /// `conn` にはアクティブなトランザクション（またはコネクション）を渡す。
    /// 戻り値は `(chart_id, chart_file_id)`。
    pub fn write_chart_import(
        conn: &Connection,
        record: &ChartImportRecord<'_>,
    ) -> Result<(i64, i64)> {
        let chart_file_id = upsert_chart_file(conn, record)?;

        let existing_chart_id: Option<i64> = conn
            .query_row(
                "SELECT chart_id FROM chart_file_links WHERE chart_file_id = ?1",
                params![chart_file_id],
                |row| row.get(0),
            )
            .optional()?;

        let chart_id = if let Some(existing_id) = existing_chart_id {
            update_chart(conn, existing_id, record)?;
            existing_id
        } else {
            let new_id = insert_chart(conn, record)?;
            conn.execute(
                "INSERT INTO chart_file_links (chart_id, chart_file_id) VALUES (?1, ?2)",
                params![new_id, chart_file_id],
            )?;
            new_id
        };

        write_chart_analysis(conn, chart_id, record.chart)?;
        super::super::course_db::refresh_course_entries_for_chart(
            conn,
            &hash_to_hex(&record.chart.identity.file_sha256),
            &hash_to_hex(&record.chart.identity.file_md5),
        )?;

        Ok((chart_id, chart_file_id))
    }

    pub fn upsert_chart_import(&mut self, record: &ChartImportRecord<'_>) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let (chart_id, _) = Self::write_chart_import(&tx, record)?;
        tx.commit()?;
        Ok(chart_id)
    }

    pub fn chart_id_by_title(&self, title: &str) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT id FROM charts WHERE title = ?1 LIMIT 1", params![title], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Into::into)
    }

    pub fn chart_id_by_sha256(&self, sha256: [u8; 32]) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM charts WHERE sha256 = ?1 LIMIT 1",
                params![hash_to_hex(&sha256)],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
}

/// A chart that moved, or whose alias path was replaced by the chosen one, keeps its
/// place in "new songs": remaining files with the same hash take the earliest
/// `first_seen_at` of the removed ones.
fn inherit_first_seen_at(tx: &Connection, removed_file_ids: &[i64]) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "UPDATE chart_files
         SET first_seen_at = removed.first_seen_at
         FROM (SELECT sha256, first_seen_at FROM chart_files WHERE id = ?1) AS removed
         WHERE chart_files.sha256 = removed.sha256
           AND removed.sha256 <> ''
           AND chart_files.first_seen_at > removed.first_seen_at",
    )?;
    for &file_id in removed_file_ids {
        stmt.execute([file_id])?;
    }
    Ok(())
}

pub(super) fn delete_chart_files(tx: &Connection, file_ids: &[i64]) -> Result<()> {
    for &file_id in file_ids {
        let chart_ids = {
            let mut stmt =
                tx.prepare("SELECT chart_id FROM chart_file_links WHERE chart_file_id = ?1")?;
            stmt.query_map([file_id], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute("DELETE FROM chart_import_warnings WHERE chart_file_id = ?1", [file_id])?;
        tx.execute("DELETE FROM chart_file_links WHERE chart_file_id = ?1", [file_id])?;
        tx.execute("DELETE FROM chart_files WHERE id = ?1", [file_id])?;
        for chart_id in chart_ids {
            let linked: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM chart_file_links WHERE chart_id = ?1)",
                [chart_id],
                |row| row.get(0),
            )?;
            if !linked {
                tx.execute(
                    "UPDATE course_entries SET chart_id = NULL WHERE chart_id = ?1",
                    [chart_id],
                )?;
                tx.execute("DELETE FROM charts WHERE id = ?1", [chart_id])?;
            }
        }
    }
    if !file_ids.is_empty() {
        super::super::course_db::repair_course_entry_chart_links_with_stats(tx)?;
    }
    Ok(())
}
