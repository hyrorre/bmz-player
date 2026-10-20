use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::SelectRowKind;
use serde::Deserialize;

use super::*;

mod query;

use query::{Field, VirtualQuery, compare_for_order};

pub const VIRTUAL_FOLDER_PATH_PREFIX: &str = "bmz-filter:";
pub const VIRTUAL_FOLDER_CONFIG_FILE: &str = "select-folders.toml";

const BUILTIN_VIRTUAL_FOLDERS: &str = include_str!("../../../resources/select-folders.toml");

#[derive(Debug, Clone, Deserialize)]
struct VirtualFolderDocument {
    version: u32,
    #[serde(default)]
    folders: Vec<VirtualFolderDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct VirtualFolderDef {
    id: String,
    #[serde(default)]
    name: String,
    /// Mark this folder and its descendants as LR2's new-song folders.
    #[serde(default)]
    new_song: bool,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    query: Option<QueryDef>,
    #[serde(default)]
    items: Vec<VirtualFolderDef>,
    #[serde(default)]
    buckets: Option<BucketDef>,
    #[serde(default)]
    generate: Option<GenerateDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum QueryDef {
    Simple(String),
    Detailed {
        filter: String,
        #[serde(default)]
        order_by: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
}

impl QueryDef {
    fn filter(&self) -> &str {
        match self {
            Self::Simple(filter) | Self::Detailed { filter, .. } => filter,
        }
    }

    fn order_by(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed { order_by, .. } => order_by.as_deref(),
        }
    }

    fn limit(&self) -> Option<usize> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed { limit, .. } => *limit,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BucketDef {
    field: String,
    prefix: String,
    cuts: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GenerateDef {
    values: String,
    id: String,
    name: String,
    query: String,
    #[serde(default)]
    insert_at: Option<usize>,
}

#[derive(Debug, Clone)]
struct OrderBy {
    field: Field,
    descending: bool,
}

struct VirtualChartCandidate {
    chart: ChartListItem,
    analysis: Option<ChartAnalysisSummary>,
    score: Option<BestScoreSummary>,
    score_key: ScoreKey,
    first_seen_at: i64,
    update_times: crate::storage::score_db::ChartUpdateTimes,
}

pub(super) struct VirtualChartFacts<'a> {
    pub(super) chart: &'a ChartListItem,
    pub(super) analysis: Option<&'a ChartAnalysisSummary>,
    pub(super) score: Option<&'a BestScoreSummary>,
    pub(super) score_key: ScoreKey,
    pub(super) first_seen_at: i64,
    pub(super) update_times: &'a crate::storage::score_db::ChartUpdateTimes,
}

impl VirtualChartCandidate {
    fn facts(&self) -> VirtualChartFacts<'_> {
        VirtualChartFacts {
            chart: &self.chart,
            analysis: self.analysis.as_ref(),
            score: self.score.as_ref(),
            score_key: self.score_key,
            first_seen_at: self.first_seen_at,
            update_times: &self.update_times,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Loads the built-in catalog and merges an optional profile-local
/// `select-folders.toml`. A matching top-level id replaces the built-in entry;
/// `enabled = false` removes it.
fn load_catalog(profile_root: &Path) -> Result<Vec<VirtualFolderDef>> {
    let mut built_in: VirtualFolderDocument =
        toml::from_str(BUILTIN_VIRTUAL_FOLDERS).context("parse built-in select-folders.toml")?;
    validate_version(built_in.version)?;

    let custom_path = profile_root.join(VIRTUAL_FOLDER_CONFIG_FILE);
    if custom_path.is_file() {
        let source = fs::read_to_string(&custom_path)
            .with_context(|| format!("read {}", custom_path.display()))?;
        let custom: VirtualFolderDocument =
            toml::from_str(&source).with_context(|| format!("parse {}", custom_path.display()))?;
        validate_version(custom.version)?;
        for override_folder in custom.folders {
            if let Some(index) =
                built_in.folders.iter().position(|folder| folder.id == override_folder.id)
            {
                built_in.folders.remove(index);
                if override_folder.enabled {
                    built_in.folders.insert(index, override_folder);
                }
            } else if override_folder.enabled {
                built_in.folders.push(override_folder);
            }
        }
    }

    let mut folders = built_in
        .folders
        .into_iter()
        .filter(|folder| folder.enabled)
        .map(expand_folder)
        .collect::<Result<Vec<_>>>()?;
    folders.retain(|folder| folder.enabled);
    validate_siblings(&folders, VIRTUAL_FOLDER_PATH_PREFIX)?;
    Ok(folders)
}

fn validate_version(version: u32) -> Result<()> {
    if version == 1 {
        Ok(())
    } else {
        bail!("unsupported select-folders.toml version {version}; expected 1")
    }
}

fn expand_folder(mut folder: VirtualFolderDef) -> Result<VirtualFolderDef> {
    if folder.name.trim().is_empty() {
        bail!("folder `{}` must have a non-empty name", folder.id);
    }
    folder.items = folder
        .items
        .into_iter()
        .filter(|child| child.enabled)
        .map(expand_folder)
        .collect::<Result<Vec<_>>>()?;

    if let Some(bucket) = folder.buckets.take() {
        let field = Field::parse(&bucket.field)?;
        if matches!(
            field,
            Field::Mode | Field::AddedAt | Field::LampUpdatedAt | Field::ScoreUpdatedAt
        ) {
            bail!("field `{}` cannot be used for numeric buckets", bucket.field);
        }
        if bucket.cuts.len() < 2 || bucket.cuts.windows(2).any(|pair| pair[0] >= pair[1]) {
            bail!("folder `{}` bucket cuts must be strictly increasing", folder.id);
        }
        let first = bucket.cuts[0];
        folder.items.push(generated_leaf(
            format!("under-{}", number_id(first)),
            format!("{} 〜{}", bucket.prefix, display_number(first)),
            format!("{} < {}", bucket.field, display_number(first)),
        )?);
        for bounds in bucket.cuts.windows(2) {
            let lower = bounds[0];
            let upper = bounds[1];
            folder.items.push(generated_leaf(
                format!("{}-{}", number_id(lower), number_id(upper)),
                format!("{} {}〜{}", bucket.prefix, display_number(lower), display_number(upper)),
                format!(
                    "{} >= {} && {} < {}",
                    bucket.field,
                    display_number(lower),
                    bucket.field,
                    display_number(upper)
                ),
            )?);
        }
        let last = *bucket.cuts.last().expect("bucket has at least two cuts");
        folder.items.push(generated_leaf(
            format!("over-{}", number_id(last)),
            format!("{} {}〜", bucket.prefix, display_number(last)),
            format!("{} >= {}", bucket.field, display_number(last)),
        )?);
    }

    if let Some(generate) = folder.generate.take() {
        let mut generated = Vec::new();
        for value in parse_inclusive_range(&generate.values)? {
            let id = apply_template(&generate.id, value);
            let name = apply_template(&generate.name, value);
            let filter = apply_template(&generate.query, value);
            generated.push(generated_leaf(id, name, filter)?);
        }
        let insert_at = generate.insert_at.unwrap_or(folder.items.len()).min(folder.items.len());
        folder.items.splice(insert_at..insert_at, generated);
    }

    if folder.query.is_some() && !folder.items.is_empty() {
        bail!("folder `{}` cannot contain both query and child items", folder.id);
    }
    if folder.query.is_none() && folder.items.is_empty() {
        bail!("folder `{}` must contain query, items, buckets, or generate", folder.id);
    }
    if let Some(query) = &folder.query {
        VirtualQuery::parse(query.filter())
            .with_context(|| format!("parse query for folder `{}`", folder.id))?;
        if let Some(order_by) = query.order_by() {
            parse_order_by(order_by)
                .with_context(|| format!("parse order_by for folder `{}`", folder.id))?;
        }
    }
    if folder.new_song {
        mark_new_song_queries(&mut folder);
    }
    Ok(folder)
}

fn mark_new_song_queries(folder: &mut VirtualFolderDef) {
    folder.new_song = true;
    for child in &mut folder.items {
        mark_new_song_queries(child);
    }
}

fn generated_leaf(id: String, name: String, filter: String) -> Result<VirtualFolderDef> {
    VirtualQuery::parse(&filter).with_context(|| format!("parse generated query for `{id}`"))?;
    Ok(VirtualFolderDef {
        id,
        name,
        new_song: false,
        enabled: true,
        query: Some(QueryDef::Simple(filter)),
        items: Vec::new(),
        buckets: None,
        generate: None,
    })
}

fn parse_inclusive_range(source: &str) -> Result<std::ops::RangeInclusive<i64>> {
    let Some((start, end)) = source.split_once("..=") else {
        bail!("generated values `{source}` must use `start..=end`")
    };
    let start = start.trim().parse::<i64>().with_context(|| format!("invalid range `{source}`"))?;
    let end = end.trim().parse::<i64>().with_context(|| format!("invalid range `{source}`"))?;
    if start > end || end.saturating_sub(start) > 1_000 {
        bail!("generated range `{source}` is invalid or too large");
    }
    Ok(start..=end)
}

fn apply_template(template: &str, value: i64) -> String {
    let ordinal = value.saturating_add(1);
    let days_ago = if value == 0 { "TODAY".to_string() } else { format!("{ordinal} DAYS AGO") };
    template
        .replace("{value}", &value.to_string())
        .replace("{ordinal}", &ordinal.to_string())
        .replace("{days_ago}", &days_ago)
}

fn display_number(value: f64) -> String {
    if value.fract() == 0.0 { format!("{value:.0}") } else { value.to_string() }
}

fn number_id(value: f64) -> String {
    display_number(value).replace('.', "_").replace('-', "minus-")
}

fn validate_siblings(folders: &[VirtualFolderDef], parent: &str) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    for folder in folders {
        if folder.id.is_empty()
            || !folder.id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            bail!("invalid virtual-folder id `{}` below `{parent}`", folder.id);
        }
        if !ids.insert(folder.id.as_str()) {
            bail!("duplicate virtual-folder id `{}` below `{parent}`", folder.id);
        }
        validate_siblings(&folder.items, &format!("{parent}{}/", folder.id))?;
    }
    Ok(())
}

fn parse_order_by(source: &str) -> Result<OrderBy> {
    let parts = source.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 2 {
        bail!("order_by must be `field`, `field asc`, or `field desc`");
    }
    let field = Field::parse(parts[0])?;
    if !field.can_order() {
        bail!("field `{}` cannot be used for ordering", parts[0]);
    }
    let descending = match parts.get(1).copied().unwrap_or("asc") {
        "asc" => false,
        "desc" => true,
        value => bail!("unknown order direction `{value}`"),
    };
    Ok(OrderBy { field, descending })
}

fn folder_item(folder: &VirtualFolderDef, parent_path: &str) -> SelectItem {
    let path = if parent_path == VIRTUAL_FOLDER_PATH_PREFIX {
        format!("{parent_path}{}", folder.id)
    } else {
        format!("{parent_path}/{}", folder.id)
    };
    SelectItem::Folder {
        path,
        name: folder.name.clone(),
        kind: if folder.new_song {
            SelectRowKind::NewSongFolder
        } else if folder.query.is_some() {
            SelectRowKind::Command
        } else {
            SelectRowKind::Container
        },
        summary: None,
    }
}

pub fn virtual_folder_root_items(profile_root: &Path) -> Result<Vec<SelectItem>> {
    Ok(VirtualFolderCatalog::load(profile_root)?.root_items())
}

/// Validated definitions shared by list loading and frame snapshots.
#[derive(Debug, Clone)]
pub struct VirtualFolderCatalog {
    folders: Vec<VirtualFolderDef>,
}

impl VirtualFolderCatalog {
    pub fn load(profile_root: &Path) -> Result<Self> {
        Ok(Self { folders: load_catalog(profile_root)? })
    }

    pub fn root_items(&self) -> Vec<SelectItem> {
        self.folders.iter().map(|folder| folder_item(folder, VIRTUAL_FOLDER_PATH_PREFIX)).collect()
    }

    pub fn breadcrumb(&self, path: &str) -> Option<String> {
        let rest = path.strip_prefix(VIRTUAL_FOLDER_PATH_PREFIX)?;
        let mut current = self.folders.as_slice();
        let mut names = Vec::new();
        for segment in rest.split('/').filter(|segment| !segment.is_empty()) {
            let folder = current.iter().find(|folder| folder.id == segment)?;
            names.push(folder.name.as_str());
            current = &folder.items;
        }
        (!names.is_empty()).then(|| names.join(" > "))
    }
}

fn find_folder<'a>(folders: &'a [VirtualFolderDef], path: &str) -> Option<&'a VirtualFolderDef> {
    let rest = path.strip_prefix(VIRTUAL_FOLDER_PATH_PREFIX)?;
    let mut segments = rest.split('/').filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let mut folder = folders.iter().find(|folder| folder.id == first)?;
    for segment in segments {
        folder = folder.items.iter().find(|child| child.id == segment)?;
    }
    Some(folder)
}

pub fn virtual_folder_breadcrumb(profile_root: &Path, path: &str) -> Result<Option<String>> {
    Ok(VirtualFolderCatalog::load(profile_root)?.breadcrumb(path))
}

pub fn load_select_items_in_virtual_folder(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    profile_root: &Path,
    path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    active_song_roots: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    VirtualFolderCatalog::load(profile_root)?.load_items(
        library_db,
        score_db,
        path,
        ln_policy_setting,
        rule_mode,
        active_song_roots,
    )
}

impl VirtualFolderCatalog {
    pub fn load_items(
        &self,
        library_db: &LibraryDatabase,
        score_db: &ScoreDatabase,
        path: &str,
        ln_policy_setting: LnPolicySetting,
        rule_mode: RuleMode,
        active_song_roots: Option<&[String]>,
    ) -> Result<Vec<SelectItem>> {
        let folder = find_folder(&self.folders, path)
            .with_context(|| format!("unknown virtual folder `{path}`"))?;
        if folder.query.is_none() {
            return Ok(folder.items.iter().map(|child| folder_item(child, path)).collect());
        }
        let query_def = folder.query.as_ref().expect("query checked above");
        let query = VirtualQuery::parse(query_def.filter())?;
        let order_by = query_def.order_by().map(parse_order_by).transpose()?;

        let mut charts = library_db.list_all_charts()?;
        retain_active_charts(&mut charts, active_song_roots);
        let score_keys = charts
            .iter()
            .map(|chart| score_key_for_chart(chart, ln_policy_setting, rule_mode))
            .collect::<Vec<_>>();
        let score_map = score_db
            .best_scores_for_charts(&score_keys)?
            .into_iter()
            .map(|score| {
                (
                    ScoreKey::with_options(
                        score.chart_sha256,
                        score.ln_policy,
                        score.double_option,
                        score.rule_mode,
                    ),
                    score,
                )
            })
            .collect::<HashMap<_, _>>();
        let chart_ids = charts.iter().map(|chart| chart.chart_id).collect::<Vec<_>>();
        let mut analysis_map = library_db.chart_analysis_summaries_by_chart_ids(&chart_ids)?;
        let first_seen_map = library_db.chart_first_seen_at_by_chart_ids(&chart_ids)?;
        let local_days = score_db.recent_local_day_ranges(query.required_local_days().max(1))?;
        let update_start = local_days.last().map_or(0, |range| range.0);
        let update_map = score_db.chart_update_times_since(&score_keys, update_start)?;

        let mut candidates = charts
            .into_iter()
            .zip(score_keys)
            .map(|(chart, score_key)| VirtualChartCandidate {
                analysis: analysis_map.remove(&chart.chart_id),
                score: score_map.get(&score_key).cloned(),
                first_seen_at: first_seen_map.get(&chart.chart_id).copied().unwrap_or(0),
                update_times: update_map.get(&score_key).cloned().unwrap_or_default(),
                chart,
                score_key,
            })
            .filter(|candidate| query.matches(&candidate.facts(), &local_days))
            .collect::<Vec<_>>();

        if let Some(order_by) = order_by {
            candidates.sort_by(|left, right| {
                let ordering = compare_for_order(order_by.field, &left.facts(), &right.facts());
                let ordering = if order_by.descending { ordering.reverse() } else { ordering };
                ordering
                    .then_with(|| left.chart.title.cmp(&right.chart.title))
                    .then_with(|| left.chart.chart_id.cmp(&right.chart.chart_id))
            });
        }
        let mut seen_sha256 = HashSet::new();
        candidates.retain(|candidate| seen_sha256.insert(candidate.chart.sha256));
        if let Some(limit) = query_def.limit() {
            candidates.truncate(limit);
        }

        chart_items_without_table_enrichment(
            library_db,
            score_db,
            candidates.into_iter().map(|candidate| candidate.chart).collect(),
            ln_policy_setting,
            rule_mode,
        )
    }
}

pub fn virtual_folder_summary(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    profile_root: &Path,
    path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    active_song_roots: &[String],
) -> Result<SelectFolderSummary> {
    let items = load_select_items_in_virtual_folder(
        library_db,
        score_db,
        profile_root,
        path,
        ln_policy_setting,
        rule_mode,
        Some(active_song_roots),
    )?;
    let rows = items
        .into_iter()
        .filter_map(|item| match item {
            SelectItem::Chart(row) => Some(row),
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(SelectFolderSummary::from(rows.as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_catalog_excludes_invisible_and_expands_generators() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("missing-profile");
        let folders = load_catalog(&profile).unwrap();
        assert!(!folders.iter().any(|folder| folder.name.contains("INVISIBLE")));
        let lamp = folders.iter().find(|folder| folder.id == "lamp-update").unwrap();
        assert_eq!(lamp.items.len(), 30);
        assert_eq!(lamp.items[0].name, "TODAY");
        assert_eq!(lamp.items[1].name, "2 DAYS AGO");
        let density = folders.iter().find(|folder| folder.id == "density").unwrap();
        assert_eq!(density.items[0].items.len(), 46);
        let new = folders.iter().find(|folder| folder.id == "new").unwrap();
        assert!(matches!(
            folder_item(new, VIRTUAL_FOLDER_PATH_PREFIX),
            SelectItem::Folder { kind: SelectRowKind::NewSongFolder, .. }
        ));
        assert!(new.items.iter().all(|child| matches!(
            folder_item(child, "bmz-filter:new"),
            SelectItem::Folder { kind: SelectRowKind::NewSongFolder, .. }
        )));
        let best = folders.iter().find(|folder| folder.id == "my-best").unwrap();
        assert!(matches!(
            folder_item(best, VIRTUAL_FOLDER_PATH_PREFIX),
            SelectItem::Folder { kind: SelectRowKind::Command, .. }
        ));
    }

    #[test]
    fn profile_catalog_can_disable_and_replace_top_level_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join(VIRTUAL_FOLDER_CONFIG_FILE),
            r#"
version = 1

[[folders]]
id = "density"
enabled = false

[[folders]]
id = "custom"
name = "CUSTOM"
query = "mode == '7K' && level >= 10"
"#,
        )
        .unwrap();
        let folders = load_catalog(root).unwrap();
        assert!(!folders.iter().any(|folder| folder.id == "density"));
        assert!(folders.iter().any(|folder| folder.id == "custom"));
    }
}
