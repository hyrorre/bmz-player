use super::*;
use crate::chart_source::ChartLocator;
use bmz_chart::import::{BmsRandomSource, import_bms_chart_with_random_source};
use std::io::Write;
use std::path::PathBuf;

const CHART: &[u8] = b"#TITLE Direct\n#BPM 120\n#STAGEFILE stage.png\n\
#WAV01 KICK.wav\n#WAV02 snare.wav\n#WAV03 ../shared/key.wav\n#BMP01 bga.bmp\n\
#00111:01\n#00112:02\n#00113:03\n#00104:01\n";

struct Temp(PathBuf);
impl Temp {
    fn new(label: &str) -> Self {
        let mut random = [0_u8; 16];
        getrandom::getrandom(&mut random).unwrap();
        let id = uuid::Builder::from_random_bytes(random).into_uuid();
        let path = std::env::temp_dir().join(format!("bmz-archive-resources-{label}-{id}"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn entries(chart: &'static [u8]) -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("song/chart.bms", chart),
        ("song/kick.WAV", b"kick"),
        ("song/snare.ogg", b"snare"),
        ("song/bga.png", b"bga"),
        ("song/stage.png", b"stage"),
        ("song/unused.wav", b"unused"),
        ("shared/key.wav", b"key"),
        ("other/big.wav", b"other song"),
    ]
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn write_7z(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    let metadata =
        entries.iter().map(|(name, _)| sevenz_rust2::ArchiveEntry::new_file(name)).collect();
    let sources = entries
        .iter()
        .map(|(_, data)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(*data)))
        .collect();
    writer.push_archive_entries(metadata, sources).unwrap();
    writer.finish().unwrap();
}

fn load(path: &Path) -> (crate::chart_source::ResolvedChartSource, PlayableChart) {
    let locator =
        ChartLocator::Archive { container: path.to_path_buf(), entry: "song/chart.bms".into() };
    let resolved = locator.resolve().unwrap();
    let import =
        import_bms_chart_with_random_source(&resolved.path, BmsRandomSource::Seed(Some(1)), true)
            .unwrap();
    resolved.validate_chart_assets(&import.chart).unwrap();
    resolved.load_chart_assets(&import.chart).unwrap();
    (resolved, import.chart)
}

fn sound(chart: &PlayableChart, name: &str) -> PathBuf {
    chart.sounds.iter().find(|sound| sound.path.ends_with(name)).unwrap().path.clone()
}

fn loaded(resources: &SongResourceSet) -> Vec<&'static str> {
    let mut names: Vec<_> = entries(CHART)
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| resources.loaded(name).is_some())
        .collect();
    names.sort();
    names
}

#[test]
fn zip_and_solid_7z_charts_read_only_their_assets_into_memory() {
    let temp = Temp::new("formats");
    let zip = temp.0.join("song.zip");
    let sevenz = temp.0.join("song.7z");
    write_zip(&zip, &entries(CHART));
    write_7z(&sevenz, &entries(CHART));
    for (path, solid) in [(zip, false), (sevenz, true)] {
        assert_eq!(song_archive::inspect(&path, &Default::default()).unwrap().solid, solid);
        let (resolved, chart) = load(&path);
        let resources = resolved.resources().unwrap();
        // Unused files, including what a solid pass decoded on the way, are not kept.
        assert_eq!(
            loaded(resources),
            [
                "shared/key.wav",
                "song/bga.png",
                "song/chart.bms",
                "song/kick.WAV",
                "song/snare.ogg",
                "song/stage.png",
            ]
        );
        // Case and extension fallbacks resolve like an extracted folder on Windows.
        let read = |path: &Path| bmz_song_resource::read(path).unwrap().unwrap().to_vec();
        assert_eq!(read(&sound(&chart, "KICK.wav")), b"kick");
        let snare = sound(&chart, "snare.wav");
        assert_eq!(
            bmz_chart::sound_asset::sound_asset_candidates(&snare),
            [snare.with_extension("ogg")]
        );
        assert_eq!(read(&sound(&chart, "key.wav")), b"key");
        assert!(chart.bga_assets[0].path.ends_with("bga.png"));
        assert_eq!(read(&chart.bga_assets[0].path), b"bga");
        assert!(!temp.0.join("cache").exists());
    }
}

#[test]
fn reloading_an_unchanged_archive_reuses_its_set_and_a_rewrite_replaces_it() {
    let temp = Temp::new("reuse");
    let path = temp.0.join("song.zip");
    write_zip(&path, &entries(CHART));
    let (first, first_chart) = load(&path);
    let (second, _) = load(&path);
    assert!(Arc::ptr_eq(first.resources().unwrap(), second.resources().unwrap()));

    let mut changed = entries(CHART);
    changed[1].1 = b"kick v2";
    write_zip(&path, &changed);
    let (third, chart) = load(&path);
    assert!(!Arc::ptr_eq(first.resources().unwrap(), third.resources().unwrap()));
    let kick = sound(&chart, "KICK.wav");
    assert_eq!(&*bmz_song_resource::read(&kick).unwrap().unwrap(), b"kick v2");
    // Use the same path-based API as the decoders, including delayed opens on another thread.
    let old_kick = sound(&first_chart, "KICK.wav");
    assert_eq!(
        &*std::thread::spawn(move || bmz_song_resource::read(&old_kick).unwrap().unwrap())
            .join()
            .unwrap(),
        b"kick"
    );
}

#[test]
fn select_prefetch_reuses_the_play_set_and_still_drops_unused_entries() {
    let temp = Temp::new("prefetch");
    let path = temp.0.join("song.7z");
    write_7z(&path, &entries(CHART));
    prefetch_chart(&path, "song/chart.bms", &Default::default()).unwrap();
    let prefetched = bmz_song_resource::find(&path).unwrap();
    assert!(prefetched.loaded("song/unused.wav").is_some());
    let pinned = prefetched.path("song/chart.bms");
    drop(prefetched);
    let (resolved, _) = load(&path);
    assert_eq!(resolved.path, pinned);
    assert!(resolved.resources().unwrap().loaded("song/unused.wav").is_none());
    assert!(resolved.resources().unwrap().loaded("song/kick.WAV").is_some());
}

#[test]
fn concurrent_first_archive_loads_share_one_set() {
    let temp = Temp::new("concurrent");
    let path = temp.0.join("song.7z");
    write_7z(&path, &entries(CHART));
    let barrier = std::sync::Barrier::new(8);
    let opened = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    open_chart(&path, "song/chart.bms", &Default::default()).unwrap()
                })
            })
            .collect();
        workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>()
    });
    assert!(opened.iter().all(|item| Arc::ptr_eq(&item.resources, &opened[0].resources)));
}

#[test]
fn a_prefetch_waiting_for_an_archive_reader_can_be_cancelled() {
    let temp = Temp::new("cancel-prefetch");
    let path = temp.0.join("song.zip");
    write_zip(&path, &entries(CHART));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let held_path = &path;
        let held = scope.spawn(move || {
            prefetch::with_load_lock(held_path, &Default::default(), || {
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .unwrap()
        });
        ready_rx.recv().unwrap();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let control = ArchiveControl { cancelled: Some(cancel), ..Default::default() };
        assert!(prefetch::with_load_lock(&path, &control, || Ok(())).is_err());
        release_tx.send(()).unwrap();
        held.join().unwrap();
    });
}

#[test]
fn chart_assets_outside_the_archive_are_rejected() {
    let temp = Temp::new("escape");
    let path = temp.0.join("song.zip");
    write_zip(&path, &[("song/chart.bms", b"#WAV01 ../../outside.wav\n#00111:01\n")]);
    let locator = ChartLocator::Archive { container: path.clone(), entry: "song/chart.bms".into() };
    let resolved = locator.resolve().unwrap();
    let import =
        import_bms_chart_with_random_source(&resolved.path, BmsRandomSource::Seed(None), true)
            .unwrap();
    assert!(resolved.validate_chart_assets(&import.chart).is_err());
    assert!(bmz_song_resource::read(&import.chart.sounds[0].path).unwrap().is_err());
}

#[test]
fn delayed_audio_image_and_video_decodes_keep_the_loaded_archive_generation() {
    use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
    use bmz_audio::loader::SampleLoader;
    let temp = Temp::new("decode-generations");
    let path = temp.0.join("song.zip");
    let chart = b"#TITLE Pinned\n#BPM 120\n#STAGEFILE bga.png\n#WAV01 kick.bin\n#BMP01 bga.png\n#BMP02 clip.y4m\n#00111:01\n#00104:01\n#00107:02\n";
    let write = |value: i16, color: [u8; 4]| {
        let mut wav = b"RIFF".to_vec();
        wav.extend(132_u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(48_000_u32.to_le_bytes());
        wav.extend(96_000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(96_u32.to_le_bytes());
        for _ in 0..48 {
            wav.extend(value.to_le_bytes());
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba(color))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        // The same raw-video fixture format used by bmz-video's decoder tests.
        let mut video = b"YUV4MPEG2 W2 H2 F10:1 Ip A1:1 C420jpeg\nFRAME\n".to_vec();
        let luma = if value > 0 { 16 } else { 235 };
        video.extend([luma, luma, luma, luma, 128, 128]);
        write_zip(
            &path,
            &[
                ("song/chart.bms", chart),
                ("song/kick.bin", &wav),
                ("song/bga.png", png.get_ref()),
                ("song/clip.y4m", &video),
            ],
        );
    };
    write(8192, [255, 0, 0, 255]);
    let (old, old_chart) = load(&path);
    write(-8192, [0, 0, 255, 255]);
    let (new, new_chart) = load(&path);
    let worker = std::thread::spawn(move || {
        for (source, chart, sample, color, video_color) in [
            (old, old_chart, 0.25, [255, 0, 0, 255], [0, 0, 0, 255]),
            (new, new_chart, -0.25, [0, 0, 255, 255], [255, 255, 255, 255]),
        ] {
            // The non-WAV extension exercises the FFmpeg memory input too.
            let decoded = FfmpegSampleLoader::default().load(&chart.sounds[0].path).unwrap();
            assert!(!decoded.frames.is_empty());
            assert!((decoded.frames[0] - sample).abs() < 0.001);
            let stage = source.resolve_asset(&chart.metadata.stage_file).unwrap();
            let image = bmz_render::assets::load_static_rgba_image(&stage).unwrap();
            assert_eq!(&image.pixels[..4], &color);
            let video = &chart
                .bga_assets
                .iter()
                .find(|asset| asset.path.ends_with("clip.y4m"))
                .unwrap()
                .path;
            // Delayed video opening and reopening must use the same old/new bytes.
            for _ in 0..2 {
                let frame = bmz_video::decode_first_frame(video).unwrap();
                assert_eq!(&frame.rgba[..4], &video_color);
            }
        }
    });
    worker.join().unwrap();
}
