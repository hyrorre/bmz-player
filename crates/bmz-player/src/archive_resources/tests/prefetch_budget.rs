use super::*;

const SMALL_CHART: &[u8] = b"#TITLE Budget\n#BPM 120\n#WAV01 payload.wav\n#00111:01\n";
const PAYLOAD: &[u8] = &[1; 64];
const UNUSED: &[u8] = &[2; 16];
const FOLDER_BYTES: u64 = (SMALL_CHART.len() + PAYLOAD.len() + UNUSED.len()) as u64;

fn fixture(label: &str) -> (Temp, PathBuf, Arc<SongResourceSet>) {
    let temp = Temp::new(label);
    let path = temp.0.join("songs.7z");
    write_7z(
        &path,
        &[
            ("a/chart.bms", SMALL_CHART),
            ("a/payload.wav", PAYLOAD),
            ("a/unused.wav", UNUSED),
            ("b/chart.bms", SMALL_CHART),
            ("b/payload.wav", PAYLOAD),
            ("b/unused.wav", UNUSED),
            ("c/chart.bms", SMALL_CHART),
            ("c/payload.wav", PAYLOAD),
            ("c/unused.wav", UNUSED),
        ],
    );
    let index = song_archive::inspect(&path, &Default::default()).unwrap();
    let resources = resources_for_index(&path, &index);
    bmz_song_resource::install(resources.clone());
    // Keep a live owner from the first operation, even if parallel tests evict the cache.
    (temp, path, resources)
}

fn prefetch_with_limit(path: &Path, chart: &str, limit: u64) -> bool {
    prefetch::prefetch_chart_with_byte_limit(path, chart, &Default::default(), limit).unwrap()
}

#[test]
fn cumulative_prefetch_stops_at_the_budget_but_cached_and_normal_loads_still_work() {
    let (_temp, path, resources) = fixture("prefetch-budget");
    assert!(prefetch_with_limit(&path, "a/chart.bms", FOLDER_BYTES * 2));
    let pinned = resources.path("a/payload.wav");
    assert!(prefetch_with_limit(&path, "b/chart.bms", FOLDER_BYTES * 2));
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES * 2, "the exact limit is allowed");
    assert!(!prefetch_with_limit(&path, "c/chart.bms", FOLDER_BYTES * 2));
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES * 2);
    assert!(resources.loaded("c/chart.bms").is_none());
    assert!(resources.loaded("c/payload.wav").is_none());
    // A cache hit adds nothing, even when a prior real Play exceeded the limit.
    assert!(prefetch_with_limit(&path, "a/chart.bms", 0));
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES * 2);
    // Skipping must not consume the metadata needed to trim speculative entries later.
    let speculative = prefetch::take_speculative(&resources);
    assert!(speculative.iter().any(|name| name == "a/unused.wav"));
    assert!(speculative.iter().any(|name| name == "b/unused.wav"));

    // Deciding the skipped chart still loads it normally; an existing owner keeps its data.
    let source =
        ChartLocator::Archive { container: path, entry: "c/chart.bms".into() }.resolve().unwrap();
    let import =
        import_bms_chart_with_random_source(&source.path, BmsRandomSource::Seed(Some(1)), true)
            .unwrap();
    source.load_chart_assets(&import.chart).unwrap();
    assert!(Arc::ptr_eq(source.resources().unwrap(), &resources));
    assert_eq!(&*bmz_song_resource::read(&pinned).unwrap().unwrap(), PAYLOAD);
    assert_eq!(&*bmz_song_resource::read(&import.chart.sounds[0].path).unwrap().unwrap(), PAYLOAD);
    assert!(resources.loaded_bytes() > FOLDER_BYTES * 2);
}

#[test]
fn partial_folder_cache_is_not_counted_twice() {
    let (_temp, path, resources) = fixture("prefetch-partial");
    let loaded = resources.read("b/payload.wav").unwrap();
    assert_eq!(resources.loaded_bytes(), PAYLOAD.len() as u64);
    assert!(prefetch_with_limit(&path, "b/chart.bms", FOLDER_BYTES));
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES);
    assert!(Arc::ptr_eq(&loaded, &resources.loaded("b/payload.wav").unwrap()));
}

#[test]
fn a_new_archive_generation_has_its_own_prefetch_budget() {
    let (_temp, path, old) = fixture("prefetch-generation-budget");
    assert!(prefetch_with_limit(&path, "a/chart.bms", FOLDER_BYTES * 2));
    assert!(prefetch_with_limit(&path, "b/chart.bms", FOLDER_BYTES * 2));
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    write_7z(
        &path,
        &[
            ("song/chart.bms", SMALL_CHART),
            ("song/payload.wav", &[3; 64]),
            ("song/unused.wav", UNUSED),
        ],
    );
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified + std::time::Duration::from_secs(2))
        .unwrap();
    assert!(prefetch_with_limit(&path, "song/chart.bms", FOLDER_BYTES));
    let new = bmz_song_resource::find(&path).unwrap();
    assert!(!Arc::ptr_eq(&old, &new));
    assert_eq!(new.loaded_bytes(), FOLDER_BYTES);
    assert_eq!(old.loaded_bytes(), FOLDER_BYTES * 2);
    assert_eq!(&*bmz_song_resource::read(&old.path("a/payload.wav")).unwrap().unwrap(), PAYLOAD);
    assert_eq!(&*new.read("song/payload.wav").unwrap(), &[3; 64]);
}

#[test]
fn concurrent_prefetches_check_the_budget_under_the_container_lock() {
    let (_temp, path, resources) = fixture("prefetch-concurrent-budget");
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = ["a/chart.bms", "b/chart.bms"]
            .into_iter()
            .map(|chart| {
                let (path, barrier) = (&path, &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    prefetch_with_limit(path, chart, FOLDER_BYTES)
                })
            })
            .collect();
        workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|loaded| **loaded).count(), 1);
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES);
}

#[test]
fn cancelled_prefetch_does_not_change_an_existing_set() {
    let (_temp, path, resources) = fixture("prefetch-cancelled-budget");
    assert!(prefetch_with_limit(&path, "a/chart.bms", FOLDER_BYTES));
    let control = ArchiveControl {
        cancelled: Some(Arc::new(std::sync::atomic::AtomicBool::new(true))),
        ..Default::default()
    };
    assert!(
        prefetch::prefetch_chart_with_byte_limit(&path, "b/chart.bms", &control, FOLDER_BYTES)
            .is_err()
    );
    assert_eq!(resources.loaded_bytes(), FOLDER_BYTES);
    assert!(resources.loaded("b/chart.bms").is_none());
}
