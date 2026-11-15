use super::*;
use crate::song_archive::ArchiveStamp;
use bmz_render::skin::SkinImageSize;

fn key(number: u64) -> Arc<ImageKey> {
    Arc::new(ImageKey::File {
        path: PathBuf::from(format!("banner-{number}.png")),
        stamp: ArchiveStamp { size: number, modified: None, created: None, changed: None },
    })
}

fn pixels() -> Arc<RgbaImageAsset> {
    Arc::new(RgbaImageAsset { width: 1, height: 1, pixels: vec![1, 2, 3, 255] })
}

fn view(texture: u32) -> SelectRowBannerSnapshot {
    SelectRowBannerSnapshot {
        texture: SkinTextureId(texture),
        size: SkinImageSize { width: 1.0, height: 1.0 },
        lease: Arc::new(()),
    }
}

fn seed(runtime: &mut RowBannerRuntime, index: usize, key: Arc<ImageKey>) {
    runtime.sources.get_mut(&runtime.wanted[index]).unwrap().revision = Some(key.clone());
    runtime.complete(
        Completion::Decoded { epoch: runtime.epoch, key, result: Ok(pixels()) },
        Instant::now(),
    );
}

#[test]
fn no_definition_means_no_pool_and_unchanged_windows_reuse_metadata() {
    let mut runtime = RowBannerRuntime::default();
    runtime.tick(Instant::now());
    assert!(runtime.pool.is_none());
    let sources = [(SourceRef { folder: "song", file: "banner.png" }, true)];
    runtime.reconcile(sources.into_iter());
    let window = runtime.window.as_ptr();
    let source = runtime.wanted[0].clone();
    runtime.reconcile(sources.into_iter());
    assert_eq!(runtime.window.as_ptr(), window);
    assert!(Arc::ptr_eq(&runtime.wanted[0], &source));
}

#[test]
fn aliases_share_one_decode_and_one_upload_but_prefetch_stays_on_cpu() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    let a = SourceRef { folder: "song", file: "a.png" };
    let b = SourceRef { folder: "song", file: "./a.png" };
    runtime.reconcile([(a, true), (b, true)].into_iter());
    let shared = key(1);
    for state in runtime.sources.values_mut() {
        state.revision = Some(shared.clone());
        state.checked = Some(now);
    }
    assert!(matches!(runtime.next_job(now), Some(Job::Decode { .. })));
    assert!(runtime.next_job(now).is_none(), "same physical revision must not decode twice");
    runtime.complete(Completion::Decoded { epoch: 0, key: shared, result: Ok(pixels()) }, now);
    let mut uploads = 0;
    runtime.upload_ready(now, |_| {
        uploads += 1;
        Some(view(400))
    });
    assert_eq!(uploads, 1);
    assert_eq!(runtime.image(a).unwrap().texture, runtime.image(b).unwrap().texture);
    runtime.reconcile([(a, false), (b, false)].into_iter());
    runtime.deactivate();
    runtime.collect_retired(|_| {});
    runtime.reconcile([(a, false)].into_iter());
    runtime.upload_ready(now, |_| panic!("prefetch must not upload"));
    assert!(runtime.image(a).is_none());
}

#[test]
fn bounded_jobs_negative_retry_refresh_and_obsolete_generations() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    let demands = ["a", "b", "c"].map(|file| (SourceRef { folder: "song", file }, true));
    runtime.reconcile(demands.into_iter());
    let Some(Job::Resolve { epoch, source }) = runtime.next_job(now) else { panic!() };
    let Some(Job::Resolve { source: other, .. }) = runtime.next_job(now) else { panic!() };
    assert!(runtime.next_job(now).is_none());
    runtime.complete(Completion::Resolved { epoch, source: source.clone(), result: Ok(None) }, now);
    assert!(
        matches!(runtime.next_job(now), Some(Job::Resolve { source: third, .. }) if third.file == "c")
    );
    assert!(runtime.next_job(now + RETRY).is_none(), "two old jobs still occupy the pool");
    runtime.invalidate();
    runtime.reconcile(demands.into_iter());
    assert!(runtime.next_job(now).is_none());
    runtime.complete(Completion::Resolved { epoch, source: other, result: Ok(Some(key(8))) }, now);
    assert!(runtime.sources.values().all(|s| s.revision.is_none()));
    assert!(runtime.next_job(now).is_some());

    let mut runtime = RowBannerRuntime::default();
    runtime.reconcile(demands[..1].iter().copied());
    let Some(Job::Resolve { epoch, source }) = runtime.next_job(now) else { panic!() };
    runtime.complete(Completion::Resolved { epoch, source, result: Ok(None) }, now);
    assert!(runtime.next_job(now + RETRY - Duration::from_millis(1)).is_none());
    assert!(runtime.next_job(now + RETRY).is_some());
    runtime.active = 0;
    runtime.sources.values_mut().for_each(|s| {
        s.loading = false;
        s.revision = Some(key(3));
        s.checked = Some(now);
    });
    seed(&mut runtime, 0, key(3));
    assert!(runtime.next_job(now + REFRESH - Duration::from_millis(1)).is_none());
    assert!(matches!(runtime.next_job(now + REFRESH), Some(Job::Resolve { .. })));
}

#[test]
fn out_of_order_decodes_never_assign_the_previous_rows_image_to_a_new_row() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    let old = SourceRef { folder: "song", file: "old" };
    let current = SourceRef { file: "current", ..old };
    runtime.reconcile([(old, true)].into_iter());
    runtime.sources.get_mut(&runtime.wanted[0]).unwrap().revision = Some(key(1));
    let Some(Job::Decode { epoch, key: old_key }) = runtime.next_job(now) else { panic!() };
    runtime.reconcile([(current, true)].into_iter());
    runtime.sources.get_mut(&runtime.wanted[0]).unwrap().revision = Some(key(2));
    let Some(Job::Decode { key: current_key, .. }) = runtime.next_job(now) else { panic!() };
    runtime.complete(Completion::Decoded { epoch, key: current_key, result: Ok(pixels()) }, now);
    runtime.upload_ready(now, |_| Some(view(2)));
    runtime.complete(Completion::Decoded { epoch, key: old_key, result: Ok(pixels()) }, now);
    runtime.upload_ready(now, |_| panic!("old row must not upload"));
    assert_eq!(runtime.image(current).unwrap().texture, SkinTextureId(2));
    assert!(runtime.image(old).is_none());
    runtime.invalidate();
    runtime.complete(Completion::Decoded { epoch, key: key(3), result: Ok(pixels()) }, now);
    assert!(runtime.cpu.is_empty());
}

#[test]
fn corrupt_image_decodes_use_negative_cache_until_retry() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    runtime.reconcile([(SourceRef { folder: "song", file: "broken.png" }, true)].into_iter());
    let source = runtime.wanted[0].clone();
    runtime.sources.get_mut(&source).unwrap().revision = Some(key(1));
    let Some(Job::Decode { epoch, key }) = runtime.next_job(now) else { panic!() };
    runtime
        .complete(Completion::Decoded { epoch, key, result: Err(anyhow::anyhow!("corrupt")) }, now);
    for elapsed in [Duration::ZERO, RETRY - Duration::from_millis(1)] {
        runtime.sources.get_mut(&source).unwrap().checked = Some(now + elapsed);
        assert!(runtime.next_job(now + elapsed).is_none());
    }
    assert!(matches!(runtime.next_job(now + RETRY), Some(Job::Decode { .. })));
}

#[test]
fn failed_uploads_back_off_and_each_frame_uploads_at_most_two() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    let demands = ["a", "b", "c"].map(|file| (SourceRef { folder: "song", file }, true));
    runtime.reconcile(demands.into_iter());
    for n in 0..3 {
        seed(&mut runtime, n, key(n as u64));
    }
    let mut attempts = 0;
    runtime.upload_ready(now, |_| {
        attempts += 1;
        None
    });
    assert_eq!(attempts, 2);
    runtime.upload_ready(now, |_| {
        attempts += 1;
        None
    });
    assert_eq!(attempts, 3);
    runtime.upload_ready(now + RETRY - Duration::from_millis(1), |_| panic!("retry too early"));
    runtime.upload_ready(now + RETRY, |_| {
        attempts += 1;
        Some(view(attempts))
    });
    assert_eq!(attempts, 5);
    assert_eq!(runtime.gpu.len(), 2);
}

#[test]
fn snapshot_leases_pin_retired_textures_and_keep_them_in_the_budget() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime { gpu_limit: 4, ..Default::default() };
    let a = SourceRef { folder: "song", file: "a" };
    runtime.reconcile([(a, true)].into_iter());
    seed(&mut runtime, 0, key(1));
    runtime.upload_ready(now, |_| Some(view(1)));
    let old_scene = runtime.image(a).unwrap();
    runtime.reconcile([(SourceRef { file: "b", ..a }, true)].into_iter());
    seed(&mut runtime, 0, key(2));
    runtime.upload_ready(now, |_| panic!("old scene is still charged"));
    runtime.collect_retired(|_| panic!("old plan still owns its texture"));
    assert_eq!(runtime.gpu_bytes, 4);
    drop(old_scene);
    let mut released = Vec::new();
    runtime.collect_retired(|texture| released.push(texture));
    assert_eq!(released, [SkinTextureId(1)]);
    runtime.upload_ready(now, |_| Some(view(2)));
    assert_eq!(runtime.gpu_bytes, 4);
    runtime.deactivate();
    runtime.collect_retired(|texture| released.push(texture));
    assert_eq!(runtime.gpu_bytes, 0);
    assert_eq!(released, [SkinTextureId(1), SkinTextureId(2)]);
    assert!(!runtime.cpu.is_empty(), "scene changes keep the bounded CPU cache");
}

#[test]
fn ten_thousand_scrolls_keep_image_bytes_and_source_metadata_bounded() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime { cpu_limit: 64, gpu_limit: 32, ..Default::default() };
    for n in 0..10_000 {
        let file = n.to_string();
        runtime.reconcile([(SourceRef { folder: "song", file: &file }, true)].into_iter());
        seed(&mut runtime, 0, key(n));
        runtime.upload_ready(now, |_| Some(view(n as u32 + 1)));
        runtime.collect_retired(|_| {});
        assert!(runtime.sources.len() <= MAX_SOURCES);
        assert!(runtime.cpu_bytes <= 64);
        assert!(runtime.gpu_bytes <= 32);
        assert!(runtime.retired.len() <= 8);
    }
    assert_eq!(runtime.sources.len(), MAX_SOURCES);
}

#[test]
fn tiny_images_also_have_a_cache_entry_limit() {
    let now = Instant::now();
    let mut runtime = RowBannerRuntime::default();
    for n in 0..600 {
        let file = n.to_string();
        runtime.reconcile([(SourceRef { folder: "song", file: &file }, true)].into_iter());
        seed(&mut runtime, 0, key(n));
        runtime.upload_ready(now, |_| Some(view(n as u32 + 1)));
        runtime.collect_retired(|_| {});
        assert!(runtime.cpu.len() <= MAX_SOURCES);
        assert!(runtime.gpu.len() + runtime.retired.len() <= MAX_SOURCES);
    }
    assert_eq!(runtime.cpu.len(), MAX_SOURCES);
}

#[test]
fn native_workers_share_extension_fallback_and_reject_changed_files() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Banner.PNG");
    image::RgbaImage::from_pixel(4, 2, image::Rgba([7, 8, 9, 255])).save(&path).unwrap();
    let folder = temp.path().to_str().unwrap();
    let a = Source { folder: folder.into(), file: "Banner.bmp".into() };
    let b = Source { folder: folder.into(), file: "Banner.PNG".into() };
    let resolved = worker::resolve(&a).unwrap().unwrap();
    assert_eq!(resolved, worker::resolve(&b).unwrap().unwrap());
    assert_eq!(worker::decode(&resolved).unwrap().pixels[..4], [7, 8, 9, 255]);
    std::fs::write(&path, b"changed").unwrap();
    assert!(worker::decode(&resolved).is_err());
    assert_ne!(resolved, worker::resolve(&b).unwrap().unwrap());
}

#[test]
fn real_pool_delivers_a_row_image_without_filesystem_work_in_snapshot_lookup() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("banner.png");
    image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
    let source = SourceRef { folder: temp.path().to_str().unwrap(), file: "banner.png" };
    let mut runtime = RowBannerRuntime::default();
    runtime.reconcile([(source, true)].into_iter());
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime.image(source).is_none() {
        assert!(Instant::now() < deadline);
        runtime.tick(Instant::now());
        runtime.upload_ready(Instant::now(), |_| Some(view(42)));
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::remove_file(path).unwrap();
    assert_eq!(runtime.image(source).unwrap().texture, SkinTextureId(42));
}

#[test]
fn archive_banner_reads_only_its_entry_and_invalidates_container_replacements() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let png = temp.path().join("banner.png");
    image::RgbaImage::from_pixel(3, 1, image::Rgba([5, 6, 7, 255])).save(&png).unwrap();
    let bytes = std::fs::read(png).unwrap();
    let path = temp.path().join("songs.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("song/banner.png", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(&bytes).unwrap();
    zip.finish().unwrap();
    let source = Source { folder: format!("{}!/song", path.display()), file: "banner.bmp".into() };
    let key = worker::resolve(&source).unwrap().unwrap();
    assert_eq!(worker::decode(&key).unwrap().pixels[..4], [5, 6, 7, 255]);
    assert!(!temp.path().join("song").exists(), "no disk extraction");
    std::fs::write(path, b"replacement").unwrap();
    assert!(worker::decode(&key).is_err());
}
