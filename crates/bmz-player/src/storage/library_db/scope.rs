use super::*;
use crate::config::app_config::PathEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SongRootPath {
    #[serde(flatten)]
    entry: PathEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_path: Option<String>,
}

/// Persisted lookup scope. Reading it never accesses song disks.
#[derive(Debug, Clone)]
pub(crate) struct SongRootScope {
    roots: Option<Vec<SongRootPath>>,
}

impl SongRootScope {
    pub(crate) fn load(conn: &Connection) -> Result<Self> {
        let roots =
            configured_scope_json(conn)?.map(|json| serde_json::from_str(&json)).transpose()?;
        Ok(Self { roots })
    }

    fn retained_roots(&self, entries: &[PathEntry]) -> Vec<SongRootPath> {
        entries
            .iter()
            .map(|entry| {
                let key = normalize_library_path(&entry.path);
                let canonical_path = self
                    .roots
                    .iter()
                    .flatten()
                    .find(|previous| normalize_library_path(&previous.entry.path) == key)
                    .and_then(|previous| previous.canonical_path.clone());
                SongRootPath { entry: entry.clone(), canonical_path }
            })
            .collect()
    }

    pub(crate) fn is_unrestricted(&self) -> bool {
        self.roots.is_none()
    }

    pub(crate) fn contains_file(&self, path: &str) -> bool {
        self.roots.as_ref().is_some_and(|roots| roots.iter().any(|root| root.contains_file(path)))
    }

    pub(crate) fn contains_file_in_enabled_root(&self, path: &str) -> bool {
        self.roots.as_ref().is_some_and(|roots| {
            roots.iter().any(|root| root.entry.enabled && root.contains_file(path))
        })
    }
}

impl SongRootPath {
    fn contains_file(&self, path: &str) -> bool {
        song_root_contains_file(&self.entry, path)
            || self.canonical_path.as_deref().is_some_and(|canonical_path| {
                song_root_path_contains_file(canonical_path, self.entry.recursive, path)
            })
    }
}

/// A lexical membership check also works for disconnected or removed roots.
pub(crate) fn song_root_contains_file(root: &PathEntry, path: &str) -> bool {
    song_root_path_contains_file(&root.path, root.recursive, path)
}

fn song_root_path_contains_file(root_path: &str, recursive: bool, path: &str) -> bool {
    let root_path = normalize_library_path(root_path);
    let path = normalize_library_path(path);
    #[cfg(windows)]
    let (root_path, path) = (root_path.to_lowercase(), path.to_lowercase());
    let prefix = format!("{}/", root_path.trim_end_matches('/'));
    path.strip_prefix(&prefix)
        .is_some_and(|relative| !relative.is_empty() && (recursive || !relative.contains('/')))
}

impl LibraryDatabase {
    pub fn configured_song_roots(&self) -> Result<Option<Vec<PathEntry>>> {
        configured_song_roots(&self.conn)
    }

    /// Publish the session's lookup scope without deleting charts. Full cleanup
    /// separately requires saved settings and an unchanged scan snapshot.
    pub fn set_configured_song_roots(&self, roots: &[PathEntry]) -> Result<()> {
        // Resolve aliases only when publishing configuration, retaining the last
        // successful resolution while a configured root is offline.
        let previous = SongRootScope::load(&self.conn)?;
        let mut resolved = previous.retained_roots(roots);
        for root in &mut resolved {
            if let Ok(path) = Path::new(&root.entry.path).canonicalize() {
                root.canonical_path = Some(normalize_library_path(&path.to_string_lossy()));
            }
        }
        self.conn.execute(
            "INSERT INTO library_song_scope (id, roots_json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET roots_json = excluded.roots_json",
            [serde_json::to_string(&resolved)?],
        )?;
        Ok(())
    }

    /// Disabled/offline roots and rootless CLI/Viewer imports retain their records.
    pub fn reconcile_configured_song_roots(&mut self, roots: &[PathEntry]) -> Result<usize> {
        let previous = SongRootScope::load(&self.conn)?;
        let scope = SongRootScope { roots: Some(previous.retained_roots(roots)) };
        let tx = self.conn.transaction()?;
        let candidates = {
            let mut stmt =
                tx.prepare("SELECT id, path FROM chart_files WHERE root_id IS NOT NULL")?;
            stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let removed: Vec<i64> = candidates
            .into_iter()
            .filter(|(_, path)| !scope.contains_file(path))
            .map(|(id, _)| id)
            .collect();
        super::database_write::delete_chart_files(&tx, &removed)?;
        tx.execute("DELETE FROM roots WHERE NOT EXISTS (SELECT 1 FROM chart_files WHERE root_id = roots.id)", [])?;
        tx.commit()?;
        Ok(removed.len())
    }
}

pub(crate) fn configured_song_roots(conn: &Connection) -> Result<Option<Vec<PathEntry>>> {
    Ok(SongRootScope::load(conn)?
        .roots
        .map(|roots| roots.into_iter().map(|root| root.entry).collect()))
}

fn configured_scope_json(conn: &Connection) -> Result<Option<String>> {
    // Course repair also runs during migrations preceding the scope table.
    let has_scope: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'library_song_scope')", [], |row| row.get(0))?;
    if !has_scope {
        return Ok(None);
    }
    let json: Option<String> = conn
        .query_row("SELECT roots_json FROM library_song_scope WHERE id = 1", [], |row| row.get(0))
        .optional()?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::ScanConfig;
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};
    use crate::storage::scan::scan_song_roots;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[cfg(unix)]
    #[test]
    fn scope_membership_matches_canonical_aliases_and_retains_lexical_offline_roots() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let temp =
            std::env::temp_dir().join(format!("bmz-scope-alias-{}-{stamp}", std::process::id()));
        let actual_root = temp.join("actual");
        let alias_root = temp.join("alias");
        std::fs::create_dir_all(&actual_root).unwrap();
        std::os::unix::fs::symlink(&actual_root, &alias_root).unwrap();

        let db_path = temp.join("library.db");
        crate::storage::migration::migrate_library_db(&db_path).unwrap();
        let mut db = LibraryDatabase::open(&db_path).unwrap();
        let roots = vec![PathEntry {
            path: alias_root.to_string_lossy().into_owned(),
            enabled: false,
            recursive: true,
        }];
        db.set_configured_song_roots(&roots).unwrap();
        let scope = SongRootScope::load(&db.conn).unwrap();
        let file_path = actual_root
            .canonicalize()
            .unwrap()
            .join("nested/song.bms")
            .to_string_lossy()
            .into_owned();
        assert!(scope.contains_file(&file_path));
        assert!(!scope.contains_file_in_enabled_root(&file_path));

        let chart_path = actual_root.canonicalize().unwrap().join("song.bms");
        std::fs::write(&chart_path, "#TITLE Alias\n#BPM 120\n#00011:01\n").unwrap();
        let id = crate::storage::import::import_chart_file(&mut db, &chart_path, None, None, 1)
            .unwrap()
            .chart_id;
        let chart = db.verified_chart_source(id).unwrap().chart;
        assert!(db.available_chart_source(id).unwrap().is_none());
        assert!(db.registered_chart_sources(&[&chart]).unwrap().is_empty());

        // A new connection and scope publication must retain the alias after it
        // disappears. The canonical chart remains readable for explicit play.
        std::fs::remove_file(&alias_root).unwrap();
        drop(db);
        let db = LibraryDatabase::open(&db_path).unwrap();
        assert!(db.registered_chart_sources(&[&chart]).unwrap().is_empty());
        db.set_configured_song_roots(&roots).unwrap();
        assert!(db.available_chart_source(id).unwrap().is_none());
        assert!(db.registered_chart_sources(&[&chart]).unwrap().is_empty());
        assert!(db.verified_chart_source(id).is_ok());
        assert_eq!(db.configured_song_roots().unwrap(), Some(roots));

        let offline_root = alias_root.join("offline").to_string_lossy().into_owned();
        db.set_configured_song_roots(&[PathEntry {
            path: offline_root.clone(),
            enabled: false,
            recursive: true,
        }])
        .unwrap();
        let offline_scope = SongRootScope::load(&db.conn).unwrap();
        assert!(offline_scope.contains_file(&format!("{offline_root}/song.bms")));

        drop(db);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn scope_loads_legacy_json_without_canonical_paths() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let roots =
            vec![PathEntry { path: "/offline/songs".into(), enabled: false, recursive: true }];
        conn.execute(
            "INSERT INTO library_song_scope (id, roots_json) VALUES (1, ?1)",
            [serde_json::to_string(&roots).unwrap()],
        )
        .unwrap();
        assert_eq!(configured_song_roots(&conn).unwrap(), Some(roots));
        let scope = SongRootScope::load(&conn).unwrap();
        assert!(scope.contains_file("/offline/songs/song.bms"));
        assert!(!scope.contains_file_in_enabled_root("/offline/songs/song.bms"));
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
