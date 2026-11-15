use super::*;
use crate::chart_source::ChartLocator;
use crate::song_archive::{self, ArchiveControl, ArchiveGeneration, ArchiveStamp};
use anyhow::{Context, Result, ensure};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum ImageKey {
    File { path: PathBuf, stamp: ArchiveStamp },
    Archive { path: PathBuf, stamp: ArchiveStamp, generation: String, entry: String },
}

impl ImageKey {
    fn path(&self) -> &Path {
        match self {
            Self::File { path, .. } | Self::Archive { path, .. } => path,
        }
    }

    fn verify(&self) -> Result<()> {
        let expected = match self {
            Self::File { stamp, .. } | Self::Archive { stamp, .. } => stamp,
        };
        ensure!(&song_archive::metadata_stamp(self.path())? == expected, "banner source changed");
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum Job {
    Resolve { epoch: u64, source: Arc<Source> },
    Decode { epoch: u64, key: Arc<ImageKey> },
}

pub(super) enum Completion {
    Resolved { epoch: u64, source: Arc<Source>, result: Result<Option<Arc<ImageKey>>> },
    Decoded { epoch: u64, key: Arc<ImageKey>, result: Result<Arc<RgbaImageAsset>> },
}

pub(super) struct Pool {
    pub tx: std::sync::mpsc::SyncSender<Job>,
    pub rx: std::sync::mpsc::Receiver<Completion>,
}

impl Pool {
    pub fn new() -> std::io::Result<Self> {
        let (tx, jobs) = std::sync::mpsc::sync_channel::<Job>(WORKERS);
        let jobs = Arc::new(std::sync::Mutex::new(jobs));
        let (results, rx) = std::sync::mpsc::sync_channel(WORKERS);
        for number in 0..WORKERS {
            let jobs = jobs.clone();
            let results = results.clone();
            std::thread::Builder::new().name(format!("select-row-banner-{number}")).spawn(
                move || {
                    crate::generated_preview::lower_current_thread_priority();
                    loop {
                        let job = jobs.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok(job) = job else {
                            break;
                        };
                        let completion = match job {
                            Job::Resolve { epoch, source } => {
                                let result = resolve(&source);
                                Completion::Resolved { epoch, source, result }
                            }
                            Job::Decode { epoch, key } => {
                                let result = decode(&key).map(Arc::new);
                                Completion::Decoded { epoch, key, result }
                            }
                        };
                        if results.send(completion).is_err() {
                            break;
                        }
                    }
                },
            )?;
        }
        Ok(Self { tx, rx })
    }
}

pub(super) fn resolve(source: &Source) -> Result<Option<Arc<ImageKey>>> {
    let locator = ChartLocator::parse(Path::new(&source.folder))?.canonicalize()?;
    let key = match locator {
        ChartLocator::Archive { container, entry: folder } => {
            let control = ArchiveControl::default();
            let stamp = song_archive::metadata_stamp(&container)?;
            let index = song_archive::inspect_shared(&container, &control)?;
            let Some(entry) =
                crate::chart_asset::archive_asset_entry(&index, &folder, &source.file, false)?
            else {
                return Ok(None);
            };
            ensure!(
                index.entries.iter().any(|e| e.name == entry && e.size <= 16 * 1024 * 1024),
                "banner archive entry exceeds 16 MiB"
            );
            ensure!(
                song_archive::metadata_stamp(&container)? == stamp,
                "banner archive changed during resolution"
            );
            ImageKey::Archive {
                path: container.clone(),
                stamp,
                generation: index.generation.fingerprint.clone(),
                entry,
            }
        }
        ChartLocator::File(_) => {
            let Some(path) =
                crate::chart_asset::resolve_chart_asset_path(&source.folder, &source.file)
            else {
                return Ok(None);
            };
            let path = std::fs::canonicalize(path)?;
            ImageKey::File { stamp: song_archive::metadata_stamp(&path)?, path }
        }
    };
    Ok(Some(Arc::new(key)))
}

pub(super) fn decode(key: &ImageKey) -> Result<RgbaImageAsset> {
    key.verify()?;
    let image = match key {
        ImageKey::File { path, .. } => bmz_render::assets::load_chart_thumbnail(path)?,
        ImageKey::Archive { path, generation, entry, .. } => {
            let generation = ArchiveGeneration { fingerprint: generation.clone() };
            let (_, mut entries) = song_archive::read_entries_limited(
                path,
                Some(&generation),
                &ArchiveControl::default(),
                &|item| item.name == *entry,
                Some(16 * 1024 * 1024),
            )?;
            let (_, bytes) = entries.pop().context("banner archive entry disappeared")?;
            bmz_render::assets::decode_chart_thumbnail(&bytes)?
        }
    };
    key.verify()?;
    Ok(image)
}
