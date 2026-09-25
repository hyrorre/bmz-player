use super::*;
use crate::config::app_config::PathEntry;

/// A lexical membership check also works for disconnected or removed roots.
pub(crate) fn song_root_contains_file(root: &PathEntry, path: &str) -> bool {
    let root_path = normalize_library_path(&root.path);
    let path = normalize_library_path(path);
    #[cfg(windows)]
    let (root_path, path) = (root_path.to_lowercase(), path.to_lowercase());
    let prefix = format!("{}/", root_path.trim_end_matches('/'));
    path.strip_prefix(&prefix)
        .is_some_and(|relative| !relative.is_empty() && (root.recursive || !relative.contains('/')))
}

impl LibraryDatabase {
    pub fn configured_song_roots(&self) -> Result<Option<Vec<PathEntry>>> {
        configured_song_roots(&self.conn)
    }

    /// Publish the session's lookup scope without deleting charts. Full cleanup
    /// separately requires saved settings and an unchanged scan snapshot.
    pub fn set_configured_song_roots(&self, roots: &[PathEntry]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO library_song_scope (id, roots_json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET roots_json = excluded.roots_json",
            [serde_json::to_string(roots)?],
        )?;
        Ok(())
    }

    /// Disabled/offline roots and rootless CLI/Viewer imports retain their records.
    pub fn reconcile_configured_song_roots(&mut self, roots: &[PathEntry]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let candidates = {
            let mut stmt =
                tx.prepare("SELECT id, path FROM chart_files WHERE root_id IS NOT NULL")?;
            stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let removed: Vec<i64> = candidates
            .into_iter()
            .filter(|(_, path)| !roots.iter().any(|root| song_root_contains_file(root, path)))
            .map(|(id, _)| id)
            .collect();
        super::database_write::delete_chart_files(&tx, &removed)?;
        tx.execute("DELETE FROM roots WHERE NOT EXISTS (SELECT 1 FROM chart_files WHERE root_id = roots.id)", [])?;
        tx.commit()?;
        Ok(removed.len())
    }
}

pub(crate) fn configured_song_roots(conn: &Connection) -> Result<Option<Vec<PathEntry>>> {
    // Course repair also runs during migrations preceding the scope table.
    let has_scope: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'library_song_scope')", [], |row| row.get(0))?;
    if !has_scope {
        return Ok(None);
    }
    let json: Option<String> = conn
        .query_row("SELECT roots_json FROM library_song_scope WHERE id = 1", [], |row| row.get(0))
        .optional()?;
    json.map(|json| serde_json::from_str(&json).map_err(Into::into)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::ScanConfig;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};
    use crate::storage::scan::scan_song_roots;

    #[test]
    fn scope_membership_respects_boundaries_recursion_and_windows_paths() {
        let mut root = PathEntry { path: "C:/BMS/".into(), enabled: false, recursive: false };
        assert!(song_root_contains_file(&root, "C:/BMS/song.bms"));
        assert!(!song_root_contains_file(&root, "C:/BMS-old/song.bms"));
        assert!(!song_root_contains_file(&root, "C:/BMS/sub/song.bms"));
        assert!(!song_root_contains_file(&root, "C:/BMS/"));
        root.recursive = true;
        assert!(song_root_contains_file(&root, r"\\?\C:\BMS\sub\song.bms"));
        #[cfg(windows)]
        assert!(song_root_contains_file(&root, "c:/bms/sub/song.bms"));
    }

    #[test]
    fn full_sync_retains_disabled_offline_and_standalone_imports_and_rolls_back_failures() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("bmz-scope-{}-{stamp}", std::process::id()));
        let mut roots = Vec::new();
        for name in ["old", "new", "disabled", "offline", "standalone"] {
            let path = dir.join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("song.bms"), "#TITLE Same\n#BPM 120\n#00011:01\n").unwrap();
            roots.push(PathEntry {
                path: path.to_string_lossy().into_owned(),
                enabled: true,
                recursive: true,
            });
        }
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut db = LibraryDatabase::from_connection(conn);
        let config = ScanConfig {
            use_everything: false,
            ..crate::config::app_config::AppConfig::default().scan
        };
        scan_song_roots(&mut db, &roots, &config, 1, false).unwrap();
        let standalone = library_path_key(&dir.join("standalone/song.bms"));
        db.conn
            .execute("UPDATE chart_files SET root_id = NULL WHERE path = ?1", [&standalone])
            .unwrap();
        roots[2].enabled = false;
        std::fs::remove_file(dir.join("offline/song.bms")).unwrap();
        std::fs::remove_dir(dir.join("offline")).unwrap();
        let retained = roots[1..4].to_vec();
        db.set_configured_song_roots(&retained).unwrap();
        assert_eq!(db.configured_song_roots().unwrap(), Some(retained.clone()));
        // Partial scan cannot remove the other configured or old roots.
        scan_song_roots(&mut db, &retained, &config, 2, false).unwrap();
        assert_eq!(db.list_charts(20, 0).unwrap().len(), 5);
        db.conn.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON charts BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(db.reconcile_configured_song_roots(&retained).is_err());
        assert_eq!(db.list_charts(20, 0).unwrap().len(), 5);
        let files: i64 =
            db.conn.query_row("SELECT COUNT(*) FROM chart_files", [], |r| r.get(0)).unwrap();
        assert_eq!(files, 5);
        db.conn.execute_batch("DROP TRIGGER fail_cleanup;").unwrap();
        assert_eq!(db.reconcile_configured_song_roots(&retained).unwrap(), 1);
        assert!(dir.join("old/song.bms").is_file());
        assert_eq!(db.list_charts(20, 0).unwrap().len(), 4);
        // Explicit empty configuration removes scan-owned registrations, not direct imports.
        assert_eq!(db.reconcile_configured_song_roots(&[]).unwrap(), 3);
        assert_eq!(db.list_charts(20, 0).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
