use super::*;

#[cfg(test)]
mod tests;
use anyhow::{Context, bail};
use bmz_chart::import::{BmsRandomSource, ImportResult, import_bms_chart_with_random_source};

/// A single copy's identity, assets and metadata must travel together.
#[derive(Debug, Clone)]
pub struct ChartSource {
    pub chart: ChartListItem,
    pub path: PathBuf,
    pub import_version: i64,
    root_id: Option<i64>,
}

impl ChartSource {
    fn active(&self, roots: &scope::SongRootScope) -> bool {
        if roots.is_unrestricted() {
            return true;
        }
        let path = self.path.to_string_lossy();
        roots.contains_file_in_enabled_root(&path)
            || (!roots.contains_file(&path)
                && (self.root_id.is_none() || roots.contains_file_in_partial_scan(&path)))
    }

    fn readable(&self) -> bool {
        self.path.is_file() && std::fs::File::open(&self.path).is_ok()
    }
}

fn sources_for_hash(conn: &Connection, column: &str, hash: &str) -> Result<Vec<ChartSource>> {
    debug_assert!(matches!(column, "sha256" | "md5"));
    let mut stmt = conn.prepare(&format!(
        "SELECT {CHART_LIST_ITEM_COLUMNS_C}, f.path, f.root_id, c.import_version
         FROM charts c
         JOIN chart_file_links l ON l.chart_id = c.id
         JOIN chart_files f ON f.id = l.chart_file_id
         WHERE c.{column} = ?1
         ORDER BY c.import_version DESC, c.id DESC, f.path COLLATE NOCASE"
    ))?;
    let rows = stmt.query_map([hash], |row| {
        Ok(ChartSource {
            chart: chart_list_item_from_row(row)?,
            path: PathBuf::from(row.get::<_, String>(36)?),
            root_id: row.get(37)?,
            import_version: row.get(38)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub(crate) fn available_chart_id_for_hash(
    conn: &Connection,
    column: &str,
    hash: &str,
    preferred: Option<i64>,
) -> Result<Option<i64>> {
    let roots = scope::SongRootScope::load(conn)?;
    let mut sources = sources_for_hash(conn, column, hash)?;
    sources.sort_by_key(|source| source.chart.chart_id != preferred.unwrap_or(-1));
    Ok(sources
        .into_iter()
        .find(|source| source.active(&roots) && source.readable())
        .map(|source| source.chart.chart_id))
}

impl LibraryDatabase {
    /// Resolve a list in batches, sharing root lookup and file checks for duplicates.
    pub fn available_chart_sources(
        &self,
        charts: &[&ChartListItem],
    ) -> Result<HashMap<i64, ChartSource>> {
        self.chart_sources_for_list(charts, true)
    }

    /// Use the last successful scan's file registrations, without touching song disks.
    /// Missing registrations are pruned by complete scans; playback still verifies files.
    pub fn registered_chart_sources(
        &self,
        charts: &[&ChartListItem],
    ) -> Result<HashMap<i64, ChartSource>> {
        self.chart_sources_for_list(charts, false)
    }

    fn chart_sources_for_list(
        &self,
        charts: &[&ChartListItem],
        verify_files: bool,
    ) -> Result<HashMap<i64, ChartSource>> {
        if charts.is_empty() {
            return Ok(HashMap::new());
        }
        let roots = scope::SongRootScope::load(&self.conn)?;
        let mut hashes: Vec<_> = charts.iter().map(|chart| hash_to_hex(&chart.sha256)).collect();
        hashes.sort_unstable();
        hashes.dedup();
        let mut candidates: HashMap<[u8; 32], Vec<ChartSource>> = HashMap::new();
        for chunk in hashes.chunks(CHART_HASH_LOOKUP_BATCH_SIZE) {
            let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(",");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {CHART_LIST_ITEM_COLUMNS_C}, f.path, f.root_id, c.import_version
                 FROM charts c
                 JOIN chart_file_links l ON l.chart_id = c.id
                 JOIN chart_files f ON f.id = l.chart_file_id
                 WHERE c.sha256 IN ({placeholders})
                 ORDER BY c.import_version DESC, c.id DESC, f.path COLLATE NOCASE"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |row| {
                Ok(ChartSource {
                    chart: chart_list_item_from_row(row)?,
                    path: PathBuf::from(row.get::<_, String>(36)?),
                    root_id: row.get(37)?,
                    import_version: row.get(38)?,
                })
            })?;
            for row in rows {
                let source = row?;
                if source.active(&roots) {
                    candidates.entry(source.chart.sha256).or_default().push(source);
                }
            }
        }
        let mut readable = HashMap::new();
        let mut resolved = HashMap::new();
        for chart in charts {
            if let Some(sources) = candidates.get(&chart.sha256) {
                // Preserve each requested copy's assets, even for identical hashes.
                let preferred = sources.iter().filter(|s| s.chart.chart_id == chart.chart_id);
                let fallback = sources.iter().filter(|s| s.chart.chart_id != chart.chart_id);
                if let Some(source) = preferred.chain(fallback).find(|source| {
                    !verify_files
                        || *readable.entry(source.path.clone()).or_insert_with(|| source.readable())
                }) {
                    resolved.insert(chart.chart_id, source.clone());
                }
            }
        }
        Ok(resolved)
    }

    /// Metadata consumers (score/replay import and IR) do not require chart files.
    /// Prefer the newest parser version and active copies, excluding stale copies
    /// from duplicate-consistency checks when a better metadata tier is available.
    pub fn preferred_chart_metadata(&self, sha256: [u8; 32]) -> Result<Vec<ChartSource>> {
        let roots = scope::SongRootScope::load(&self.conn)?;
        let mut sources = sources_for_hash(&self.conn, "sha256", &hash_to_hex(&sha256))?;
        let rank = |source: &ChartSource| (source.import_version, source.active(&roots));
        if let Some(best) = sources.iter().map(&rank).max() {
            sources.retain(|source| rank(source) == best);
        }
        sources.sort_by_cached_key(|source| {
            (!source.readable(), std::cmp::Reverse(source.chart.chart_id))
        });
        sources.dedup_by_key(|source| source.chart.chart_id);
        Ok(sources)
    }

    pub fn available_chart_id_by_sha256(&self, sha256: [u8; 32]) -> Result<Option<i64>> {
        available_chart_id_for_hash(&self.conn, "sha256", &hash_to_hex(&sha256), None)
    }

    pub fn available_chart_source(&self, chart_id: i64) -> Result<Option<ChartSource>> {
        Ok(self
            .chart_source_candidates(chart_id, false)?
            .into_iter()
            .find(|source| source.readable()))
    }

    /// Resolve the copy before creating a play/skin session. A readable file may
    /// have been overwritten since scanning, so availability alone is insufficient.
    pub fn verified_chart_source(&self, chart_id: i64) -> Result<ChartSource> {
        let mut errors = Vec::new();
        for source in self.chart_source_candidates(chart_id, true)? {
            match std::fs::read(&source.path) {
                Ok(bytes)
                    if bmz_chart::hash::compute_chart_identity(&bytes).file_sha256
                        == source.chart.sha256 =>
                {
                    return Ok(source);
                }
                Ok(_) => errors
                    .push(format!("{}: file hash changed; rescan library", source.path.display())),
                Err(error) => errors.push(format!("{}: {error}", source.path.display())),
            }
        }
        bail!(
            "no usable chart file for chart {chart_id}: {}",
            if errors.is_empty() {
                "no copy in enabled song roots".to_string()
            } else {
                errors.join("; ")
            }
        )
    }

    fn chart_source_candidates(
        &self,
        chart_id: i64,
        explicit_import: bool,
    ) -> Result<Vec<ChartSource>> {
        let Some(sha) = self.chart_sha256_by_chart_id(chart_id)? else {
            return Ok(Vec::new());
        };
        let roots = scope::SongRootScope::load(&self.conn)?;
        let mut sources = sources_for_hash(&self.conn, "sha256", &hash_to_hex(&sha))?;
        sources.retain(|source| {
            source.active(&roots)
                || (explicit_import
                    && source.chart.chart_id == chart_id
                    && source.root_id.is_none())
        });
        // Preserve the selected folder and its assets when that copy is usable.
        sources.sort_by_key(|source| source.chart.chart_id != chart_id);
        Ok(sources)
    }

    /// Recheck at load time, including the file hash, and try another copy on failure.
    pub fn load_chart_source(
        &self,
        chart_id: i64,
        random: BmsRandomSource,
    ) -> Result<(ChartSource, ImportResult)> {
        let mut errors = Vec::new();
        for source in self.chart_source_candidates(chart_id, true)? {
            let loaded = import_bms_chart_with_random_source(&source.path, random.clone(), true)
                .with_context(|| format!("failed to read chart {}", source.path.display()));
            match loaded {
                Ok(import) if import.chart.identity.file_sha256 == source.chart.sha256 => {
                    return Ok((source, import));
                }
                Ok(_) => errors
                    .push(format!("{}: file hash changed; rescan library", source.path.display())),
                Err(error) => errors.push(format!("{error:#}")),
            }
        }
        bail!(
            "no usable chart file for chart {chart_id}: {}",
            if errors.is_empty() {
                "no copy in enabled song roots".to_string()
            } else {
                errors.join("; ")
            }
        )
    }

    /// Resolve at list refresh, never from the per-frame renderer.
    pub fn available_table_entries_at_level(
        &self,
        source_url: &str,
        level: Option<&str>,
    ) -> Result<Vec<TableEntryListItem>> {
        self.table_entries_with_sources(source_url, level, true)
    }

    pub fn registered_table_entries_at_level(
        &self,
        source_url: &str,
        level: Option<&str>,
    ) -> Result<Vec<TableEntryListItem>> {
        self.table_entries_with_sources(source_url, level, false)
    }

    fn table_entries_with_sources(
        &self,
        source_url: &str,
        level: Option<&str>,
        verify_files: bool,
    ) -> Result<Vec<TableEntryListItem>> {
        let rows = self.list_table_entries_with_chart_at_level(source_url, level)?;
        let charts: Vec<_> = rows.iter().filter_map(|entry| entry.chart.as_ref()).collect();
        let sources = self.chart_sources_for_list(&charts, verify_files)?;
        rows.into_iter()
            .map(|mut entry| {
                entry.chart = entry.chart.and_then(|chart| {
                    sources.get(&chart.chart_id).map(|source| source.chart.clone())
                });
                Ok(entry)
            })
            .collect()
    }
}
