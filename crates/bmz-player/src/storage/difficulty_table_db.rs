use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::difficulty_table::FetchedDifficultyTable;

const DOWNLOAD_METADATA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct DifficultyTableRecord {
    pub id: i64,
    pub source_url: String,
    pub name: String,
    pub symbol: String,
    pub level_order: Vec<String>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone)]
pub struct DifficultyTableEntryRecord {
    pub source_url: String,
    pub table_name: String,
    pub table_symbol: String,
    pub level: String,
    pub md5: String,
    pub sha256: String,
}

/// Raw difficulty-table entry metadata stored in `library.db`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntryRow {
    pub level: String,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub url: String,
    pub append_url: String,
    pub ipfs: String,
    pub append_ipfs: String,
}

pub(super) fn upsert_difficulty_table(
    conn: &mut Connection,
    table: &FetchedDifficultyTable,
) -> Result<i64> {
    let tx = conn.transaction()?;
    let table_id = upsert_difficulty_table_in_transaction(&tx, table)?;
    tx.commit()?;
    Ok(table_id)
}

pub(super) fn upsert_difficulty_table_in_transaction(
    conn: &Connection,
    table: &FetchedDifficultyTable,
) -> Result<i64> {
    let level_order_json = serde_json::to_string(&table.level_order)?;

    conn.execute(
        "INSERT INTO difficulty_tables (
             source_url, head_url, name, symbol, level_order, fetched_at,
             download_metadata_version
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source_url) DO UPDATE SET
             head_url = excluded.head_url,
             name = excluded.name,
             symbol = excluded.symbol,
             level_order = excluded.level_order,
             fetched_at = excluded.fetched_at,
             download_metadata_version = excluded.download_metadata_version",
        params![
            table.source_url,
            table.head_url,
            table.name,
            table.symbol,
            level_order_json,
            table.fetched_at,
            DOWNLOAD_METADATA_VERSION,
        ],
    )?;

    let table_id: i64 = conn.query_row(
        "SELECT id FROM difficulty_tables WHERE source_url = ?1",
        params![table.source_url],
        |row| row.get(0),
    )?;

    conn.execute("DELETE FROM difficulty_table_entries WHERE table_id = ?1", params![table_id])?;

    for entry in &table.entries {
        conn.execute(
            "INSERT INTO difficulty_table_entries
             (table_id, level, md5, sha256, title, artist, comment,
              url, append_url, ipfs, append_ipfs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                table_id,
                entry.level,
                entry.md5,
                entry.sha256,
                entry.title,
                entry.artist,
                entry.comment,
                entry.url,
                entry.append_url,
                entry.ipfs,
                entry.append_ipfs,
            ],
        )?;
    }

    Ok(table_id)
}

pub(super) fn list_difficulty_tables(conn: &Connection) -> Result<Vec<DifficultyTableRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_url, name, symbol, level_order, fetched_at
         FROM difficulty_tables ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (id, source_url, name, symbol, level_order_json, fetched_at) = row?;
        let level_order: Vec<String> = serde_json::from_str(&level_order_json).unwrap_or_default();
        result.push(DifficultyTableRecord {
            id,
            source_url,
            name,
            symbol,
            level_order,
            fetched_at,
        });
    }
    Ok(result)
}

pub(super) fn delete_difficulty_tables_by_source_prefix(
    conn: &Connection,
    source_prefix: &str,
) -> Result<usize> {
    let escaped = source_prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    Ok(conn.execute(
        "DELETE FROM difficulty_tables WHERE source_url LIKE ?1 ESCAPE '\\'",
        params![format!("{escaped}%")],
    )?)
}

pub(super) fn delete_difficulty_table(conn: &Connection, source_url: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM difficulty_tables WHERE source_url = ?1", params![source_url])?
        > 0)
}

pub(super) fn list_difficulty_table_sources_with_current_download_metadata(
    conn: &Connection,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT source_url
         FROM difficulty_tables
         WHERE download_metadata_version = ?1",
    )?;
    let rows = stmt.query_map(params![DOWNLOAD_METADATA_VERSION], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

/// Lists every entry of the given difficulty table without joining local charts.
pub(super) fn list_table_entries(
    conn: &Connection,
    source_url: &str,
) -> Result<Vec<TableEntryRow>> {
    let sql = "
        SELECT dte.level, dte.md5, dte.sha256, dte.title, dte.artist, dte.comment,
               dte.url, dte.append_url, dte.ipfs, dte.append_ipfs
        FROM difficulty_table_entries dte
        JOIN difficulty_tables dt ON dt.id = dte.table_id
        WHERE dt.source_url = ?1";
    query_table_entries(conn, sql, params![source_url])
}

/// Lists entries at one level of a difficulty table without joining local charts.
pub(super) fn list_table_entries_at_level(
    conn: &Connection,
    source_url: &str,
    level: &str,
) -> Result<Vec<TableEntryRow>> {
    let sql = "
        SELECT dte.level, dte.md5, dte.sha256, dte.title, dte.artist, dte.comment,
               dte.url, dte.append_url, dte.ipfs, dte.append_ipfs
        FROM difficulty_table_entries dte
        JOIN difficulty_tables dt ON dt.id = dte.table_id
        WHERE dt.source_url = ?1 AND dte.level = ?2";
    query_table_entries(conn, sql, params![source_url, level])
}

/// Lists levels that have at least one entry in the table definition.
pub(super) fn list_table_entry_levels(conn: &Connection, source_url: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT dte.level
         FROM difficulty_table_entries dte
         JOIN difficulty_tables dt ON dt.id = dte.table_id
         WHERE dt.source_url = ?1",
    )?;
    let rows = stmt.query_map(params![source_url], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

/// Finds download metadata for one course entry in its source difficulty table.
pub(super) fn find_table_entry_by_hash(
    conn: &Connection,
    source_url: &str,
    md5: Option<&str>,
    sha256: Option<&str>,
) -> Result<Option<TableEntryRow>> {
    let md5 = md5.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("");
    let sha256 = sha256.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("");
    if md5.is_empty() && sha256.is_empty() {
        return Ok(None);
    }
    let sql = "
        SELECT dte.level, dte.md5, dte.sha256, dte.title, dte.artist, dte.comment,
               dte.url, dte.append_url, dte.ipfs, dte.append_ipfs
        FROM difficulty_table_entries dte
        JOIN difficulty_tables dt ON dt.id = dte.table_id
        WHERE dt.source_url = ?1
          AND ((?2 <> '' AND lower(dte.md5) = lower(?2))
            OR (?3 <> '' AND lower(dte.sha256) = lower(?3)))
        ORDER BY CASE WHEN ?2 <> '' AND lower(dte.md5) = lower(?2) THEN 0 ELSE 1 END
        LIMIT 1";
    conn.query_row(sql, params![source_url, md5, sha256], table_entry_from_row)
        .optional()
        .map_err(Into::into)
}

fn query_table_entries(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<TableEntryRow>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, table_entry_from_row)?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

fn table_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TableEntryRow> {
    Ok(TableEntryRow {
        level: row.get(0)?,
        md5: row.get(1)?,
        sha256: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        comment: row.get(5)?,
        url: row.get(6)?,
        append_url: row.get(7)?,
        ipfs: row.get(8)?,
        append_ipfs: row.get(9)?,
    })
}

pub(super) fn list_entries_by_md5s(
    conn: &Connection,
    md5s: &[&str],
) -> Result<Vec<DifficultyTableEntryRecord>> {
    list_entries_by_hash_column(conn, "md5", md5s)
}

pub(super) fn list_entries_by_sha256s(
    conn: &Connection,
    sha256s: &[&str],
) -> Result<Vec<DifficultyTableEntryRecord>> {
    list_entries_by_hash_column(conn, "sha256", sha256s)
}

/// Above this many hashes, reading every entry once beats one index probe per hash
/// (48k entries: 30-50 ms to read, against ~300 ms of probes for a 68k-chart folder).
const FULL_SCAN_HASH_COUNT: usize = 4096;

/// Look up table entries by a single hash column (`md5` or `sha256`).
///
/// Small lists reuse one prepared `WHERE dte.<col> = ?` per hash, which also stays under
/// SQLite's per-statement variable limit. A folder can hold tens of thousands of BMS
/// files; then every entry is read once and filtered in memory, in the same per-hash
/// entry order.
fn list_entries_by_hash_column(
    conn: &Connection,
    column: &'static str,
    hashes: &[&str],
) -> Result<Vec<DifficultyTableEntryRecord>> {
    debug_assert!(matches!(column, "md5" | "sha256"));
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let entry = |row: &rusqlite::Row<'_>| {
        Ok(DifficultyTableEntryRecord {
            source_url: row.get(0)?,
            table_name: row.get(1)?,
            table_symbol: row.get(2)?,
            level: row.get(3)?,
            md5: row.get(4)?,
            sha256: row.get(5)?,
        })
    };
    let mut result = Vec::new();
    if hashes.len() >= FULL_SCAN_HASH_COUNT {
        let wanted: std::collections::HashSet<&str> = hashes.iter().copied().collect();
        let mut stmt = conn.prepare_cached(
            "SELECT dt.source_url, dt.name, dt.symbol, dte.level, dte.md5, dte.sha256
             FROM difficulty_table_entries dte
             JOIN difficulty_tables dt ON dt.id = dte.table_id
             ORDER BY dte.rowid",
        )?;
        for row in stmt.query_map([], entry)? {
            let row = row?;
            let hash = if column == "md5" { &row.md5 } else { &row.sha256 };
            if wanted.contains(hash.as_str()) {
                result.push(row);
            }
        }
        return Ok(result);
    }
    let sql = format!(
        "SELECT dt.source_url, dt.name, dt.symbol, dte.level, dte.md5, dte.sha256
         FROM difficulty_table_entries dte
         JOIN difficulty_tables dt ON dt.id = dte.table_id
         WHERE dte.{column} = ?1
         ORDER BY dte.rowid"
    );
    let mut stmt = conn.prepare(&sql)?;
    for hash in hashes {
        for row in stmt.query_map(params![hash], entry)? {
            result.push(row?);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    fn open_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        conn
    }

    fn sample_table(source_url: &str) -> FetchedDifficultyTable {
        FetchedDifficultyTable {
            source_url: source_url.to_string(),
            head_url: format!("{source_url}header.json"),
            name: "Insane Table".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["1".to_string(), "2".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "1".to_string(),
                    md5: "aabbcc".repeat(5) + "aabb",
                    sha256: "00".repeat(32),
                    title: "Song A".to_string(),
                    artist: String::new(),
                    comment: String::new(),
                    url: "https://example.com/song-a".to_string(),
                    append_url: "https://example.com/song-a-diff".to_string(),
                    ipfs: "/ipfs/bafy-song-a".to_string(),
                    append_ipfs: "/ipfs/bafy-song-a-diff".to_string(),
                },
                FetchedTableEntry {
                    level: "2".to_string(),
                    md5: "112233".repeat(5) + "1122",
                    sha256: "ff".repeat(32),
                    title: "Song B".to_string(),
                    artist: String::new(),
                    comment: String::new(),
                    ..FetchedTableEntry::default()
                },
            ],
            courses: Vec::new(),
            fetched_at: 1_700_000_000,
        }
    }

    #[test]
    fn upsert_and_list_tables() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");

        let id = upsert_difficulty_table(&mut conn, &table).unwrap();
        assert!(id > 0);

        let tables = list_difficulty_tables(&conn).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "Insane Table");
        assert_eq!(tables[0].symbol, "★");
        assert_eq!(tables[0].level_order, vec!["1", "2"]);
    }

    #[test]
    fn download_metadata_round_trips_with_table_entries() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let rows = list_table_entries_at_level(&conn, &table.source_url, "1").unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "https://example.com/song-a");
        assert_eq!(rows[0].append_url, "https://example.com/song-a-diff");
        assert_eq!(rows[0].ipfs, "/ipfs/bafy-song-a");
        assert_eq!(rows[0].append_ipfs, "/ipfs/bafy-song-a-diff");
    }

    #[test]
    fn download_metadata_version_marks_legacy_tables_for_refresh() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        assert_eq!(
            list_difficulty_table_sources_with_current_download_metadata(&conn).unwrap(),
            vec!["https://example.com/".to_string()]
        );

        conn.execute(
            "UPDATE difficulty_tables SET download_metadata_version = 0 WHERE source_url = ?1",
            params![table.source_url],
        )
        .unwrap();

        assert!(
            list_difficulty_table_sources_with_current_download_metadata(&conn).unwrap().is_empty()
        );
    }

    #[test]
    fn upsert_replaces_entries_on_conflict() {
        let mut conn = open_db();
        let mut table = sample_table("https://example.com/");

        upsert_difficulty_table(&mut conn, &table).unwrap();

        table.entries.clear();
        table.fetched_at = 1_700_000_001;
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let tables = list_difficulty_tables(&conn).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].fetched_at, 1_700_000_001);

        let md5 = "aabbcc".repeat(5) + "aabb";
        let entries = list_entries_by_md5s(&conn, &[md5.as_str()]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn delete_by_source_prefix_only_removes_matching_account_cache() {
        let mut conn = open_db();
        upsert_difficulty_table(&mut conn, &sample_table("bmz-rian-table:account-a:popular"))
            .unwrap();
        upsert_difficulty_table(&mut conn, &sample_table("bmz-rian-table:account-b:popular"))
            .unwrap();
        upsert_difficulty_table(&mut conn, &sample_table("https://example.com/table")).unwrap();

        let removed =
            delete_difficulty_tables_by_source_prefix(&conn, "bmz-rian-table:account-a:").unwrap();
        let remaining = list_difficulty_tables(&conn).unwrap();

        assert_eq!(removed, 1);
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|table| !table.source_url.contains("account-a")));
    }

    #[test]
    fn lookup_entries_by_md5() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let md5 = "aabbcc".repeat(5) + "aabb";
        let entries = list_entries_by_md5s(&conn, &[md5.as_str()]).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, "1");
        assert_eq!(entries[0].table_symbol, "★");
    }

    #[test]
    fn lookup_entries_by_sha256() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let sha256 = "ff".repeat(32);
        let entries = list_entries_by_sha256s(&conn, &[sha256.as_str()]).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, "2");
    }

    #[test]
    fn large_hash_lookups_match_small_ones_per_hash() {
        let mut conn = open_db();
        upsert_difficulty_table(&mut conn, &sample_table("https://example.com/a/")).unwrap();
        upsert_difficulty_table(&mut conn, &sample_table("https://example.com/b/")).unwrap();
        let md5s = ["112233".repeat(5) + "1122", "aabbcc".repeat(5) + "aabb"];
        let small: Vec<&str> = md5s.iter().map(String::as_str).collect();
        let padding: Vec<String> =
            (0..FULL_SCAN_HASH_COUNT).map(|index| format!("{index:032x}")).collect();
        let mut large = small.clone();
        large.extend(padding.iter().map(String::as_str));
        assert!(large.len() >= FULL_SCAN_HASH_COUNT);

        let by_join = list_entries_by_md5s(&conn, &small).unwrap();
        let by_scan = list_entries_by_md5s(&conn, &large).unwrap();
        assert_eq!(by_join.len(), 4);
        // Callers group by hash; within one hash both paths keep entry order.
        let key = |entries: &[DifficultyTableEntryRecord]| {
            let mut keys = entries
                .iter()
                .map(|entry| (entry.md5.clone(), entry.source_url.clone(), entry.level.clone()))
                .collect::<Vec<_>>();
            keys.sort_by(|left, right| left.0.cmp(&right.0));
            keys
        };
        assert_eq!(key(&by_scan), key(&by_join));
        let shas = ["ff".repeat(32)];
        let mut large: Vec<&str> = shas.iter().map(String::as_str).collect();
        large.extend(padding.iter().map(String::as_str));
        assert_eq!(list_entries_by_sha256s(&conn, &large).unwrap().len(), 2);
    }

    #[test]
    fn list_table_entries_includes_missing_library_entries() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let entries = list_table_entries(&conn, "https://example.com/").unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "Song A");
        assert_eq!(entries[1].title, "Song B");
    }

    #[test]
    fn list_table_entries_at_level_filters_in_sql() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let rows = list_table_entries_at_level(&conn, "https://example.com/", "2").unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].level, "2");
        assert_eq!(rows[0].title, "Song B");
    }
}
