use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;
use bmz_core::course::{CourseDefinition, CourseKind, CourseLnConstraint};
use bmz_core::lane::KeyMode;
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::SelectRowKind;

use crate::i18n::{AppLocale, FluentArgs, Localizer};
use crate::ln_policy::{LnPolicySetting, LnScorePolicy, course_score_ln_policy, score_ln_policy};
use crate::screens::settings_model::{AppConfigSelectRow, ConfigSelectRow, KeyBindingSelectRow};
use crate::song_download::ChartDownloadMetadata;
use crate::storage::collection_db::{CollectionDatabase, FavoriteChartRecord, FavoriteSongRecord};
use crate::storage::common::hash_to_hex;
use crate::storage::library_db::{
    ChartAnalysisSummary, ChartListItem, DifficultyTableEntryRecord, LibraryDatabase,
    TableEntryListItem,
};
use crate::storage::score_db::ScoreKey;
use crate::storage::score_db::{BestScoreSummary, ReplaySlotSummary, ScoreDatabase};
mod enrichment;
mod favorites;
mod folder;
mod paths;
mod root;
mod search;
mod table;
mod virtual_folder;

use enrichment::*;
use paths::*;
use table::*;

pub use enrichment::{select_folder_summary, select_folder_summary_for_rule_mode};
pub use favorites::{
    apply_collection_flags, favorite_song_representatives_for_folder,
    load_select_items_for_favorite_charts, load_select_items_for_favorite_song,
    load_select_items_for_favorite_songs,
};
pub use folder::{
    load_select_items_in_folder, load_select_items_in_folder_for_rule_mode,
    load_select_items_in_folder_for_rule_mode_with_filters,
    load_select_items_in_folder_for_rule_mode_with_table_order,
};
pub(crate) use paths::chart_is_in_active_song_roots;
pub use paths::{
    COURSE_CONTENTS_PATH_PREFIX, COURSE_ROOT_PATH, FAVORITE_CHART_PATH, FAVORITE_ROOT_PATH,
    FAVORITE_SONG_DETAIL_PREFIX, FAVORITE_SONG_PATH, MAX_SEARCH_HISTORY, SAME_FOLDER_PATH_PREFIX,
    SEARCH_PATH_PREFIX, TABLE_LEVEL_SEPARATOR, TABLE_ROOT_PATH, TablePath, course_contents_path,
    favorite_song_detail_path, parse_course_contents_path, parse_favorite_song_detail_path,
    parse_same_folder_path, parse_search_query, parse_table_path, same_folder_path,
    search_history_folder_items, search_history_folder_items_for_locale,
    song_scan_path_from_context, table_source_url_from_context,
};
pub use root::{
    favorite_root_item, favorite_root_items, random_mix_item, random_select_item_from_items,
    root_folder_items,
};
pub use search::{
    load_select_items_for_search, load_select_items_for_search_for_rule_mode,
    load_select_items_for_search_for_rule_mode_with_filters,
    load_select_items_for_search_for_rule_mode_with_table_order,
};
pub use table::{
    RANDOM_MIX_COURSE_SOURCE, course_root_item, load_select_items_for_course_contents,
    load_select_items_for_courses, load_select_items_in_table,
    load_select_items_in_table_for_rule_mode, load_select_items_in_table_level,
    load_select_items_in_table_level_for_rule_mode, new_course_item_for_locale, table_folder_items,
    table_folder_items_for_active_sources, table_level_folder_items,
};
pub use virtual_folder::{
    VIRTUAL_FOLDER_CONFIG_FILE, VIRTUAL_FOLDER_PATH_PREFIX, load_select_items_in_virtual_folder,
    virtual_folder_breadcrumb, virtual_folder_root_items,
};

pub fn normalized_course_ln_policy_for_charts(
    setting: LnPolicySetting,
    constraint: CourseLnConstraint,
    charts: &[ChartListItem],
) -> LnScorePolicy {
    let fallback = match constraint {
        CourseLnConstraint::Default => None,
        CourseLnConstraint::Ln => Some(bmz_chart::model::LongNoteMode::Ln),
        CourseLnConstraint::Cn => Some(bmz_chart::model::LongNoteMode::Cn),
        CourseLnConstraint::Hcn => Some(bmz_chart::model::LongNoteMode::Hcn),
    };
    crate::ln_policy::course_score_ln_policy_for_profiles(
        setting,
        fallback,
        charts.iter().map(|chart| chart.ln_profile),
    )
}

pub fn normalized_course_ln_policy_for_definition(
    library_db: &LibraryDatabase,
    definition: &CourseDefinition,
    setting: LnPolicySetting,
) -> Result<LnScorePolicy> {
    let mut chart_ids = Vec::with_capacity(definition.entries.len());
    for entry in &definition.entries {
        let Some(chart_id) = entry.chart_id else {
            return Err(anyhow::anyhow!("course has an unresolved chart entry"));
        };
        chart_ids.push(chart_id);
    }
    let expected_ids: HashSet<i64> = chart_ids.iter().copied().collect();
    let charts = library_db.list_charts_by_ids(&chart_ids)?;
    if charts.len() != expected_ids.len() {
        return Err(anyhow::anyhow!(
            "course chart profile count mismatch: expected {}, found {}",
            expected_ids.len(),
            charts.len(),
        ));
    }
    Ok(normalized_course_ln_policy_for_charts(setting, definition.constraints.ln, &charts))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DifficultyTableText {
    pub table_name: String,
    pub table_level: String,
    pub table_full: String,
}

impl DifficultyTableText {
    pub fn from_parts(table_name: String, table_symbol: &str, level: &str) -> Self {
        let table_level = table_level_label(table_symbol, level);
        let table_full = format!("{table_level}{table_name}");
        Self { table_name, table_level, table_full }
    }

    pub fn from_entry(entry: &DifficultyTableEntryRecord) -> Self {
        Self::from_parts(entry.table_name.clone(), &entry.table_symbol, &entry.level)
    }

    pub fn is_table_song(&self) -> bool {
        !self.table_name.is_empty()
    }

    pub fn as_tuple(&self) -> (String, String, String) {
        (self.table_name.clone(), self.table_level.clone(), self.table_full.clone())
    }
}

/// Resolves beatoraja TEXT_TABLE1/2/3 information for a chart.
///
/// TEXT_TABLE1 is the table name, TEXT_TABLE2 is symbol+level, and TEXT_TABLE3
/// is TEXT_TABLE2 + TEXT_TABLE1, matching PlayerResource#getTableFullname().
/// MD5 has priority; SHA-256 is used only when no MD5 table row is found.
pub fn difficulty_table_text_for_chart(
    library_db: &LibraryDatabase,
    chart: &ChartListItem,
    source_order: &[String],
    source_hint: Option<&str>,
) -> Result<DifficultyTableText> {
    difficulty_table_text_for_chart_with_active_sources(
        library_db,
        chart,
        source_order,
        source_hint,
        None,
    )
}

pub fn difficulty_table_text_for_chart_with_active_sources(
    library_db: &LibraryDatabase,
    chart: &ChartListItem,
    source_order: &[String],
    source_hint: Option<&str>,
    active_source_urls: Option<&[String]>,
) -> Result<DifficultyTableText> {
    let md5_hex = hash_to_hex(&chart.md5);
    let mut md5_entries = library_db.list_difficulty_table_entries_by_md5s(&[md5_hex.as_str()])?;
    retain_active_table_entries(&mut md5_entries, active_source_urls);
    if !md5_entries.is_empty() {
        return Ok(choose_difficulty_table_text(md5_entries, source_order, source_hint));
    }

    let sha256_hex = hash_to_hex(&chart.sha256);
    let mut sha256_entries =
        library_db.list_difficulty_table_entries_by_sha256s(&[sha256_hex.as_str()])?;
    retain_active_table_entries(&mut sha256_entries, active_source_urls);
    Ok(choose_difficulty_table_text(sha256_entries, source_order, source_hint))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectChartRow {
    pub chart: Option<ChartListItem>,
    pub chart_analysis: Option<ChartAnalysisSummary>,
    /// beatoraja `SongData.hasDocument()` compatible same-folder `.txt` presence.
    pub has_document: bool,
    pub fallback_title: String,
    pub fallback_artist: String,
    pub entry_sha256: Option<[u8; 32]>,
    pub download_metadata: ChartDownloadMetadata,
    pub best_score: Option<BestScoreSummary>,
    pub replay_slots: [bool; 4],
    pub favorite_chart: bool,
    pub favorite_song: bool,
    pub table_level: String,
    pub table_text: DifficultyTableText,
}

impl SelectChartRow {
    pub fn display_title(&self) -> &str {
        self.chart
            .as_ref()
            .map(|chart| chart.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or(self.fallback_title.as_str())
    }

    pub fn display_artist(&self) -> &str {
        self.chart
            .as_ref()
            .map(|chart| chart.artist.as_str())
            .filter(|artist| !artist.is_empty())
            .unwrap_or(self.fallback_artist.as_str())
    }

    pub fn in_library(&self) -> bool {
        self.chart.is_some()
    }

    pub fn score_sha256(&self) -> Option<[u8; 32]> {
        self.chart.as_ref().map(|chart| chart.sha256).or(self.entry_sha256)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectCourseRow {
    pub course_id: i64,
    /// Canonical IR course hash. None while any course entry is unresolved.
    pub course_hash: Option<String>,
    /// rianIR/beatoraja connector互換のremote course hash。
    pub rian_course_hash_v1: Option<String>,
    /// BMS-IR/LR2互換の長いcourse key。
    pub bms_ir_course_key: Option<String>,
    /// Course-wide score policy normalized from every resolved chart profile.
    pub ln_policy: LnScorePolicy,
    pub title: String,
    pub kind: CourseKind,
    pub constraints: bmz_core::course::CourseConstraints,
    /// Common key mode when every resolved entry uses the same mode. Mixed or
    /// unresolved courses intentionally expose no editable play-mode config.
    pub common_key_mode: Option<KeyMode>,
    /// Total number of entries in the course.
    pub entry_count: usize,
    /// Number of entries whose `chart_id` is resolved in the local library.
    pub resolved_count: usize,
    /// Total notes across all resolved entries.
    pub total_notes: u32,
    /// Sum of length in milliseconds across resolved entries.
    pub total_length_ms: i64,
    /// Minimum / maximum BPM among resolved entries.
    pub min_bpm: f32,
    pub max_bpm: f32,
    /// Difficulty band derived from constraints (e.g. "DAN" / "COURSE").
    pub category_label: String,
    /// Trophy names defined for this course (e.g. ["silvermedal", "goldmedal"]).
    pub trophy_names: Vec<String>,
    /// Entries inside the course, used by the preview panel.
    pub entry_previews: Vec<CourseEntryPreview>,
    /// Best persisted course score, if any.  Populated from the
    /// `course_scores` table; `None` when the course has never been played
    /// successfully or when the lookup failed.
    pub best_score: Option<crate::storage::score_db::CourseBestScore>,
    /// Which of the four course replay slots have a saved attempt.  Used by
    /// the select skin to render slot indicators on course rows.
    pub replay_slots: [bool; 4],
    /// Names of trophies that have been earned at least once across all
    /// stored attempts of this course (`course_trophy_achievements`).  A
    /// strict subset of `trophy_names`.
    pub achieved_trophy_names: Vec<String>,
}

impl SelectCourseRow {
    /// beatoraja `GradeBar.existsAllSongs()`: a course is playable only when
    /// every declared entry resolves to a local song.
    pub fn exists_all_songs(&self) -> bool {
        self.entry_count > 0 && self.resolved_count == self.entry_count
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseEntryPreview {
    /// Title taken from the resolved library chart when available, otherwise
    /// the title_hint declared in the course JSON.
    pub title: String,
    pub artist: String,
    pub play_level: String,
    pub difficulty_name: String,
    pub total_notes: u32,
    /// True when this entry is resolved to a chart in the local library.
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectFolderSummary {
    pub lamp_counts: [u32; 11],
}

impl SelectFolderSummary {
    pub fn clear_type(&self) -> String {
        let index = self.lamp_counts.iter().position(|count| *count > 0).unwrap_or(0);
        clear_type_name_for_folder_lamp(index).to_string()
    }
}

impl From<&[SelectChartRow]> for SelectFolderSummary {
    fn from(rows: &[SelectChartRow]) -> Self {
        let mut lamp_counts = [0; 11];
        for row in rows {
            let index = row
                .best_score
                .as_ref()
                .map(|score| folder_lamp_index_from_clear_type(&score.clear_type))
                .unwrap_or(0);
            lamp_counts[index] += 1;
        }
        Self { lamp_counts }
    }
}

fn folder_lamp_index_from_clear_type(clear_type: &str) -> usize {
    usize::from(bmz_core::clear::ClearType::rank_from_label(clear_type))
}

fn clear_type_name_for_folder_lamp(index: usize) -> &'static str {
    match index {
        1 => "Failed",
        2 => "AssistEasy",
        3 => "LightAssistEasy",
        4 => "Easy",
        5 => "Normal",
        6 => "Hard",
        7 => "ExHard",
        8 => "FullCombo",
        9 => "Perfect",
        10 => "Max",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectExecutableKind {
    RandomSelect,
    RandomMix,
    NewCourse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectExecutableRow {
    pub title: String,
    pub kind: SelectExecutableKind,
    pub chart_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
// SelectItem は長寿命の選曲リストに格納され、全 variant を頻繁に走査する。
// variant ごとの Box 化は行単位の割当とポインタ追跡を増やすため、連続配置を優先する。
#[expect(clippy::large_enum_variant, reason = "選曲リストの連続配置と走査局所性を優先する")]
pub enum SelectItem {
    Folder {
        path: String,
        name: String,
        kind: SelectRowKind,
        summary: Option<SelectFolderSummary>,
    },
    Chart(SelectChartRow),
    Course(SelectCourseRow),
    Executable(SelectExecutableRow),
    Config(ConfigSelectRow),
    AppConfig(AppConfigSelectRow),
    KeyBinding(KeyBindingSelectRow),
    /// 設定カテゴリから 1 階層戻るアクション行。
    SettingsBack,
    /// 設定ルートを閉じるアクション行。
    SettingsClose,
    /// ゲーム内設定から egui の詳細設定ウィンドウを開くアクション行。
    AdvancedSettings,
    /// 保存済みの音声設定で出力を開き直すアクション行。
    ApplyAudioSettings,
}

impl SelectItem {
    pub fn display_name(&self) -> String {
        self.display_name_for_locale(AppLocale::DEFAULT)
    }

    pub fn display_name_for_locale(&self, locale: AppLocale) -> String {
        match self {
            Self::Folder { name, .. } => name.clone(),
            Self::Chart(row) => row.display_title().to_string(),
            Self::Course(row) => row.title.clone(),
            Self::Executable(row) => row.title.clone(),
            Self::Config(row) => row.label().to_string(),
            Self::AppConfig(row) => row.label().to_string(),
            Self::KeyBinding(row) => row.label(),
            Self::SettingsBack => Localizer::new(locale).text("select-back"),
            Self::SettingsClose => Localizer::new(locale).text("select-close"),
            Self::AdvancedSettings => Localizer::new(locale).text("select-advanced-settings"),
            Self::ApplyAudioSettings => Localizer::new(locale).text("settings-audio-apply"),
        }
    }
}

#[cfg(test)]
#[path = "select_model/tests.rs"]
mod tests;
