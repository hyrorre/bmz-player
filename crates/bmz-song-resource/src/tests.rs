use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The registry is process-wide; tests that install sets must not evict each other's.
static REGISTRY_TESTS: Mutex<()> = Mutex::new(());

fn set(container: &str, names: &[&str]) -> SongResourceSet {
    SongResourceSet::new(container, "gen", names.iter().map(|name| name.to_string()))
}

#[test]
fn parses_archive_entries_and_rejects_escapes() {
    assert_eq!(SongPath::parse(Path::new("C:/songs/song/a.wav")), SongPath::File);
    assert_eq!(SongPath::parse(Path::new("C:/songs/Wow!/a.wav")), SongPath::File);
    assert_eq!(
        SongPath::parse(Path::new(r"C:\songs\pack.ZIP!\song\..\shared\.\a.wav")),
        SongPath::Archive { container: "C:/songs/pack.ZIP".into(), entry: "shared/a.wav".into() }
    );
    assert_eq!(
        SongPath::parse(Path::new(r"\\?\C:\songs\pack.7z!")),
        SongPath::Archive { container: "C:/songs/pack.7z".into(), entry: String::new() }
    );
    assert_eq!(SongPath::parse(Path::new("C:/songs/pack.rar!/../x.wav")), SongPath::Invalid);
    assert_eq!(SongPath::parse(Path::new("C:/songs/pack.rar!/a:b.wav")), SongPath::Invalid);
}

#[test]
fn entry_lookup_prefers_exact_names_then_ignores_case() {
    let set = set("C:/case/pack.zip", &["song/KICK.wav", "song/snare.WAV"]);
    assert_eq!(set.entry_name("song/KICK.wav"), Some("song/KICK.wav"));
    assert_eq!(set.entry_name("SONG/kick.WAV"), Some("song/KICK.wav"));
    assert_eq!(set.entry_name("song/hat.wav"), None);
    set.insert("song/KICK.wav", b"kick".to_vec());
    assert_eq!(&*set.read("Song/Kick.wav").unwrap(), b"kick");
    assert_eq!(
        set.unloaded(["song/kick.wav", "song/SNARE.wav", "song/hat.wav"]),
        ["song/snare.WAV"]
    );
    assert_eq!(set.read("song/snare.WAV").unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn unloaded_entries_are_fetched_once_and_kept() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let set = set("C:/fetch/pack.7z", &["a.ogg"]).with_fetch(move |names| {
        counted.fetch_add(1, Ordering::Relaxed);
        Ok(names.iter().map(|name| (name.clone(), name.as_bytes().to_vec())).collect())
    });
    assert_eq!(&*set.read("A.OGG").unwrap(), b"a.ogg");
    assert_eq!(&*set.read("a.ogg").unwrap(), b"a.ogg");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(set.loaded_bytes(), 5);
    set.retain_loaded(|_| false);
    assert_eq!(set.loaded_count(), 0);
    assert_eq!(set.loaded_bytes(), 0);
}

#[test]
fn pinned_paths_keep_their_set_across_reloads_and_expire_without_fallback() {
    let _serial = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let container = "C:/registry-generations/pack.zip";
    let old = Arc::new(set(container, &["song/a.wav", "song/removed.wav"]));
    old.insert("song/a.wav", b"old".to_vec());
    old.insert("song/removed.wav", b"removed".to_vec());
    install(old.clone());
    let old_path = old.path("song/sub/../A.wav");
    let removed_path = old.path("song/removed.wav");
    assert_eq!(
        SongPath::parse(&old_path),
        SongPath::Archive { container: container.into(), entry: "song/A.wav".into() }
    );
    let new = Arc::new(SongResourceSet::new(container, "new", ["song/a.wav".into()]));
    new.insert("song/a.wav", b"new".to_vec());
    install(new.clone());
    assert_eq!(&*read(&new.path("song/a.wav")).unwrap().unwrap(), b"new");
    // A stale worker installing its earlier set must not redirect a newer play.
    install(old.clone());
    assert_eq!(&*read(&new.path("song/a.wav")).unwrap().unwrap(), b"new");
    install(new.clone());
    assert_eq!(&*read(&old_path).unwrap().unwrap(), b"old");
    assert_eq!(&*read(&removed_path).unwrap().unwrap(), b"removed");
    assert_eq!(file_len(&old_path), Some(3));
    assert!(read(&old.path("../outside.wav")).unwrap().is_err());
    // Old entries stay available even after all cache-owned references are dropped.
    clear();
    assert_eq!(&*read(&old_path).unwrap().unwrap(), b"old");
    drop(old);
    assert_eq!(exists(&old_path), Some(false));
    assert!(read(&old_path).unwrap().is_err());
    assert_eq!(&*read(&new.path("song/a.wav")).unwrap().unwrap(), b"new");
}

#[test]
fn an_existing_owner_prevents_another_chart_from_trimming_its_assets() {
    let _serial = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let play = Arc::new(set("C:/registry-sharing/pack.zip", &["a.wav", "b.wav"]));
    play.insert("a.wav", b"a".to_vec());
    play.insert("b.wav", b"b".to_vec());
    install(play.clone());
    let other_chart = find(play.container()).unwrap();
    other_chart.retain_loaded_if_unshared(|name| name == "b.wav");
    assert_eq!(&*read(&play.path("a.wav")).unwrap().unwrap(), b"a");
    // Removing the cache owner must not let one of two active charts trim the other.
    clear();
    other_chart.retain_loaded_if_unshared(|name| name == "b.wav");
    assert!(play.loaded("a.wav").is_some());
    drop(play);
    other_chart.retain_loaded_if_unshared(|name| name == "b.wav");
    assert!(other_chart.loaded("a.wav").is_none());
    assert_eq!(other_chart.loaded_bytes(), 1);
}

#[test]
fn registry_enforces_byte_budget_after_insert_and_lazy_fetch() {
    let _serial = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = (RECENT_BYTES_LIMIT / 2 + 1) as usize;
    let first = Arc::new(set("C:/registry-growing/first.zip", &["a"]));
    install(first.clone());
    first.insert("a", vec![0; bytes]);
    let weak = Arc::downgrade(&first);
    let first_path = first.path("a");
    let second = Arc::new(
        set("C:/registry-growing/second.zip", &["a"])
            .with_fetch(move |_| Ok(vec![("a".into(), vec![0; bytes])])),
    );
    install(second.clone());
    // Lazy fetch inserts bytes into a set that was empty at install time.
    drop(read(&second.path("a")).unwrap().unwrap());
    assert_eq!(second.loaded_bytes(), bytes as u64);
    // Eviction releases the cache reference, but never the running play's owner.
    assert_eq!(file_len(&first_path), Some(bytes as u64));
    drop(first);
    assert!(weak.upgrade().is_none());
    assert!(find(Path::new("C:/registry-growing/first.zip")).is_none());
    assert!(find(second.container()).is_some());
    // Replacing an entry updates the byte accounting instead of accumulating it.
    second.insert("a", vec![1; 3]);
    assert_eq!(second.loaded_bytes(), 3);
}

#[test]
fn registry_serves_installed_sets_by_virtual_path() {
    let _serial = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let container = "C:/registry-serve/pack.zip";
    let installed = set(container, &["song/a.wav"]);
    installed.insert("song/a.wav", b"wav".to_vec());
    install(Arc::new(installed));
    let path = Path::new("C:/registry-serve/pack.zip!/song/sub/../A.WAV");
    assert_eq!(exists(path), Some(true));
    assert_eq!(&*read(path).unwrap().unwrap(), b"wav");
    assert_eq!(file_len(path), Some(3));
    assert_eq!(exists(Path::new("C:/registry-serve/pack.zip!/song/b.wav")), Some(false));
    assert_eq!(exists(Path::new("C:/registry-serve/pack.zip!/../b.wav")), Some(false));
    assert!(read(Path::new("C:/registry-serve/pack.zip!/../b.wav")).unwrap().is_err());
    assert_eq!(exists(Path::new("C:/registry-serve/song/a.wav")), None);

    // A new generation replaces the old set for the same container.
    install(Arc::new(set(container, &["song/b.wav"])));
    assert_eq!(exists(path), Some(false));
    assert_eq!(exists(Path::new("C:/registry-serve/pack.zip!/song/b.wav")), Some(true));
}

#[test]
fn registry_keeps_the_newest_sets_within_its_limit() {
    let _serial = REGISTRY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let names: Vec<String> =
        (0..RECENT_SET_LIMIT + 1).map(|index| format!("C:/registry-evict/{index}.zip")).collect();
    for name in &names {
        install(Arc::new(set(name, &["a"])));
    }
    assert!(find(Path::new(&names[0])).is_none());
    // An owner keeps an evicted set reachable.
    let held = Arc::new(set("C:/registry-evict/held.zip", &["a"]));
    install(held.clone());
    for name in &names {
        install(Arc::new(set(name, &["a"])));
    }
    assert!(Arc::ptr_eq(&find(Path::new("C:/registry-evict/held.zip")).unwrap(), &held));
    drop(held);
    assert!(find(Path::new("C:/registry-evict/held.zip")).is_none());
    for name in &names[1..] {
        assert!(find(Path::new(name)).is_some(), "{name}");
    }

    // The newest set stays even when it alone exceeds the byte budget.
    let large = set("C:/registry-evict/large.zip", &["big"]);
    large.insert("big", vec![0; (RECENT_BYTES_LIMIT + 1) as usize]);
    install(Arc::new(large));
    assert!(find(Path::new("C:/registry-evict/large.zip")).is_some());
    assert!(find(Path::new(&names[RECENT_SET_LIMIT])).is_none());
}
