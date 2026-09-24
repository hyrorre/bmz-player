use super::*;

impl LibraryDatabase {
    pub fn upsert_difficulty_table(
        &mut self,
        table: &crate::difficulty_table::FetchedDifficultyTable,
    ) -> Result<i64> {
        super::super::difficulty_table_db::upsert_difficulty_table(&mut self.conn, table)
    }

    /// Replaces one account-owned table snapshot atomically, including courses.
    ///
    /// Validation must happen before this call. If any table or course fails to
    /// store, SQLite rolls back both the new rows and stale-row deletions.
    pub fn replace_account_difficulty_tables(
        &mut self,
        source_prefix: &str,
        tables: &[crate::difficulty_table::FetchedDifficultyTable],
    ) -> Result<(usize, usize)> {
        let current_sources: std::collections::HashSet<&str> =
            tables.iter().map(|table| table.source_url.as_str()).collect();
        if tables.iter().any(|table| !table.source_url.starts_with(source_prefix)) {
            anyhow::bail!("difficulty-table snapshot contains a source outside its account scope");
        }

        let tx = self.conn.transaction()?;
        let stale_sources: Vec<String> =
            super::super::difficulty_table_db::list_difficulty_tables(&tx)?
                .into_iter()
                .filter(|table| {
                    table.source_url.starts_with(source_prefix)
                        && !current_sources.contains(table.source_url.as_str())
                })
                .map(|table| table.source_url)
                .collect();
        for source in stale_sources {
            super::super::difficulty_table_db::delete_difficulty_table(&tx, &source)?;
            super::super::course_db::delete_courses_by_source(&tx, &format!("table:{source}"))?;
        }

        let mut entries = 0;
        for table in tables {
            let course_source = format!("table:{}", table.source_url);
            super::super::course_db::delete_courses_by_source(&tx, &course_source)?;
            super::super::difficulty_table_db::upsert_difficulty_table_in_transaction(&tx, table)?;
            for (position, course) in table.courses.iter().enumerate() {
                super::super::course_db::upsert_course_in_transaction(
                    &tx,
                    &course_source,
                    course,
                    position as i64,
                    table.fetched_at,
                )?;
            }
            entries += table.entries.len();
        }
        tx.commit()?;
        Ok((tables.len(), entries))
    }

    pub fn list_difficulty_tables(&self) -> Result<Vec<DifficultyTableRecord>> {
        super::super::difficulty_table_db::list_difficulty_tables(&self.conn)
    }

    pub fn delete_difficulty_tables_by_source_prefix(&self, source_prefix: &str) -> Result<usize> {
        super::super::difficulty_table_db::delete_difficulty_tables_by_source_prefix(
            &self.conn,
            source_prefix,
        )
    }

    pub fn delete_difficulty_table(&self, source_url: &str) -> Result<bool> {
        super::super::difficulty_table_db::delete_difficulty_table(&self.conn, source_url)
    }

    pub fn delete_courses_by_source(&self, source: &str) -> Result<usize> {
        super::super::course_db::delete_courses_by_source(&self.conn, source)
    }

    pub fn delete_course(&self, course_id: i64) -> Result<bool> {
        super::super::course_db::delete_course(&self.conn, course_id)
    }

    pub fn delete_table_courses_by_source_prefix(&self, source_prefix: &str) -> Result<usize> {
        super::super::course_db::delete_table_courses_by_source_prefix(&self.conn, source_prefix)
    }

    pub fn list_difficulty_table_sources_with_current_download_metadata(
        &self,
    ) -> Result<Vec<String>> {
        super::super::difficulty_table_db::list_difficulty_table_sources_with_current_download_metadata(
            &self.conn,
        )
    }

    pub fn list_difficulty_table_entries_by_md5s(
        &self,
        md5s: &[&str],
    ) -> Result<Vec<DifficultyTableEntryRecord>> {
        super::super::difficulty_table_db::list_entries_by_md5s(&self.conn, md5s)
    }

    pub fn list_difficulty_table_entries_by_sha256s(
        &self,
        sha256s: &[&str],
    ) -> Result<Vec<DifficultyTableEntryRecord>> {
        super::super::difficulty_table_db::list_entries_by_sha256s(&self.conn, sha256s)
    }

    /// Returns every entry of the given difficulty table, including entries that
    /// are not present in the local library.  Matched charts use MD5 first, then
    /// SHA-256.
    pub fn list_table_entries_with_chart(
        &self,
        source_url: &str,
    ) -> Result<Vec<TableEntryListItem>> {
        self.list_table_entries_with_chart_at_level(source_url, None)
    }

    pub fn list_table_entries_with_chart_at_level(
        &self,
        source_url: &str,
        level: Option<&str>,
    ) -> Result<Vec<TableEntryListItem>> {
        let rows = match level {
            Some(level) => super::super::difficulty_table_db::list_table_entries_at_level(
                &self.conn, source_url, level,
            )?,
            None => super::super::difficulty_table_db::list_table_entries(&self.conn, source_url)?,
        };
        let md5_refs: Vec<&str> =
            rows.iter().filter(|row| row.md5.len() >= 24).map(|row| row.md5.as_str()).collect();
        let sha256_refs: Vec<&str> = rows
            .iter()
            .filter(|row| row.sha256.len() >= 24)
            .map(|row| row.sha256.as_str())
            .collect();
        let md5_charts = self.charts_by_md5s(&md5_refs)?;
        let sha256_charts = self.charts_by_sha256s(&sha256_refs)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let chart = md5_charts
                    .get(&row.md5)
                    .cloned()
                    .or_else(|| sha256_charts.get(&row.sha256).cloned());
                TableEntryListItem {
                    level: row.level,
                    md5: row.md5,
                    sha256: row.sha256,
                    title: row.title,
                    artist: row.artist,
                    comment: row.comment,
                    url: row.url,
                    append_url: row.append_url,
                    ipfs: row.ipfs,
                    append_ipfs: row.append_ipfs,
                    chart,
                }
            })
            .collect())
    }

    /// Returns levels that are defined by at least one table entry, regardless
    /// of whether the corresponding chart exists in the local library.
    pub fn list_table_entry_levels(&self, source_url: &str) -> Result<Vec<String>> {
        super::super::difficulty_table_db::list_table_entry_levels(&self.conn, source_url)
    }

    pub fn find_table_entry_by_hash(
        &self,
        source_url: &str,
        md5: Option<&str>,
        sha256: Option<&str>,
    ) -> Result<Option<TableEntryRow>> {
        super::super::difficulty_table_db::find_table_entry_by_hash(
            &self.conn, source_url, md5, sha256,
        )
    }

    pub(super) fn charts_by_md5s(&self, md5s: &[&str]) -> Result<HashMap<String, ChartListItem>> {
        charts_by_hash_column(&self.conn, "md5", md5s)
    }

    pub(super) fn charts_by_sha256s(
        &self,
        sha256s: &[&str],
    ) -> Result<HashMap<String, ChartListItem>> {
        charts_by_hash_column(&self.conn, "sha256", sha256s)
    }

    pub fn upsert_course(
        &mut self,
        source: &str,
        course: &bmz_core::course::CourseDefinition,
        source_position: i64,
        imported_at: i64,
    ) -> Result<i64> {
        super::super::course_db::upsert_course(
            &mut self.conn,
            source,
            course,
            source_position,
            imported_at,
        )
    }

    pub fn list_courses(&self) -> Result<Vec<StoredCourse>> {
        super::super::course_db::list_courses(&self.conn)
    }

    pub fn course_by_id(&self, course_id: i64) -> Result<Option<StoredCourse>> {
        super::super::course_db::course_by_id(&self.conn, course_id)
    }

    pub fn repair_course_entry_chart_links_for_course(&self, course_id: i64) -> Result<usize> {
        super::super::course_db::repair_course_entry_chart_links_for_course(&self.conn, course_id)
    }

    pub fn list_courses_by_source(&self, source: &str) -> Result<Vec<StoredCourse>> {
        super::super::course_db::list_courses_by_source(&self.conn, source)
    }

    pub fn list_course_entries(&self, course_id: i64) -> Result<Vec<StoredCourseEntry>> {
        super::super::course_db::list_course_entries(&self.conn, course_id)
    }

    /// Returns `(ChartListItem, raw_level)` pairs for charts in the library that
    /// appear in the given difficulty table, matched first by MD5 then by SHA-256.
    /// Charts not present in the local library are omitted.
    ///
    /// Prefer [`Self::list_table_entries_with_chart`] when table entries without a
    /// local chart should be included.
    pub fn list_charts_with_level_in_table(
        &self,
        source_url: &str,
    ) -> Result<Vec<(ChartListItem, String)>> {
        // Use UNION (not UNION ALL) so that a chart matched by both MD5 and SHA-256
        // for the same entry only appears once.
        let sql = format!(
            "
            SELECT {CHART_LIST_ITEM_COLUMNS_C}, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON c.md5 = dte.md5
            WHERE dt.source_url = ?1 AND length(dte.md5) >= 24
            UNION
            SELECT {CHART_LIST_ITEM_COLUMNS_C}, dte.level
            FROM difficulty_table_entries dte
            JOIN difficulty_tables dt ON dt.id = dte.table_id
            JOIN charts c ON c.sha256 = dte.sha256
            WHERE dt.source_url = ?1 AND length(dte.sha256) >= 24"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![source_url], |row| {
            let chart = chart_list_item_from_row(row)?;
            let level: String = row.get("level")?;
            Ok((chart, level))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseKind};
    use rusqlite::Connection;

    use super::*;
    use crate::difficulty_table::FetchedDifficultyTable;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn open_db() -> LibraryDatabase {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        LibraryDatabase::from_connection(conn)
    }

    fn table(source_url: &str) -> FetchedDifficultyTable {
        FetchedDifficultyTable {
            source_url: source_url.to_string(),
            head_url: "https://example.test/header.json".to_string(),
            name: source_url.to_string(),
            symbol: String::new(),
            level_order: Vec::new(),
            entries: Vec::new(),
            courses: Vec::new(),
            fetched_at: 1,
        }
    }

    #[test]
    fn account_table_snapshot_rolls_back_stale_deletes_when_a_course_fails() {
        let prefix = "bmz-bms-ir-table:test:";
        let mut db = open_db();
        let old = table(&format!("{prefix}old"));
        db.replace_account_difficulty_tables(prefix, std::slice::from_ref(&old)).unwrap();

        let mut invalid = table(&format!("{prefix}new"));
        invalid.courses.push(CourseDefinition {
            key: "course".to_string(),
            title: "Broken course".to_string(),
            kind: CourseKind::Course,
            entries: vec![CourseEntry {
                title_hint: "Missing chart".to_string(),
                md5: None,
                sha256: None,
                chart_id: Some(i64::MAX),
            }],
            constraints: CourseConstraints::default(),
            trophies: Vec::new(),
            release: true,
        });

        assert!(db.replace_account_difficulty_tables(prefix, &[invalid]).is_err());
        let sources = db
            .list_difficulty_tables()
            .unwrap()
            .into_iter()
            .map(|table| table.source_url)
            .collect::<Vec<_>>();
        assert_eq!(sources, vec![old.source_url]);
    }
}
