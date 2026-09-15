//! rianIR がアカウント向けに生成する難易度表を BMZ の既存テーブル形式へ変換する。
//!
//! rianIR 側の API や DB schema は変更せず、`get_tables.php` の応答を
//! `difficulty_tables` にキャッシュする。source URL は provider/base/account ごとの
//! digest を含むため、別アカウントの POPULAR / rival 表が混ざらない。

use std::fmt::Write as _;
use std::time::Duration;

use anyhow::Result;
use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry};
use sha2::{Digest, Sha256};

use crate::config::profile_config::IrConfig;
use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
use crate::storage::library_db::LibraryDatabase;

use super::provider_key::{configured_provider_key, primary_provider_config};
use super::rian_ir::{
    RianIrClient, RianTableChart, RianTableCourse, RianTableResource, is_rian_ir_config,
};

pub const RIAN_TABLE_SOURCE_PREFIX: &str = "bmz-rian-table:";
pub const BMS_IR_TABLE_SOURCE_PREFIX: &str = "bmz-bms-ir-table:";
pub const RIAN_TABLE_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
pub const RIAN_TABLE_MANUAL_REFRESH_COOLDOWN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RianTableIdentity {
    pub provider_key: String,
    pub base_url: String,
    pub account_id: String,
    source_prefix: String,
}

impl RianTableIdentity {
    pub fn from_ir_config(config: &IrConfig) -> Option<Self> {
        let provider = primary_provider_config(config)?;
        if !is_rian_ir_config(provider) && !crate::ir::bms_ir::is_bms_ir_config(provider) {
            return None;
        }
        let provider_key = configured_provider_key(provider)?.trim();
        let base_url = provider.base_url.trim();
        let account_id = provider.account_id.trim();
        if base_url.is_empty() || account_id.is_empty() {
            return None;
        }
        let normalized_base = base_url.trim_end_matches('/');
        let scope = stable_digest(&format!("{provider_key}\0{normalized_base}\0{account_id}"));
        Some(Self {
            provider_key: provider_key.to_string(),
            base_url: base_url.to_string(),
            account_id: account_id.to_string(),
            source_prefix: format!(
                "{}{scope}:",
                if crate::ir::bms_ir::is_bms_ir_config(provider) {
                    BMS_IR_TABLE_SOURCE_PREFIX
                } else {
                    RIAN_TABLE_SOURCE_PREFIX
                }
            ),
        })
    }

    pub fn source_prefix(&self) -> &str {
        &self.source_prefix
    }

    pub fn display_name(&self) -> &'static str {
        if crate::ir::bms_ir::is_bms_ir_provider(&self.provider_key) { "BMS-IR" } else { "rianIR" }
    }

    pub fn owns_source(&self, source_url: &str) -> bool {
        source_url.starts_with(&self.source_prefix)
    }
}

pub fn is_rian_table_source(source_url: &str) -> bool {
    source_url.starts_with(RIAN_TABLE_SOURCE_PREFIX)
        || source_url.starts_with(BMS_IR_TABLE_SOURCE_PREFIX)
}

pub fn active_source_urls(
    library_db: &LibraryDatabase,
    identity: &RianTableIdentity,
) -> Result<Vec<String>> {
    Ok(library_db
        .list_difficulty_tables()?
        .into_iter()
        .filter(|table| identity.owns_source(&table.source_url))
        .map(|table| table.source_url)
        .collect())
}

pub async fn fetch_account_tables(
    identity: &RianTableIdentity,
    profile_root: &std::path::Path,
    fetched_at: i64,
) -> Result<Vec<FetchedDifficultyTable>> {
    let resources = if crate::ir::bms_ir::is_bms_ir_provider(&identity.provider_key) {
        let credentials = crate::ir::sync::ensure_fresh_credentials(
            profile_root,
            &identity.provider_key,
            &identity.base_url,
            fetched_at,
        )
        .await?;
        crate::ir::bms_ir::BmsIrClient::new(&identity.base_url)?
            .fetch_tables(&credentials.account_id, &credentials.access_token)
            .await?
    } else {
        RianIrClient::new(&identity.base_url)?.fetch_tables(&identity.account_id).await?
    };
    Ok(convert_resources(identity, resources, fetched_at))
}

pub fn store_account_tables(
    library_db: &mut LibraryDatabase,
    identity: &RianTableIdentity,
    tables: &[FetchedDifficultyTable],
) -> Result<(usize, usize)> {
    library_db.replace_account_difficulty_tables(identity.source_prefix(), tables)
}

fn convert_resources(
    identity: &RianTableIdentity,
    resources: Vec<RianTableResource>,
    fetched_at: i64,
) -> Vec<FetchedDifficultyTable> {
    resources
        .into_iter()
        .map(|resource| {
            let table = resource.attributes;
            let source_key = stable_digest(&table.name);
            let mut level_order = Vec::with_capacity(table.folders.len());
            let mut entries = Vec::new();
            for folder in table.folders {
                if !level_order.contains(&folder.name) {
                    level_order.push(folder.name.clone());
                }
                entries.extend(
                    folder
                        .charts
                        .into_iter()
                        .filter_map(|chart| convert_chart(chart, &folder.name)),
                );
            }
            FetchedDifficultyTable {
                source_url: format!("{}{source_key}", identity.source_prefix),
                head_url: identity.base_url.clone(),
                name: table.name,
                // rianIR の folder.name は表示用の完成形で、通常表では既にsymbolを
                // 含む。BMZの通常表表示は `symbol + level` なので、ここでは空にして
                // POPULAR / reviewを含むfolder名の二重prefixを防ぐ。
                symbol: String::new(),
                level_order,
                entries,
                courses: table.courses.into_iter().map(convert_course).collect(),
                fetched_at,
            }
        })
        .collect()
}

fn convert_chart(chart: RianTableChart, fallback_level: &str) -> Option<FetchedTableEntry> {
    let md5 = chart.md5.trim().to_ascii_lowercase();
    let sha256 = chart.sha256.trim().to_ascii_lowercase();
    if md5.len() < 24 && sha256.len() < 24 {
        return None;
    }
    Some(FetchedTableEntry {
        // rianIR の POPULAR は chart.level が "Top 20" でも、所属 folder は
        // "24H POPULAR SONGS" になる。BMZ の level folder と一致させるため
        // API の folder 境界を正とする。
        level: fallback_level.to_string(),
        md5,
        sha256,
        title: joined_title(&chart.title, &chart.subtitle),
        artist: joined_title(&chart.artist, &chart.subartist),
        ..Default::default()
    })
}

fn convert_course(course: RianTableCourse) -> CourseDefinition {
    let constraints =
        CourseConstraints::from_beatoraja_names(course.constraint.iter().map(String::as_str));
    let entries: Vec<CourseEntry> = course
        .charts
        .into_iter()
        .map(|chart| CourseEntry {
            title_hint: joined_title(&chart.title, &chart.subtitle),
            md5: nonempty_lowercase(chart.md5),
            sha256: nonempty_lowercase(chart.sha256),
            chart_id: None,
        })
        .collect();
    let key = if course.sha256.trim().is_empty() {
        stable_digest(&format!("{}\0{:?}", course.name, entries))
    } else {
        course.sha256.trim().to_ascii_lowercase()
    };
    CourseDefinition {
        key,
        title: course.name,
        kind: CourseDefinition::derive_kind_from_constraints(&constraints),
        entries,
        constraints,
        trophies: Vec::new(),
        release: true,
    }
}

fn joined_title(primary: &str, secondary: &str) -> String {
    match (primary.trim(), secondary.trim()) {
        ("", secondary) => secondary.to_string(),
        (primary, "") => primary.to_string(),
        (primary, secondary) => format!("{primary} {secondary}"),
    }
}

fn nonempty_lowercase(value: String) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn stable_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{IrProviderConfig, IrProviderRoleConfig};
    use crate::ir::rian_ir::{RianTable, RianTableFolder};

    fn identity() -> RianTableIdentity {
        RianTableIdentity {
            provider_key: "rian-ir".to_string(),
            base_url: "https://rianir.example/api/".to_string(),
            account_id: "player".to_string(),
            source_prefix: "bmz-rian-table:test:".to_string(),
        }
    }

    #[test]
    fn identity_requires_primary_enabled_rian_account() {
        let mut config = IrConfig {
            primary_provider: "rian-ir".to_string(),
            providers: vec![IrProviderConfig {
                provider: "rian-ir".to_string(),
                provider_key: "rian-ir".to_string(),
                base_url: "https://rianir.example/api/".to_string(),
                enabled: true,
                account_display_name: "Player".to_string(),
                account_id: "player".to_string(),
                send_policy: Default::default(),
                role: IrProviderRoleConfig::Primary,
                last_login_at: Some(1),
                last_success_at: None,
            }],
            ..Default::default()
        };
        let first = RianTableIdentity::from_ir_config(&config).unwrap();
        assert!(first.source_prefix().starts_with(RIAN_TABLE_SOURCE_PREFIX));

        config.providers[0].account_id.clear();
        assert!(RianTableIdentity::from_ir_config(&config).is_none());
    }

    #[test]
    fn converts_folders_and_courses_to_existing_table_model() {
        let resource = RianTableResource {
            id: "0".to_string(),
            attributes: RianTable {
                name: "rianIR POPULAR".to_string(),
                symbol: "P".to_string(),
                folders: vec![RianTableFolder {
                    name: "24H".to_string(),
                    charts: vec![RianTableChart {
                        title: "Song".to_string(),
                        subtitle: "[Another]".to_string(),
                        artist: "Artist".to_string(),
                        subartist: String::new(),
                        md5: "a".repeat(32),
                        sha256: "b".repeat(64),
                        level: serde_json::Value::Null,
                    }],
                }],
                courses: vec![RianTableCourse {
                    name: "Course".to_string(),
                    sha256: String::new(),
                    constraint: vec!["grade".to_string(), "ln".to_string()],
                    charts: vec![RianTableChart {
                        title: "Song".to_string(),
                        subtitle: String::new(),
                        artist: String::new(),
                        subartist: String::new(),
                        md5: "a".repeat(32),
                        sha256: "b".repeat(64),
                        level: serde_json::Value::Null,
                    }],
                }],
            },
        };

        let tables = convert_resources(&identity(), vec![resource], 123);
        assert_eq!(tables[0].level_order, vec!["24H"]);
        assert!(tables[0].symbol.is_empty());
        assert_eq!(tables[0].entries[0].level, "24H");
        assert_eq!(tables[0].entries[0].title, "Song [Another]");
        assert_eq!(tables[0].courses[0].entries.len(), 1);
        assert!(tables[0].courses[0].constraints.is_dan());
    }
}
