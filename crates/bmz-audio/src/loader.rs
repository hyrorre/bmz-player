use std::path::{Path, PathBuf};

use bmz_chart::model::{PlayableChart, SoundAssetRef};
use thiserror::Error;

use crate::engine::AudioEngine;
use crate::sample::DecodedSample;

#[derive(Debug, Error)]
pub enum SampleLoadError {
    #[error("failed to read sample file: {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to decode sample file: {path}: {message}")]
    Decode { path: PathBuf, message: String },
}

pub trait SampleLoader {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError>;
}

#[derive(Debug, Clone)]
pub struct LoadedSampleReport {
    pub path: PathBuf,
    pub status: LoadedSampleStatus,
}

#[derive(Debug, Clone)]
pub enum LoadedSampleStatus {
    Loaded,
    Failed(String),
}

pub fn load_chart_samples(
    engine: &mut AudioEngine,
    chart: &PlayableChart,
    loader: &mut dyn SampleLoader,
) -> Vec<LoadedSampleReport> {
    chart.sounds.iter().map(|asset| load_asset(engine, asset, loader)).collect()
}

fn load_asset(
    engine: &mut AudioEngine,
    asset: &SoundAssetRef,
    loader: &mut dyn SampleLoader,
) -> LoadedSampleReport {
    match loader.load(&asset.path) {
        Ok(sample) => {
            engine.insert_sample(asset.id, sample);
            LoadedSampleReport { path: asset.path.clone(), status: LoadedSampleStatus::Loaded }
        }
        Err(error) => LoadedSampleReport {
            path: asset.path.clone(),
            status: LoadedSampleStatus::Failed(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef};
    use bmz_core::ids::SoundId;
    use bmz_core::time::TimeUs;

    use super::*;

    #[derive(Default)]
    struct TestLoader {
        samples: HashMap<PathBuf, DecodedSample>,
    }

    impl SampleLoader for TestLoader {
        fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
            self.samples.get(path).cloned().ok_or_else(|| SampleLoadError::Decode {
                path: path.to_path_buf(),
                message: "missing test sample".to_string(),
            })
        }
    }

    #[test]
    fn load_chart_samples_inserts_loaded_samples_and_reports_failures() {
        let mut engine = AudioEngine::default();
        let chart = chart();
        let mut loader = TestLoader::default();
        loader.samples.insert(
            PathBuf::from("ok.wav"),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0] },
        );

        let report = load_chart_samples(&mut engine, &chart, &mut loader);

        assert_eq!(report.len(), 2);
        assert!(matches!(report[0].status, LoadedSampleStatus::Loaded));
        assert!(matches!(report[1].status, LoadedSampleStatus::Failed(_)));
        assert!(engine.samples.get(SoundId(1)).is_some());
        assert!(engine.samples.get(SoundId(2)).is_none());
    }

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"samples"),
            metadata: ChartMetadata {
                title: "samples".to_string(),
                initial_bpm: 120.0,
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: vec![
                SoundAssetRef { id: SoundId(1), path: PathBuf::from("ok.wav") },
                SoundAssetRef { id: SoundId(2), path: PathBuf::from("missing.wav") },
            ],
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}
