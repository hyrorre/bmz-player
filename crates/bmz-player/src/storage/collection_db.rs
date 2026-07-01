use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};

#[derive(Debug)]
pub struct CollectionDatabase {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteChartRecord {
    pub chart_sha256: [u8; 32],
    pub title_hint: String,
    pub artist_hint: String,
    pub folder_hint: String,
    pub chart_path_hint: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteSongRecord {
    pub representative_sha256: [u8; 32],
    pub title_hint: String,
    pub artist_hint: String,
    pub origin_folder_hint: String,
    pub origin_chart_path_hint: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteHints {
    pub title: String,
    pub artist: String,
    pub folder: String,
    pub chart_path: String,
}

impl FavoriteHints {
    pub fn new(
        title: impl Into<String>,
        artist: impl Into<String>,
        folder: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            folder: folder.into(),
            chart_path: String::new(),
        }
    }
}

impl CollectionDatabase {
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

    pub fn favorite_chart_records(&self) -> Result<Vec<FavoriteChartRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256, title_hint, artist_hint, folder_hint, chart_path_hint,
                    created_at, updated_at
             FROM favorite_charts
             ORDER BY created_at ASC, title_hint COLLATE NOCASE ASC",
        )?;
        stmt.query_map([], favorite_chart_record_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn favorite_song_records(&self) -> Result<Vec<FavoriteSongRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT representative_sha256, title_hint, artist_hint, origin_folder_hint,
                    origin_chart_path_hint, created_at, updated_at
             FROM favorite_songs
             ORDER BY created_at ASC, title_hint COLLATE NOCASE ASC",
        )?;
        stmt.query_map([], favorite_song_record_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn favorite_chart_set(&self) -> Result<HashSet<[u8; 32]>> {
        Ok(self.favorite_chart_records()?.into_iter().map(|record| record.chart_sha256).collect())
    }

    pub fn is_favorite_chart(&self, chart_sha256: [u8; 32]) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM favorite_charts WHERE chart_sha256 = ?1",
                params![hash_to_hex(&chart_sha256)],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn is_favorite_song_representative(&self, representative_sha256: [u8; 32]) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM favorite_songs WHERE representative_sha256 = ?1",
                params![hash_to_hex(&representative_sha256)],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn upsert_favorite_chart(
        &mut self,
        chart_sha256: [u8; 32],
        hints: &FavoriteHints,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO favorite_charts (
                chart_sha256, title_hint, artist_hint, folder_hint, chart_path_hint,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(chart_sha256) DO UPDATE SET
                title_hint = excluded.title_hint,
                artist_hint = excluded.artist_hint,
                folder_hint = excluded.folder_hint,
                chart_path_hint = excluded.chart_path_hint,
                updated_at = excluded.updated_at",
            params![
                hash_to_hex(&chart_sha256),
                hints.title.as_str(),
                hints.artist.as_str(),
                hints.folder.as_str(),
                hints.chart_path.as_str(),
                now,
            ],
        )?;
        Ok(())
    }

    pub fn remove_favorite_chart(&mut self, chart_sha256: [u8; 32]) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM favorite_charts WHERE chart_sha256 = ?1",
            params![hash_to_hex(&chart_sha256)],
        )?;
        Ok(changed > 0)
    }

    pub fn toggle_favorite_chart(
        &mut self,
        chart_sha256: [u8; 32],
        hints: &FavoriteHints,
        now: i64,
    ) -> Result<bool> {
        if self.is_favorite_chart(chart_sha256)? {
            self.remove_favorite_chart(chart_sha256)?;
            Ok(false)
        } else {
            self.upsert_favorite_chart(chart_sha256, hints, now)?;
            Ok(true)
        }
    }

    pub fn upsert_favorite_song(
        &mut self,
        representative_sha256: [u8; 32],
        hints: &FavoriteHints,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO favorite_songs (
                representative_sha256, title_hint, artist_hint, origin_folder_hint,
                origin_chart_path_hint, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(representative_sha256) DO UPDATE SET
                title_hint = excluded.title_hint,
                artist_hint = excluded.artist_hint,
                origin_folder_hint = excluded.origin_folder_hint,
                origin_chart_path_hint = excluded.origin_chart_path_hint,
                updated_at = excluded.updated_at",
            params![
                hash_to_hex(&representative_sha256),
                hints.title.as_str(),
                hints.artist.as_str(),
                hints.folder.as_str(),
                hints.chart_path.as_str(),
                now,
            ],
        )?;
        Ok(())
    }

    pub fn remove_favorite_song(&mut self, representative_sha256: [u8; 32]) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM favorite_songs WHERE representative_sha256 = ?1",
            params![hash_to_hex(&representative_sha256)],
        )?;
        Ok(changed > 0)
    }

    pub fn remove_favorite_songs(&mut self, representatives: &[[u8; 32]]) -> Result<usize> {
        if representatives.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut removed = 0_usize;
        {
            let mut stmt =
                tx.prepare("DELETE FROM favorite_songs WHERE representative_sha256 = ?1")?;
            for representative in representatives {
                removed += stmt.execute(params![hash_to_hex(representative)])?;
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn toggle_favorite_song(
        &mut self,
        representative_sha256: [u8; 32],
        hints: &FavoriteHints,
        now: i64,
    ) -> Result<bool> {
        if self.is_favorite_song_representative(representative_sha256)? {
            self.remove_favorite_song(representative_sha256)?;
            Ok(false)
        } else {
            self.upsert_favorite_song(representative_sha256, hints, now)?;
            Ok(true)
        }
    }
}

fn favorite_chart_record_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<FavoriteChartRecord> {
    let sha256_hex: String = row.get(0)?;
    Ok(FavoriteChartRecord {
        chart_sha256: hex_to_hash::<32>(&sha256_hex)?,
        title_hint: row.get(1)?,
        artist_hint: row.get(2)?,
        folder_hint: row.get(3)?,
        chart_path_hint: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn favorite_song_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FavoriteSongRecord> {
    let sha256_hex: String = row.get(0)?;
    Ok(FavoriteSongRecord {
        representative_sha256: hex_to_hash::<32>(&sha256_hex)?,
        title_hint: row.get(1)?,
        artist_hint: row.get(2)?,
        origin_folder_hint: row.get(3)?,
        origin_chart_path_hint: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::storage::migration::{COLLECTION_MIGRATIONS, run_migrations};

    fn open_db() -> CollectionDatabase {
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, COLLECTION_MIGRATIONS).unwrap();
        CollectionDatabase::from_connection(conn)
    }

    #[test]
    fn favorite_chart_toggles_by_sha256() {
        let mut db = open_db();
        let sha = [1_u8; 32];
        let hints = FavoriteHints::new("Title", "Artist", "/songs/title");

        assert!(db.toggle_favorite_chart(sha, &hints, 10).unwrap());
        assert!(db.is_favorite_chart(sha).unwrap());
        assert!(!db.toggle_favorite_chart(sha, &hints, 11).unwrap());
        assert!(!db.is_favorite_chart(sha).unwrap());
    }

    #[test]
    fn favorite_song_toggles_representative_sha256() {
        let mut db = open_db();
        let sha = [2_u8; 32];
        let hints = FavoriteHints::new("Song", "Composer", "/songs/song");

        assert!(db.toggle_favorite_song(sha, &hints, 20).unwrap());
        let records = db.favorite_song_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].representative_sha256, sha);
        assert_eq!(records[0].origin_folder_hint, "/songs/song");

        assert!(!db.toggle_favorite_song(sha, &hints, 21).unwrap());
        assert!(db.favorite_song_records().unwrap().is_empty());
    }
}
