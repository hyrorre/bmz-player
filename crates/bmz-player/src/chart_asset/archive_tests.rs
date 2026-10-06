use super::*;
use std::io::{Cursor, Write};

pub(crate) struct ArchiveFixture {
    pub root: PathBuf,
    pub archive: PathBuf,
    pub locator: PathBuf,
}

impl ArchiveFixture {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "bmz-archive-app-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("songs.zip");
        let locator = PathBuf::from(format!("{}!/song/chart.bms", archive.display()));
        let fixture = Self { root, archive, locator };
        fixture.write(3_000, 12_000);
        fixture
    }

    pub(crate) fn write(&self, amplitude: i16, frames: usize) {
        let mut wav = Vec::new();
        let size = (frames * 2) as u32;
        wav.extend(b"RIFF");
        wav.extend((36 + size).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(48_000_u32.to_le_bytes());
        wav.extend(96_000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(size.to_le_bytes());
        for i in 0..frames {
            wav.extend((if i % 64 < 32 { amplitude } else { -amplitude }).to_le_bytes());
        }
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([255, 0, 0, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&self.archive).unwrap());
        for (name, bytes) in [
            ("song/chart.bms", b"#TITLE Archive app\n#BPM 120\n#WAV01 ../shared/key.wav\n#STAGEFILE stage.bmp\n#BACKBMP stage.bmp\n#BANNER stage.bmp\n#BMP01 stage.png\n#BMP02 clip.mp4\n#00011:01\n#00004:01\n#00104:02\n".as_slice()),
            ("shared/key.wav", wav.as_slice()),
            ("song/preview.wav", wav.as_slice()),
            ("song/stage.png", png.get_ref().as_slice()),
            ("song/clip.mp4", b"video fixture path only".as_slice()),
            ("song/readme.txt", b"archive fixture".as_slice()),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn folder(&self) -> String {
        format!("{}!/song", self.archive.display())
    }
}

impl Drop for ArchiveFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn archive_cli_canonicalizes_container_and_guides_bare_archive() {
    let fixture = ArchiveFixture::new();
    let path = canonical_chart_path(&fixture.locator).unwrap();
    let locator = crate::chart_source::ChartLocator::parse(&path).unwrap();
    assert!(locator.is_archive());
    assert_eq!(
        locator.container_path().canonicalize().unwrap(),
        fixture.archive.canonicalize().unwrap()
    );
    assert!(path.to_string_lossy().ends_with("!/song/chart.bms"));
    let error = canonical_chart_path(&fixture.archive).unwrap_err().to_string();
    assert!(error.contains("songs load"));
    assert!(
        canonical_chart_path(&PathBuf::from(format!(
            "{}!/../../escape.bms",
            fixture.archive.display()
        )))
        .is_err()
    );
    assert!(!fixture.root.join("cache").exists());
}

#[test]
fn archive_asset_workers_resolve_fallbacks_and_confine_shared_resources() {
    let fixture = ArchiveFixture::new();
    let cache = fixture.root.join("cache");
    let key = asset_cache_key(&fixture.folder(), "stage.bmp");
    let image = resolve_cached_asset(&key, &cache, false).unwrap().unwrap();
    assert_eq!(image.extension().unwrap(), "png");
    assert_eq!(bmz_render::assets::load_static_rgba_image(&image).unwrap().width, 1);
    let preview = resolve_cached_asset(&asset_cache_key(&fixture.folder(), ""), &cache, true)
        .unwrap()
        .unwrap();
    assert_eq!(preview.file_name().unwrap(), "preview.wav");
    let shared = resolve_cached_asset(
        &asset_cache_key(&fixture.folder(), "../shared/key.ogg"),
        &cache,
        true,
    )
    .unwrap()
    .unwrap();
    assert_eq!(shared.file_name().unwrap(), "key.wav");
    let windows_shared = resolve_cached_asset(
        &asset_cache_key(&fixture.folder(), "..\\shared\\key.ogg"),
        &cache,
        true,
    )
    .unwrap()
    .unwrap();
    assert_eq!(windows_shared, shared);
    assert!(
        resolve_cached_asset(
            &asset_cache_key(&fixture.folder(), "../../escape.png"),
            &cache,
            false
        )
        .is_err()
    );
    assert!(
        resolve_cached_asset(
            &asset_cache_key(
                &fixture.folder(),
                &fixture.root.join("outside.png").to_string_lossy()
            ),
            &cache,
            false
        )
        .is_err()
    );
}

#[test]
fn archive_asset_only_update_invalidates_images_and_both_preview_keys() {
    let fixture = ArchiveFixture::new();
    let cache = fixture.root.join("cache");
    let image_key = asset_cache_key(&fixture.folder(), "stage.bmp");
    let preview_key = asset_cache_key(&fixture.folder(), "preview.wav");
    let generated =
        crate::generated_preview::generated_preview_cache_key_for_source(1, 0, &fixture.folder());
    let first = resolve_cached_asset(&preview_key, &cache, true).unwrap().unwrap();
    let first_bytes = std::fs::read(&first).unwrap();
    fixture.write(9_000, 16_000);
    assert_ne!(image_key, asset_cache_key(&fixture.folder(), "stage.bmp"));
    assert_ne!(preview_key, asset_cache_key(&fixture.folder(), "preview.wav"));
    let next_generated =
        crate::generated_preview::generated_preview_cache_key_for_source(1, 0, &fixture.folder());
    assert_ne!(generated, next_generated);
    assert!(crate::generated_preview::parse_generated_preview_cache_key(&next_generated).is_some());
    assert!(resolve_cached_asset(&preview_key, &cache, true).is_err());
    let next_key = asset_cache_key(&fixture.folder(), "preview.wav");
    let second = resolve_cached_asset(&next_key, &cache, true).unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
    assert_ne!(std::fs::read(&second).unwrap(), first_bytes);
    std::fs::remove_dir_all(&cache).unwrap();
    assert_eq!(resolve_cached_asset(&next_key, &cache, true).unwrap().unwrap(), second);
    assert!(second.is_file());
}

#[test]
fn archive_preload_preserves_identity_and_copy_assets_for_practice_replay_and_course() {
    use crate::screens::play_session::*;
    use crate::storage::{
        library_db::LibraryDatabase,
        migration::{LIBRARY_MIGRATIONS, run_migrations},
    };
    let first = ArchiveFixture::new();
    let second = ArchiveFixture::new();
    second.write(9_000, 16_000);
    let mut db = LibraryDatabase::open(Path::new(":memory:")).unwrap();
    run_migrations(db.conn_mut(), LIBRARY_MIGRATIONS).unwrap();
    let a =
        crate::storage::import::import_chart_file(&mut db, &first.locator, None, None, 0).unwrap();
    let b =
        crate::storage::import::import_chart_file(&mut db, &second.locator, None, None, 0).unwrap();
    assert_eq!(a.chart.identity, b.chart.identity);
    assert_ne!(a.chart_id, b.chart_id);
    let cache = first.root.join("cache");
    assert!(!cache.exists());
    assert!(
        scored_chart_metrics_for_chart(&db, a.chart_id, &PlaySessionOptions::default())
            .unwrap()
            .total_notes
            > 0
    );
    assert!(load_source_chart_for_chart(&db, a.chart_id, None).is_ok());
    assert!(!cache.exists());
    let options =
        PlaySessionOptions { archive_cache_dir: Some(cache.clone()), ..Default::default() };
    db.write_chart_normalization_analysis(
        a.chart_id,
        crate::storage::library_db::ChartNormalizationAnalysis {
            loudness_lufs: -80.0,
            short_term_lufs: -80.0,
            sample_peak: 1000.0,
        },
    )
    .unwrap();
    let published = std::cell::Cell::new(false);
    let a_play = preload_play_session_for_chart_with_callbacks(
        &db,
        a.chart_id,
        options.clone(),
        1.0,
        |prepared| {
            for relative in [
                &prepared.chart.metadata.stage_file,
                &prepared.chart.metadata.backbmp_file,
                &prepared.chart.metadata.banner_file,
            ] {
                assert!(Path::new(relative).is_absolute());
                let path = resolve_chart_asset_path("", relative).unwrap();
                assert_eq!(bmz_render::assets::load_static_rgba_image(&path).unwrap().width, 1);
            }
            published.set(true);
        },
        |_, _| assert!(published.get(), "metadata images must be available before audio progress"),
    )
    .unwrap();
    assert!(published.get());
    let b_play = preload_play_session_for_chart(&db, b.chart_id, options.clone(), 1.0).unwrap();
    assert!(a_play.audio.samples.source_count() > 0);
    assert!(
        a_play.chart_normalization_gain > 0.01,
        "archive must ignore analysis without a resource generation"
    );
    assert!(b_play.audio.samples.source_count() > 0);
    assert_eq!(a_play.score_key, b_play.score_key);
    assert_ne!(a_play.chart.sounds[0].path, b_play.chart.sounds[0].path);
    assert_ne!(
        std::fs::read(&a_play.chart.sounds[0].path).unwrap(),
        std::fs::read(&b_play.chart.sounds[0].path).unwrap()
    );
    assert!(Path::new(&a_play.chart.metadata.stage_file).is_absolute());
    let canonical_cache = cache.canonicalize().unwrap();
    assert!(
        a_play.chart.bga_assets.iter().all(|asset| asset
            .path
            .canonicalize()
            .unwrap()
            .starts_with(&canonical_cache))
    );
    assert!(
        a_play
            .chart
            .bga_assets
            .iter()
            .any(|asset| asset.kind == bmz_chart::model::BgaAssetKind::Video)
    );
    let mut replay_options = options.clone();
    replay_options.bms_random_choices = Some(a_play.applied_arrange.bms_random_choices.clone());
    replay_options.bms_switch_choices = Some(a_play.applied_arrange.bms_switch_choices.clone());
    let replay = preload_play_session_for_chart(&db, a.chart_id, replay_options, 1.0).unwrap();
    assert_eq!(replay.chart.identity, a.chart.identity);
    let practice = build_practice_prepared_from_preloaded(
        a_play,
        &crate::config::profile_config::ProfileConfig::new_default("default", "Archive Test", 0),
        &Default::default(),
        options.clone(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    assert_eq!(practice.session.chart.identity, a.chart.identity);
    std::fs::remove_dir_all(&cache).unwrap();
    let reloaded = preload_play_session_for_chart(&db, a.chart_id, options.clone(), 1.0).unwrap();
    assert!(reloaded.audio.samples.source_count() > 0);
    assert!(reloaded.chart.sounds[0].path.is_file());
    std::fs::remove_file(&first.archive).unwrap();
    let error = preload_play_session_for_chart(&db, a.chart_id, options, 1.0).err().unwrap();
    assert!(error.to_string().contains("chart source changed"));
}

#[test]
fn archive_generated_preview_decodes_current_resource_generation() {
    use crate::storage::{
        library_db::LibraryDatabase,
        migration::{LIBRARY_MIGRATIONS, run_migrations},
    };
    let fixture = ArchiveFixture::new();
    let db_path = fixture.root.join("library.db");
    let cache = fixture.root.join("cache");
    let mut db = LibraryDatabase::open(&db_path).unwrap();
    run_migrations(db.conn_mut(), LIBRARY_MIGRATIONS).unwrap();
    let imported =
        crate::storage::import::import_chart_file(&mut db, &fixture.locator, None, None, 0)
            .unwrap();
    let folder = db.list_charts_by_ids(&[imported.chart_id]).unwrap().pop().unwrap().folder_path;
    let first_key = crate::generated_preview::generated_preview_cache_key_for_source(
        imported.chart_id,
        0,
        &folder,
    );
    let first = crate::generated_preview::render_generated_preview_for_chart(
        &db_path,
        &cache,
        imported.chart_id,
        0,
        48_000,
        Some(&first_key),
    )
    .unwrap();
    let peak = |frames: &[f32]| frames.iter().fold(0.0_f32, |peak, value| peak.max(value.abs()));
    assert!(peak(&first.frames) > 0.01);
    fixture.write(9_000, 16_000);
    assert!(
        crate::generated_preview::render_generated_preview_for_chart(
            &db_path,
            &cache,
            imported.chart_id,
            0,
            48_000,
            Some(&first_key)
        )
        .is_err()
    );
    let updated_key = crate::generated_preview::generated_preview_cache_key_for_source(
        imported.chart_id,
        0,
        &folder,
    );
    let updated = crate::generated_preview::render_generated_preview_for_chart(
        &db_path,
        &cache,
        imported.chart_id,
        0,
        48_000,
        Some(&updated_key),
    )
    .unwrap();
    assert!(peak(&updated.frames) > peak(&first.frames) * 2.0);
    assert_eq!(
        db.verified_chart_source(imported.chart_id).unwrap().chart.sha256,
        imported.chart.identity.file_sha256
    );
}
