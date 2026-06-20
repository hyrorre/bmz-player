use anyhow::Result;
use rusqlite::{Connection, params};

use crate::difficulty_table::FetchedDifficultyTable;

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
}

pub(super) fn upsert_difficulty_table(
    conn: &mut Connection,
    table: &FetchedDifficultyTable,
) -> Result<i64> {
    let level_order_json = serde_json::to_string(&table.level_order)?;
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO difficulty_tables (source_url, head_url, name, symbol, level_order, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(source_url) DO UPDATE SET
             head_url = excluded.head_url,
             name = excluded.name,
             symbol = excluded.symbol,
             level_order = excluded.level_order,
             fetched_at = excluded.fetched_at",
        params![
            table.source_url,
            table.head_url,
            table.name,
            table.symbol,
            level_order_json,
            table.fetched_at
        ],
    )?;

    let table_id: i64 = tx.query_row(
        "SELECT id FROM difficulty_tables WHERE source_url = ?1",
        params![table.source_url],
        |row| row.get(0),
    )?;

    tx.execute("DELETE FROM difficulty_table_entries WHERE table_id = ?1", params![table_id])?;

    for entry in &table.entries {
        tx.execute(
            "INSERT INTO difficulty_table_entries
             (table_id, level, md5, sha256, title, artist, comment)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                table_id,
                entry.level,
                entry.md5,
                entry.sha256,
                entry.title,
                entry.artist,
                entry.comment
            ],
        )?;
    }

    tx.commit()?;
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

/// Lists every entry of the given difficulty table without joining local charts.
pub(super) fn list_table_entries(
    conn: &Connection,
    source_url: &str,
) -> Result<Vec<TableEntryRow>> {
    let sql = "
        SELECT dte.level, dte.md5, dte.sha256, dte.title, dte.artist, dte.comment
        FROM difficulty_table_entries dte
        JOIN difficulty_tables dt ON dt.id = dte.table_id
        WHERE dt.source_url = ?1";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![source_url], |row| {
        Ok(TableEntryRow {
            level: row.get(0)?,
            md5: row.get(1)?,
            sha256: row.get(2)?,
            title: row.get(3)?,
            artist: row.get(4)?,
            comment: row.get(5)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
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

/// Look up table entries by a single hash column (`md5` or `sha256`).
///
/// Uses a single prepared statement with `WHERE dte.<col> = ?` and reuses it
/// for every input hash. This avoids hitting SQLite's per-statement variable
/// limit (`SQLITE_MAX_VARIABLE_NUMBER`, 999 on older builds / 32766 on newer
/// ones) when a folder contains tens of thousands of BMS files.
fn list_entries_by_hash_column(
    conn: &Connection,
    column: &'static str,
    hashes: &[&str],
) -> Result<Vec<DifficultyTableEntryRecord>> {
    debug_assert!(matches!(column, "md5" | "sha256"));
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT dt.source_url, dt.name, dt.symbol, dte.level, dte.md5, dte.sha256
         FROM difficulty_table_entries dte
         JOIN difficulty_tables dt ON dt.id = dte.table_id
         WHERE dte.{column} = ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut result = Vec::new();
    for hash in hashes {
        let rows = stmt.query_map(params![hash], |row| {
            Ok(DifficultyTableEntryRecord {
                source_url: row.get(0)?,
                table_name: row.get(1)?,
                table_symbol: row.get(2)?,
                level: row.get(3)?,
                md5: row.get(4)?,
                sha256: row.get(5)?,
            })
        })?;
        for row in rows {
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
                },
                FetchedTableEntry {
                    level: "2".to_string(),
                    md5: "112233".repeat(5) + "1122",
                    sha256: "ff".repeat(32),
                    title: "Song B".to_string(),
                    artist: String::new(),
                    comment: String::new(),
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
    fn list_table_entries_includes_missing_library_entries() {
        let mut conn = open_db();
        let table = sample_table("https://example.com/");
        upsert_difficulty_table(&mut conn, &table).unwrap();

        let entries = list_table_entries(&conn, "https://example.com/").unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "Song A");
        assert_eq!(entries[1].title, "Song B");
    }
}
