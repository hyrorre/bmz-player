//! Practice mode configuration (beatoraja `PracticeProperty` subset).

use std::path::PathBuf;

use anyhow::{Context, Result};
use bmz_chart::model::{JudgeRankKind, JudgeRankSpec, PlayableChart};
use bmz_chart::practice::apply_practice_section;
use bmz_core::time::TimeUs;
use bmz_gameplay::gauge::GaugeState;
use serde::{Deserialize, Serialize};

use crate::config::profile_config::GaugeTypeConfig;
use crate::paths::ProfilePaths;
use crate::screens::play_session::{AppliedArrange, apply_arrange};
use crate::select_options::ArrangeOption;

/// Persisted / editable practice settings for one chart (SHA-256).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PracticeProperty {
    pub start_time_ms: u32,
    pub end_time_ms: u32,
    pub gauge: GaugeTypeConfig,
    pub start_gauge: u32,
    pub judgerank: i32,
    pub arrange: ArrangeOption,
    pub total: Option<f64>,
}

impl Default for PracticeProperty {
    fn default() -> Self {
        Self {
            start_time_ms: 0,
            end_time_ms: 10_000,
            gauge: GaugeTypeConfig::Normal,
            start_gauge: 20,
            judgerank: 100,
            arrange: ArrangeOption::Normal,
            total: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PracticePhase {
    /// Settings overlay; chart is preloaded but not playing.
    Config,
    /// Active section play.
    Playing,
}

#[derive(Debug, Clone)]
pub struct PracticeSession {
    pub chart_id: i64,
    pub chart_title: String,
    pub chart_sha256: [u8; 32],
    pub property: PracticeProperty,
    pub phase: PracticePhase,
    pub max_end_time_ms: u32,
}

/// CLI-only overrides applied when entering practice from the command line.
#[derive(Debug, Clone, Default)]
pub struct PracticeCliOverrides {
    pub start_time_ms: Option<u32>,
    pub end_time_ms: Option<u32>,
}

pub fn practice_property_path(profile_paths: &ProfilePaths, chart_sha256: &[u8; 32]) -> PathBuf {
    profile_paths.root_dir.join("practice").join(format!("{}.json", sha256_hex(chart_sha256)))
}

pub fn load_practice_property(
    profile_paths: &ProfilePaths,
    chart_sha256: &[u8; 32],
    chart: &PlayableChart,
    profile_gauge: GaugeTypeConfig,
    cli: &PracticeCliOverrides,
) -> Result<PracticeProperty> {
    let path = practice_property_path(profile_paths, chart_sha256);
    let mut property = if path.is_file() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read practice config: {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("parse practice config: {}", path.display()))?
    } else {
        PracticeProperty::default()
    };

    if property.end_time_ms == 10_000 && property.start_time_ms == 0 {
        property.end_time_ms = default_end_time_ms(chart);
    }
    if property.judgerank == 100 {
        property.judgerank = chart.metadata.judge_rank.unwrap_or(100);
    }
    if property.gauge == GaugeTypeConfig::Normal && profile_gauge != GaugeTypeConfig::AutoShift {
        property.gauge = profile_gauge;
    }
    if property.total.is_none() {
        property.total = chart.metadata.total;
    }

    if let Some(start) = cli.start_time_ms {
        property.start_time_ms = start;
    }
    if let Some(end) = cli.end_time_ms {
        property.end_time_ms = end;
    }
    clamp_practice_property(&mut property, chart);

    Ok(property)
}

pub fn save_practice_property(
    profile_paths: &ProfilePaths,
    chart_sha256: &[u8; 32],
    property: &PracticeProperty,
) -> Result<()> {
    let path = practice_property_path(profile_paths, chart_sha256);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create practice dir: {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(property).context("serialize practice property")?;
    std::fs::write(&path, text)
        .with_context(|| format!("write practice config: {}", path.display()))
}

pub fn apply_practice_property(
    chart: &mut PlayableChart,
    property: &PracticeProperty,
) -> AppliedArrange {
    let start_us = TimeUs(i64::from(property.start_time_ms) * 1000);
    let end_ms = property.end_time_ms.max(property.start_time_ms.saturating_add(1000));
    let end_us = TimeUs(i64::from(end_ms) * 1000);
    apply_practice_section(chart, start_us, end_us);
    chart.metadata.judge_rank = Some(property.judgerank);
    chart.metadata.judge_rank_spec =
        Some(JudgeRankSpec { value: property.judgerank, kind: JudgeRankKind::BmsonJudgeRank });
    if let Some(total) = property.total {
        chart.metadata.total = Some(total);
    }
    apply_arrange(chart, property.arrange, None, None)
}

pub fn apply_practice_start_gauge(gauge: &mut GaugeState, start_gauge: u32) {
    let value = start_gauge.clamp(1, 100) as f32;
    gauge.set_initial_value(value);
}

pub fn practice_chart_zero_time(property: &PracticeProperty, skin_playstart_us: TimeUs) -> TimeUs {
    let lead_ms = property.start_time_ms.saturating_sub(1000);
    TimeUs(skin_playstart_us.0 - i64::from(lead_ms) * 1000)
}

pub fn clamp_practice_property(property: &mut PracticeProperty, chart: &PlayableChart) {
    let max_end = default_end_time_ms(chart);
    property.start_time_ms = property.start_time_ms.min(max_end.saturating_sub(1000));
    property.end_time_ms =
        property.end_time_ms.clamp(property.start_time_ms.saturating_add(1000), max_end);
    property.judgerank = property.judgerank.clamp(1, 400);
    property.start_gauge = property.start_gauge.clamp(1, 100);
    if let Some(total) = property.total.as_mut() {
        *total = total.clamp(20.0, 5000.0);
    }
}

pub fn default_end_time_ms(chart: &PlayableChart) -> u32 {
    let end_ms = (chart.end_time.0 / 1000).max(0);
    u32::try_from(end_ms).unwrap_or(u32::MAX)
}

fn sha256_hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use bmz_core::chart::ChartIdentity;

    use super::*;
    use bmz_chart::model::ChartMetadata;

    fn empty_chart(end_ms: i64) -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [1; 32] },
            metadata: ChartMetadata {
                judge_rank: Some(150),
                total: Some(250.0),
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: Default::default(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(end_ms * 1000),
        }
    }

    #[test]
    fn load_practice_property_uses_chart_defaults() {
        let root = std::env::temp_dir().join(format!(
            "bmz-practice-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            collection_db: root.join("collection.db"),
            score_db: root.join("score.db"),
            network_db: root.join("network.db"),
            replay_dir: root.join("replay"),
        };
        let chart = empty_chart(120_000);
        let property = load_practice_property(
            &paths,
            &chart.identity.file_sha256,
            &chart,
            GaugeTypeConfig::Hard,
            &PracticeCliOverrides { start_time_ms: Some(5000), end_time_ms: None },
        )
        .unwrap();
        assert_eq!(property.start_time_ms, 5000);
        assert_eq!(property.end_time_ms, 120_000);
        assert_eq!(property.judgerank, 150);
        assert_eq!(property.gauge, GaugeTypeConfig::Hard);
        assert_eq!(property.total, Some(250.0));
        std::fs::remove_dir_all(root).ok();
    }
}
