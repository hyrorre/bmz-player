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
    let (first, _) = load(&path);
    let (second, _) = load(&path);
    assert!(Arc::ptr_eq(first.resources().unwrap(), second.resources().unwrap()));

    let mut changed = entries(CHART);
    changed[1].1 = b"kick v2";
    write_zip(&path, &changed);
    let (third, chart) = load(&path);
    assert!(!Arc::ptr_eq(first.resources().unwrap(), third.resources().unwrap()));
    let kick = sound(&chart, "KICK.wav");
    assert_eq!(&*bmz_song_resource::read(&kick).unwrap().unwrap(), b"kick v2");
    // The earlier play keeps reading its own generation.
    assert_eq!(&*first.resources().unwrap().read("song/kick.WAV").unwrap(), b"kick");
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
