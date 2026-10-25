//! Charts inside song archives read their assets from memory instead of an extracted copy.
//!
//! A chart load opens the archive's [`SongResourceSet`], reads what the chart uses in as few
//! passes as the format allows and installs it. Chart import, audio, image and video loaders
//! then find the entries by their `pack.zip!/entry` paths through `bmz_song_resource`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bmz_chart::model::PlayableChart;
use bmz_song_resource::{SongPath, SongResourceSet};

use crate::song_archive::{self, ArchiveControl, ArchiveEntry, ArchiveGeneration, ArchiveIndex};

/// A set holding more than this drops the entries the next chart does not use.
const SET_TRIM_BYTES: u64 = 1 << 30;

/// Lets loaders read an archive no chart load installed, e.g. a Select image or an
/// evicted set. Entries are then read one at a time.
pub fn install_fallback() {
    bmz_song_resource::set_fallback(|container| {
        let index = song_archive::inspect(container, &ArchiveControl::default())
            .map_err(std::io::Error::other)?;
        Ok(Arc::new(new_set(container, &index)))
    });
}

fn new_set(container: &Path, index: &ArchiveIndex) -> SongResourceSet {
    let source = container.to_path_buf();
    let generation = index.generation.clone();
    let names = index.entries.iter().filter(|entry| !entry.is_directory);
    SongResourceSet::new(
        container,
        index.generation.fingerprint.clone(),
        names.map(|entry| entry.name.clone()),
    )
    .with_fetch(move |names| {
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        let pick = |entry: &ArchiveEntry| wanted.contains(entry.name.as_str());
        song_archive::read_entries(&source, Some(&generation), &ArchiveControl::default(), &pick)
            .map(|(_, entries)| entries)
            .map_err(std::io::Error::other)
    })
}

/// A chart's archive set with the chart entry in memory.
pub struct OpenedChart {
    pub resources: Arc<SongResourceSet>,
    /// Entries read only because a solid archive decodes them on the way; dropped by
    /// [`load_chart_assets`] unless the chart uses them.
    speculative: Vec<String>,
}

/// Installs the set for `container` with `chart` loaded, reusing the installed one while the
/// archive is unchanged. Solid archives also read the chart's folder in the same pass, since
/// every later read decodes the archive from the start again.
pub fn open_chart(container: &Path, chart: &str, control: &ArchiveControl) -> Result<OpenedChart> {
    let index = song_archive::inspect(container, control)?;
    let resources = match bmz_song_resource::find(container) {
        Some(set) if set.generation() == index.generation.fingerprint => set,
        _ => Arc::new(new_set(container, &index)),
    };
    let mut speculative = Vec::new();
    // A folder locator has no chart entry to read.
    if resources.contains(chart) && resources.loaded(chart).is_none() {
        let folder = chart.rsplit_once('/').map_or("", |(folder, _)| folder);
        let solid = index.solid;
        let pick =
            |entry: &ArchiveEntry| entry.name == chart || (solid && in_folder(&entry.name, folder));
        speculative = load(&resources, container, &index.generation, control, &pick)?;
        speculative.retain(|name| name != chart);
    }
    bmz_song_resource::install(resources.clone());
    Ok(OpenedChart { resources, speculative })
}

fn in_folder(name: &str, folder: &str) -> bool {
    folder.is_empty() || name.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
}

/// Reads picked entries that are not in memory yet and returns their names.
fn load(
    resources: &SongResourceSet,
    container: &Path,
    generation: &ArchiveGeneration,
    control: &ArchiveControl,
    pick: &dyn Fn(&ArchiveEntry) -> bool,
) -> Result<Vec<String>> {
    let unloaded = |entry: &ArchiveEntry| resources.loaded(&entry.name).is_none() && pick(entry);
    let (_, entries) = song_archive::read_entries(container, Some(generation), control, &unloaded)?;
    let names = entries.iter().map(|(name, _)| name.clone()).collect();
    for (name, bytes) in entries {
        resources.insert(&name, bytes);
    }
    Ok(names)
}

/// Reads every entry `chart` uses that is not in memory yet in one pass: sound candidates in
/// beatoraja's fallback order, BGA files and the STAGEFILE / BACKBMP / BANNER images.
pub fn load_chart_assets(
    opened: &OpenedChart,
    chart_path: &Path,
    chart: &PlayableChart,
    control: &ArchiveControl,
) -> Result<()> {
    let resources = &opened.resources;
    let mut used = chart_asset_entries(resources, chart_path, chart);
    if let SongPath::Archive { entry, .. } = SongPath::parse(chart_path)
        && let Some(name) = resources.entry_name(&entry)
    {
        used.insert(name.to_owned());
    }
    let speculative: HashSet<&str> = opened.speculative.iter().map(String::as_str).collect();
    let trim = resources.loaded_bytes() > SET_TRIM_BYTES;
    resources.retain_loaded(|name| used.contains(name) || (!trim && !speculative.contains(name)));
    let missing: HashSet<String> =
        resources.unloaded(used.iter().map(String::as_str)).into_iter().collect();
    if !missing.is_empty() {
        let index = song_archive::inspect(resources.container(), control)?;
        anyhow::ensure!(
            index.generation.fingerprint == resources.generation(),
            "song archive changed during chart load"
        );
        load(resources, resources.container(), &index.generation, control, &|entry| {
            missing.contains(&entry.name)
        })?;
    }
    Ok(())
}

/// Actual archive entry names of the assets `chart` may read.
fn chart_asset_entries(
    resources: &SongResourceSet,
    chart_path: &Path,
    chart: &PlayableChart,
) -> HashSet<String> {
    let mut entries = HashSet::new();
    let mut add = |path: &Path| {
        if let SongPath::Archive { container, entry } = SongPath::parse(path)
            && container == resources.container()
            && let Some(name) = resources.entry_name(&entry)
        {
            entries.insert(name.to_owned());
        }
    };
    let sounds: HashSet<&Path> = chart.sounds.iter().map(|sound| sound.path.as_path()).collect();
    for sound in sounds {
        // Decoding falls back to the next candidate when one fails.
        for candidate in bmz_chart::sound_asset::sound_asset_candidates(sound) {
            add(&candidate);
        }
    }
    for asset in &chart.bga_assets {
        add(&asset.path);
    }
    let folder = chart_path.parent().unwrap_or(Path::new("")).to_string_lossy();
    for relative in
        [&chart.metadata.stage_file, &chart.metadata.backbmp_file, &chart.metadata.banner_file]
    {
        if let Some(path) = crate::chart_asset::resolve_chart_asset_path(&folder, relative) {
            add(&path);
        }
    }
    entries
}

#[cfg(test)]
#[path = "archive_resources/tests.rs"]
mod tests;
