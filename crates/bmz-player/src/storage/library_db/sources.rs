use super::*;
use crate::config::app_config::PathEntry;

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
    fn active(&self, roots: Option<&[PathEntry]>) -> bool {
        roots.is_none_or(|roots| {
            let path = self.path.to_string_lossy();
            roots.iter().any(|root| root.enabled && song_root_contains_file(root, &path))
                || (self.root_id.is_none()
                    && !roots.iter().any(|root| song_root_contains_file(root, &path)))
        })
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
    let roots = scope::configured_song_roots(conn)?;
    let mut sources = sources_for_hash(conn, column, hash)?;
    sources.sort_by_key(|source| source.chart.chart_id != preferred.unwrap_or(-1));
    Ok(sources
        .into_iter()
        .find(|source| source.active(roots.as_deref()) && source.readable())
        .map(|source| source.chart.chart_id))
}

impl LibraryDatabase {
    /// Metadata consumers (score/replay import and IR) do not require chart files.
    /// Prefer the newest parser version and active copies, excluding stale copies
    /// from duplicate-consistency checks when a better metadata tier is available.
    pub fn preferred_chart_metadata(&self, sha256: [u8; 32]) -> Result<Vec<ChartSource>> {
        let roots = self.configured_song_roots()?;
        let mut sources = sources_for_hash(&self.conn, "sha256", &hash_to_hex(&sha256))?;
        let rank = |source: &ChartSource| (source.import_version, source.active(roots.as_deref()));
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
        let roots = self.configured_song_roots()?;
        let mut sources = sources_for_hash(&self.conn, "sha256", &hash_to_hex(&sha))?;
        sources.retain(|source| {
            source.active(roots.as_deref())
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
        let rows = self.list_table_entries_with_chart_at_level(source_url, level)?;
        let mut cache: HashMap<[u8; 32], Option<ChartListItem>> = HashMap::new();
        rows.into_iter()
            .map(|mut entry| {
                let available = if let Some(chart) = entry.chart.take() {
                    if let Some(cached) = cache.get(&chart.sha256) {
                        cached.clone()
                    } else {
                        let resolved =
                            self.available_chart_source(chart.chart_id)?.map(|source| source.chart);
                        cache.insert(chart.sha256, resolved.clone());
                        resolved
                    }
                } else {
                    None
                };
                entry.chart = available;
                Ok(entry)
            })
            .collect()
    }
}
