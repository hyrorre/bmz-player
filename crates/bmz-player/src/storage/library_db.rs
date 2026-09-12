use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use bmz_chart::import::error::ImportWarning;
use bmz_chart::model::{LongNoteMode, NoteKind, PlayableChart, TimingEventKind};
use bmz_core::lane::Lane;
use bmz_gameplay::gauge::gauge_total_for_chart;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::ln_policy::{ChartLnCounts, ChartLnProfile, LnPolicySetting, LnScorePolicy};
use crate::paths::normalize_library_path;

pub use super::course_db::{StoredCourse, StoredCourseEntry};
pub use super::difficulty_table_db::{
    DifficultyTableEntryRecord, DifficultyTableRecord, TableEntryRow,
};

use super::common::{configure_connection, hash_to_hex, hex_to_hash};

mod analysis;
mod analysis_helpers;
mod database_catalog;
mod database_query;
mod database_write;
mod path_helpers;
mod query_helpers;

use analysis_helpers::*;
use path_helpers::*;
use query_helpers::*;

pub(crate) fn library_path_key(path: &Path) -> String {
    path_helpers::path_key(path)
}

pub const CHART_IMPORT_VERSION: i64 = 8;
// v4 excludes muted battle presentation lanes from chart analysis.
pub const CHART_LOUDNESS_ANALYSIS_VERSION: i64 = 4;
const MAX_ANALYSIS_DISTRIBUTION_SECONDS: usize = 10 * 60;

pub struct LibraryDatabase {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct ChartImportRecord<'a> {
    pub root_id: Option<i64>,
    pub file_path: &'a Path,
    pub file_size: u64,
    pub modified_at: i64,
    pub scanned_at: i64,
    pub chart: &'a PlayableChart,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartListItem {
    pub chart_id: i64,
    pub md5: [u8; 16],
    pub sha256: [u8; 32],
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub mode: String,
    pub total_notes: u32,
    pub initial_bpm: f64,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub length_ms: i64,
    pub folder_path: String,
    pub stage_file: String,
    pub banner_file: String,
    pub backbmp_file: String,
    pub preview_file: String,
    pub has_document: bool,
    pub has_bga: bool,
    pub has_long_notes: bool,
    pub has_mines: bool,
    pub has_bms_random: bool,
    pub judge_rank: Option<i32>,
    /// Effective BMS-scale TOTAL (`model.getTotal()` after BMSON normalization).
    /// Unset BMS charts store `0.0`.
    pub bms_total: f64,
    pub ln_profile: ChartLnProfile,
    pub ln_counts: ChartLnCounts,
}

impl ChartListItem {
    pub const fn scored_total_notes(&self, policy: LnScorePolicy) -> u32 {
        self.ln_counts.scored_total_notes(self.total_notes, policy)
    }

    pub fn scored_total_notes_for_setting(&self, setting: LnPolicySetting) -> u32 {
        self.ln_counts.scored_total_notes_for_setting(self.total_notes, setting)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartAnalysis {
    pub normal_notes: u32,
    pub long_notes: u32,
    pub scratch_notes: u32,
    pub long_scratch_notes: u32,
    pub density: f64,
    pub peak_density: f64,
    pub end_density: f64,
    pub total_gauge: f64,
    pub main_bpm: f64,
    pub distribution: Vec<ChartDistributionSecond>,
    pub speed_changes: Vec<ChartSpeedChange>,
    pub lane_notes: Vec<ChartLaneNotes>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartAnalysisSummary {
    pub normal_notes: u32,
    pub long_notes: u32,
    pub scratch_notes: u32,
    pub long_scratch_notes: u32,
    pub density: f64,
    pub peak_density: f64,
    pub end_density: f64,
    pub total_gauge: f64,
    pub main_bpm: f64,
    pub speed_changes: Vec<ChartSpeedChange>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartNormalizationAnalysis {
    pub loudness_lufs: f32,
    pub short_term_lufs: f32,
    pub sample_peak: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartDistributionSecond {
    pub scratch_long_heads: u16,
    pub scratch_long_bodies: u16,
    pub scratch_taps: u16,
    pub key_long_heads: u16,
    pub key_long_bodies: u16,
    pub key_taps: u16,
    pub mines: u16,
}

impl ChartDistributionSecond {
    fn playable_notes(self) -> u32 {
        u32::from(self.scratch_long_heads)
            + u32::from(self.scratch_long_bodies)
            + u32::from(self.scratch_taps)
            + u32::from(self.key_long_heads)
            + u32::from(self.key_long_bodies)
            + u32::from(self.key_taps)
    }

    fn is_empty(self) -> bool {
        self.scratch_long_heads == 0
            && self.scratch_long_bodies == 0
            && self.scratch_taps == 0
            && self.key_long_heads == 0
            && self.key_long_bodies == 0
            && self.key_taps == 0
            && self.mines == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChartSpeedChange {
    pub speed: f64,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartLaneNotes {
    pub lane_index: u8,
    pub normal_notes: u32,
    pub long_notes: u32,
    pub mines: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableEntryListItem {
    pub level: String,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub url: String,
    pub append_url: String,
    pub ipfs: String,
    pub append_ipfs: String,
    pub chart: Option<ChartListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedChartFile {
    pub chart_file_id: i64,
    pub path: String,
    pub message: String,
    pub scanned_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartFileFingerprint {
    pub file_size: u64,
    pub modified_at: i64,
    pub import_version: i64,
}

#[cfg(test)]
#[path = "library_db/tests.rs"]
mod tests;
