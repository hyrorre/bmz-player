use super::*;

impl LibraryDatabase {
    /// Only remove confirmed missing files within a successfully scanned scope.
    /// Do not infer deletion from timestamps: unchanged imports keep old timestamps.
    pub fn prune_missing_chart_files(&mut self, root: &Path, recursive: bool) -> Result<usize> {
        let root = crate::config::app_config::PathEntry {
            path: library_path_key(root),
            enabled: true,
            recursive,
        };
        let candidates = {
            let mut stmt = self.conn.prepare("SELECT id, path FROM chart_files")?;
            stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let missing: Vec<_> = candidates
            .into_iter()
            .filter(|(_, path)| {
                song_root_contains_file(&root, path)
                    && matches!(Path::new(path).try_exists(), Ok(false))
            })
            .map(|(id, _)| id)
            .collect();
        if missing.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        delete_chart_files(&tx, &missing)?;
        tx.commit()?;
        Ok(missing.len())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        configure_connection(&conn)?;
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
