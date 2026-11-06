use super::*;

fn counted_bytes(layer: &LuaFileLayer) -> usize {
    layer
        .files
        .iter()
        .map(|(key, file)| LuaFileLayer::entry_bytes(key, file.as_ref().map_or(0, Vec::capacity)))
        .sum::<usize>()
        + layer.directories.iter().map(|key| LuaFileLayer::entry_bytes(key, 0)).sum::<usize>()
}

#[test]
fn empty_files_directories_and_tombstones_share_the_entry_limit() {
    let mut layer = LuaFileLayer::default();
    for i in 0..LUA_FILE_STORE_MAX_ENTRIES {
        let key = PathBuf::from(format!("entry-{i}"));
        match i % 3 {
            0 => layer.create_directory(key).unwrap(),
            1 => layer.write(key, Some(Vec::new())).unwrap(),
            _ => layer.write(key, None).unwrap(),
        }
    }
    assert!(layer.create_directory("overflow-dir".into()).is_err());
    assert!(layer.write("overflow-file".into(), Some(Vec::new())).is_err());
    assert!(layer.write("overflow-deletion".into(), None).is_err());
    layer.create_directory("entry-0".into()).unwrap();
    layer.write("entry-1".into(), None).unwrap();
    assert_eq!(layer.bytes, counted_bytes(&layer));
    assert!(layer.bytes > LUA_FILE_STORE_MAX_ENTRIES * LUA_FILE_ENTRY_OVERHEAD);
}

#[test]
fn layer_counts_metadata_capacity_and_replacements_before_mutation() {
    let mut layer = LuaFileLayer::default();
    let long = PathBuf::from("x".repeat(LUA_FILE_PATH_MAX_BYTES + 1));
    assert!(layer.write(long.clone(), None).is_err());
    assert!(layer.create_directory(long).is_err());
    assert_eq!(layer.bytes, 0);
    let key = PathBuf::from("file");
    let capacity = LUA_FILE_STORE_MAX_BYTES - LuaFileLayer::entry_bytes(&key, 0);
    layer.write(key.clone(), Some(Vec::with_capacity(capacity))).unwrap();
    assert_eq!(layer.bytes, LUA_FILE_STORE_MAX_BYTES);
    assert!(layer.create_directory("dir".into()).is_err());
    assert!(layer.write("empty".into(), None).is_err());
    assert_eq!(layer.bytes, counted_bytes(&layer));
    layer.write(key.clone(), None).unwrap();
    layer.create_directory("dir".into()).unwrap();
    assert_eq!(layer.bytes, counted_bytes(&layer));
    assert!(layer.files[&key].is_none());
}

#[test]
fn merged_session_limits_are_atomic_and_account_for_overwritten_entries() {
    let store = LuaSkinFileStore::default();
    let mut first = LuaFileOverlay::new(Some(store.clone()));
    for i in 0..LUA_FILE_STORE_MAX_ENTRIES {
        first.create_directory(PathBuf::from(format!("dir-{i}"))).unwrap();
    }
    first.commit();
    assert!(first.staged.directories.is_empty());
    let mut next = LuaFileOverlay::new(Some(store.clone()));
    next.create_directory("extra".into()).unwrap();
    next.commit();
    assert!(next.staged.directories.is_empty());
    assert!(!store.0.lock().unwrap().directories.contains(Path::new("extra")));
    next.go_live();
    assert!(next.create_directory("extra".into()).is_err());
    assert!(next.write("extra".into(), None).is_err());
    next.begin_inference();
    next.create_directory("probe".into()).unwrap();
    next.commit();
    let stored = store.0.lock().unwrap();
    assert_eq!(stored.directories.len(), LUA_FILE_STORE_MAX_ENTRIES);
    assert_eq!(stored.bytes, counted_bytes(&stored));
    drop(stored);

    let mut a = LuaFileLayer::default();
    a.write("same".into(), Some(vec![0; 100])).unwrap();
    a.create_directory("dir".into()).unwrap();
    let mut b = LuaFileLayer::default();
    b.write("same".into(), None).unwrap();
    b.create_directory("dir".into()).unwrap();
    b.merge_into(&mut a).unwrap();
    assert_eq!(a.bytes, counted_bytes(&a));
    assert_eq!(a.files.len() + a.directories.len(), 2);
    assert_eq!(b.bytes, 0);
}
