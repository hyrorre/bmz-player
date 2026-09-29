use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use bmz_core::course::CourseDefinition;
use serde::Deserialize;

pub struct FetchedDifficultyTable {
    pub source_url: String,
    pub head_url: String,
    pub name: String,
    pub symbol: String,
    pub level_order: Vec<String>,
    pub entries: Vec<FetchedTableEntry>,
    /// Courses embedded in the table header JSON.
    pub courses: Vec<CourseDefinition>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct FetchedTableEntry {
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
}

#[derive(Deserialize)]
struct HeaderJson {
    name: String,
    symbol: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    data_url: DataUrl,
    /// Per-data-file level remapping used when multiple tables are merged.
    #[serde(default)]
    data_rule: Vec<HashMap<String, String>>,
    #[serde(default, deserialize_with = "deserialize_level_order")]
    level_order: Vec<String>,
    /// Embedded course definitions in beatoraja table format.
    /// Kept as raw JSON because both one- and two-dimensional arrays are used.
    #[serde(default)]
    course: Option<serde_json::Value>,
    /// Legacy jbmstable-parser course field. Entries implicitly use
    /// grade_mirror and gauge_lr2 constraints.
    #[serde(default)]
    grade: Option<serde_json::Value>,
}

/// Deserializes `level_order`, tolerating entries that are numbers or strings.
///
/// Some tables (e.g. genocide insane) write `level_order` as a mix of integers
/// and strings, e.g. `[1, 2, ..., "???"]`.
fn deserialize_level_order<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .collect())
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum DataUrl {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

impl DataUrl {
    fn into_vec(self) -> Vec<String> {
        match self {
            DataUrl::None => vec![],
            DataUrl::Single(s) => vec![s],
            DataUrl::Multiple(v) => v,
        }
    }
}

#[derive(Deserialize)]
struct DataEntry {
    level: Option<serde_json::Value>,
    #[serde(default)]
    md5: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(
        default,
        alias = "appendUrl",
        alias = "appendURL",
        alias = "append_url",
        alias = "urlDiff",
        alias = "url_diff"
    )]
    appendurl: Option<String>,
    #[serde(default)]
    ipfs: Option<String>,
    #[serde(
        default,
        alias = "appendIpfs",
        alias = "append_ipfs",
        alias = "ipfsDiff",
        alias = "ipfs_diff"
    )]
    appendipfs: Option<String>,
}

/// Fetches and parses a BMS difficulty table.
///
/// `source_url` can be either the HTML page containing
/// `<meta name="bmstable" content="...">` or a direct header JSON URL.
pub async fn fetch_difficulty_table(
    source_url: &str,
    fetched_at: i64,
) -> Result<FetchedDifficultyTable> {
    let client = build_difficulty_table_client()?;
    fetch_difficulty_table_with_client(&client, source_url, fetched_at).await
}

/// Creates the HTTP client shared by a batch of difficulty-table fetches.
///
/// Sharing one client keeps the connection pool available to every source in
/// the batch instead of creating a separate pool for each table.
pub(crate) fn build_difficulty_table_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().user_agent("bmz-player/0.1").build()?)
}

/// Fetches and parses one difficulty table with an existing HTTP client.
///
/// The caller can run multiple independent sources concurrently while keeping
/// the per-table HTML → header → data-file dependency order intact.
pub(crate) async fn fetch_difficulty_table_with_client(
    client: &reqwest::Client,
    source_url: &str,
    fetched_at: i64,
) -> Result<FetchedDifficultyTable> {
    let head_url = if source_url.ends_with(".json") {
        source_url.to_string()
    } else {
        let response = fetch_table_response(client, source_url, "HTML").await?;
        let rel = find_bmstable_meta(&response.body)
            .with_context(|| format!("no <meta name=\"bmstable\">: {}", response.diagnostic()))?;
        resolve_url(source_url, &rel)
    };

    let response = fetch_table_response(client, &head_url, "header").await?;
    let header: HeaderJson = response.parse_json()?;
    let symbol = table_level_symbol(&header);

    let data_urls: Vec<String> =
        header.data_url.into_vec().into_iter().map(|u| resolve_url(&head_url, &u)).collect();

    if data_urls.is_empty() {
        bail!("difficulty table header has no data_url: {head_url}");
    }

    let mut entries = Vec::new();
    let mut level_order = header.level_order;

    for (data_index, data_url) in data_urls.iter().enumerate() {
        let response = fetch_table_response(client, data_url, "data").await?;
        let data: Vec<DataEntry> = response.parse_json()?;
        let data_rule = header.data_rule.get(data_index);

        for entry in data {
            let md5 = entry.md5.unwrap_or_default().to_lowercase();
            let sha256 = entry.sha256.unwrap_or_default().to_lowercase();
            if md5.len() < 24 && sha256.len() < 24 {
                continue;
            }
            let level = match entry.level {
                Some(serde_json::Value::String(s)) => s,
                Some(serde_json::Value::Number(n)) => n.to_string(),
                Some(_) | None => continue,
            };
            let Some(level) = map_merged_level(level, data_rule) else {
                continue;
            };
            if !level_order.contains(&level) {
                level_order.push(level.clone());
            }
            entries.push(FetchedTableEntry {
                level,
                md5,
                sha256,
                title: entry.title.unwrap_or_default(),
                artist: entry.artist.unwrap_or_default(),
                comment: entry.comment.unwrap_or_default(),
                url: trimmed_or_default(entry.url),
                append_url: trimmed_or_default(entry.appendurl),
                ipfs: trimmed_or_default(entry.ipfs),
                append_ipfs: trimmed_or_default(entry.appendipfs),
            });
        }
    }

    let courses = match &header.course {
        Some(_) => parse_courses_from_header(source_url, &header.course),
        None => parse_legacy_grades_from_header(source_url, &header.grade),
    };
    Ok(FetchedDifficultyTable {
        source_url: source_url.to_string(),
        head_url,
        name: header.name,
        symbol,
        level_order,
        entries,
        courses,
        fetched_at,
    })
}

struct TableResponse {
    stage: &'static str,
    requested_url: String,
    final_url: String,
    status: reqwest::StatusCode,
    content_type: String,
    body: String,
}

impl TableResponse {
    fn diagnostic(&self) -> String {
        let mut chars = self.body.chars();
        let mut preview: String = chars.by_ref().take(256).collect();
        if chars.next().is_some() {
            preview.push('…');
        }
        format!(
            "stage={} url={} final_url={} status={} content_type={:?} body_prefix={preview:?}",
            self.stage, self.requested_url, self.final_url, self.status, self.content_type,
        )
    }

    fn parse_json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(strip_bom(&self.body))
            .with_context(|| format!("invalid difficulty table JSON: {}", self.diagnostic()))
    }
}

async fn fetch_table_response(
    client: &reqwest::Client,
    url: &str,
    stage: &'static str,
) -> Result<TableResponse> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("difficulty table request failed: stage={stage} url={url}"))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("<missing>")
        .to_string();
    let body = response.text().await.with_context(|| {
        format!(
            "difficulty table response read failed: stage={stage} url={url} final_url={final_url} status={status} content_type={content_type:?}"
        )
    })?;
    let response = TableResponse {
        stage,
        requested_url: url.to_string(),
        final_url,
        status,
        content_type,
        body,
    };
    if !status.is_success() {
        bail!("difficulty table HTTP error: {}", response.diagnostic());
    }
    Ok(response)
}

fn table_level_symbol(header: &HeaderJson) -> String {
    header
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .unwrap_or(&header.symbol)
        .to_string()
}

fn map_merged_level(level: String, data_rule: Option<&HashMap<String, String>>) -> Option<String> {
    match data_rule.and_then(|rule| rule.get(&level)) {
        Some(mapped) if mapped.is_empty() => None,
        Some(mapped) => Some(mapped.clone()),
        None => Some(level),
    }
}

fn trimmed_or_default(value: Option<String>) -> String {
    value.map(|value| value.trim().to_string()).unwrap_or_default()
}

fn parse_courses_from_header(
    source_url: &str,
    course_json: &Option<serde_json::Value>,
) -> Vec<CourseDefinition> {
    let Some(value) = course_json else {
        return Vec::new();
    };

    parse_course_values(source_url, flatten_course_values(value), false)
}

fn parse_legacy_grades_from_header(
    source_url: &str,
    grade_json: &Option<serde_json::Value>,
) -> Vec<CourseDefinition> {
    let Some(value) = grade_json else {
        return Vec::new();
    };

    parse_course_values(source_url, flatten_course_values(value), true)
}

fn flatten_course_values(value: &serde_json::Value) -> Vec<serde_json::Value> {
    // Some tables (e.g. Stella) wrap the courses in an extra outer array: [[c1, c2, ...]]
    // Flatten one level to match jbmstable-parser's Course[][] handling.
    match value {
        serde_json::Value::Array(outer) => {
            let all_inner_arrays = outer.iter().all(|v| v.is_array());
            if all_inner_arrays {
                // Flatten [[c1, c2], [c3, c4]] → [c1, c2, c3, c4]
                outer.iter().flat_map(|v| v.as_array().cloned().unwrap_or_default()).collect()
            } else {
                outer.clone()
            }
        }
        other => vec![other.clone()],
    }
}

fn parse_course_values(
    source_url: &str,
    values: Vec<serde_json::Value>,
    legacy_grade: bool,
) -> Vec<CourseDefinition> {
    let source = format!("table:{source_url}");
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut value)| {
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("No Course Title")
                .to_string();
            if legacy_grade && let Some(object) = value.as_object_mut() {
                object.insert(
                    "constraint".to_string(),
                    serde_json::json!(["grade_mirror", "gauge_lr2"]),
                );
            }
            match crate::course::parse_beatoraja_course_value(&source, index, value) {
                Ok(course) => Some(course),
                Err(err) => {
                    tracing::warn!(
                        %err,
                        %source_url,
                        course_index = index,
                        course_name = %name,
                        "failed to parse course from difficulty table header"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Test-visible wrapper for `parse_courses_from_header`.
#[cfg(test)]
pub fn parse_courses_from_header_for_test(
    source_url: &str,
    course_json: &Option<serde_json::Value>,
) -> Vec<CourseDefinition> {
    parse_courses_from_header(source_url, course_json)
}

/// Strips a leading UTF-8 BOM (U+FEFF) if present.
///
/// Some tables (e.g. genocide insane) serve their header/data JSON with a BOM,
/// which `serde_json` refuses to parse.
fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

fn find_bmstable_meta(html: &str) -> Option<String> {
    for line in html.lines() {
        let lower = line.to_lowercase();
        if lower.contains("<meta") && lower.contains("name=\"bmstable\"") {
            return extract_attr(line, "content");
        }
    }
    None
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let search = format!("{attr}=\"");
    let lower = tag.to_lowercase();
    let start = lower.find(&search)? + search.len();
    let rest = &tag[start..];
    Some(rest[..rest.find('"')?].to_string())
}

fn resolve_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let path = path.strip_prefix("./").unwrap_or(path);
    let base_dir = base.rfind('/').map(|i| &base[..=i]).unwrap_or(base);
    format!("{base_dir}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_json_reports_response_details_and_bounded_unicode_preview() {
        let response = TableResponse {
            stage: "data",
            requested_url: "https://example.com/data".into(),
            final_url: "https://example.com/redirected".into(),
            status: reqwest::StatusCode::OK,
            content_type: "text/html".into(),
            body: format!("<html>\n{}SECRET_TAIL", "あ".repeat(300)),
        };
        let error = format!("{:#}", response.parse_json::<Vec<DataEntry>>().err().unwrap());
        for detail in [
            "stage=data",
            "url=https://example.com/data",
            "final_url=https://example.com/redirected",
            "status=200 OK",
            "content_type=\"text/html\"",
            "expected value at line 1 column 1",
            "…",
        ] {
            assert!(error.contains(detail), "missing {detail}: {error}");
        }
        assert!(!error.contains("SECRET_TAIL"));
        assert!(!error.contains('\n'));
    }

    #[test]
    fn find_bmstable_meta_extracts_content() {
        let html = r#"<html><head><meta name="bmstable" content="header.json"></head></html>"#;
        assert_eq!(find_bmstable_meta(html), Some("header.json".to_string()));
    }

    #[test]
    fn find_bmstable_meta_handles_content_before_name() {
        let html = r#"<meta content="header.json" name="bmstable">"#;
        assert_eq!(find_bmstable_meta(html), Some("header.json".to_string()));
    }

    #[test]
    fn find_bmstable_meta_returns_none_when_absent() {
        assert_eq!(find_bmstable_meta("<html></html>"), None);
    }

    #[test]
    fn strip_bom_removes_leading_bom() {
        assert_eq!(strip_bom("\u{feff}{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_bom("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn parse_header_json_with_bom() {
        let body = "\u{feff}{\"name\":\"X\",\"symbol\":\"x\",\"data_url\":\"score.json\"}";
        let header: HeaderJson =
            serde_json::from_str(strip_bom(body)).expect("BOM-prefixed header should parse");
        assert_eq!(header.name, "X");
    }

    #[test]
    fn parse_header_with_mixed_level_order() {
        let body = r#"{"name":"X","symbol":"x","data_url":"score.json",
            "level_order":[1,2,3,"???"]}"#;
        let header: HeaderJson =
            serde_json::from_str(body).expect("mixed numeric/string level_order should parse");
        assert_eq!(header.level_order, vec!["1", "2", "3", "???"]);
    }

    #[test]
    fn table_level_symbol_prefers_tag_and_falls_back_to_symbol() {
        let tagged: HeaderJson =
            serde_json::from_str(r#"{"name":"X","symbol":"★","tag":"st","data_url":"score.json"}"#)
                .unwrap();
        let untagged: HeaderJson =
            serde_json::from_str(r#"{"name":"X","symbol":"★","data_url":"score.json"}"#).unwrap();

        assert_eq!(table_level_symbol(&tagged), "st");
        assert_eq!(table_level_symbol(&untagged), "★");
    }

    #[test]
    fn data_rule_remaps_and_excludes_levels() {
        let rule =
            HashMap::from([("1".to_string(), "10".to_string()), ("2".to_string(), String::new())]);

        assert_eq!(map_merged_level("1".to_string(), Some(&rule)), Some("10".to_string()));
        assert_eq!(map_merged_level("2".to_string(), Some(&rule)), None);
        assert_eq!(map_merged_level("3".to_string(), Some(&rule)), Some("3".to_string()));
    }

    #[test]
    fn parse_download_metadata_aliases() {
        let entry: DataEntry = serde_json::from_str(
            r#"{
                "level": "1",
                "md5": "00112233445566778899aabbccddeeff",
                "url": " https://example.com/song ",
                "url_diff": "https://example.com/diff",
                "ipfs": "/ipfs/bafy-main",
                "ipfs_diff": "/ipfs/bafy-diff"
            }"#,
        )
        .unwrap();

        assert_eq!(trimmed_or_default(entry.url), "https://example.com/song");
        assert_eq!(entry.appendurl.as_deref(), Some("https://example.com/diff"));
        assert_eq!(entry.ipfs.as_deref(), Some("/ipfs/bafy-main"));
        assert_eq!(entry.appendipfs.as_deref(), Some("/ipfs/bafy-diff"));
    }

    #[test]
    fn parse_download_metadata_accepts_all_append_aliases() {
        let cases = [
            r#"{"appendurl":"https://example.com/diff","appendipfs":"/ipfs/diff"}"#,
            r#"{"appendUrl":"https://example.com/diff","appendIpfs":"/ipfs/diff"}"#,
            r#"{"appendURL":"https://example.com/diff","append_ipfs":"/ipfs/diff"}"#,
            r#"{"append_url":"https://example.com/diff","ipfsDiff":"/ipfs/diff"}"#,
            r#"{"urlDiff":"https://example.com/diff","ipfs_diff":"/ipfs/diff"}"#,
            r#"{"url_diff":"https://example.com/diff","appendIpfs":"/ipfs/diff"}"#,
        ];

        for body in cases {
            let entry: DataEntry = serde_json::from_str(body).unwrap();
            assert_eq!(entry.appendurl.as_deref(), Some("https://example.com/diff"));
            assert_eq!(entry.appendipfs.as_deref(), Some("/ipfs/diff"));
        }
    }

    #[test]
    fn resolve_url_handles_relative_path() {
        assert_eq!(
            resolve_url("https://example.com/table/", "header.json"),
            "https://example.com/table/header.json"
        );
    }

    #[test]
    fn resolve_url_passes_through_absolute_url() {
        assert_eq!(
            resolve_url("https://base.com/", "https://other.com/header.json"),
            "https://other.com/header.json"
        );
    }

    #[test]
    fn resolve_url_strips_dot_slash_prefix() {
        assert_eq!(
            resolve_url("https://example.com/table/index.html", "./header.json"),
            "https://example.com/table/header.json"
        );
    }

    // Integration test: parse the full Stella header JSON exactly as it arrives
    // from HTTP (via HeaderJson deserialization) and verify courses are extracted.
    #[test]
    fn parse_stella_header_json_extracts_courses() {
        let header_json = r#"{
  "name": "Stella",
  "symbol": "st",
  "data_url": "score.json",
  "course": [[
    {
      "name": "Stella Skill Simulator 4th st0",
      "constraint": ["grade_mirror", "gauge_lr2", "ln"],
      "trophy": [
        {"name": "silvermedal", "missrate": 5.0, "scorerate": 70.0},
        {"name": "goldmedal",   "missrate": 2.5, "scorerate": 85.0}
      ],
      "md5": [
        "349bc491ec40d5595412637d8a4c8d2e",
        "baee0a1921fc5041b44d7d87c7b5548d",
        "72b1ce4b2051bd2a396dfa11a2d785ee",
        "3a1661a3eaafa13f976e1010d5b87ca0"
      ]
    },
    {
      "name": "Stella Skill Simulator 4th st1",
      "constraint": ["grade_mirror", "gauge_lr2", "ln"],
      "trophy": [
        {"name": "silvermedal", "missrate": 5.0, "scorerate": 70.0},
        {"name": "goldmedal",   "missrate": 2.5, "scorerate": 85.0}
      ],
      "md5": [
        "a87c666d3232097ac5359d6913ad5b23",
        "d26ad712ceef97a5a843226bd77553eb",
        "965d42e6aa003f95958cd7bcf3a59bec",
        "8b327890a493ead1825472d4c4f7bc79"
      ]
    }
  ]]
}"#;

        // Simulate how fetch_difficulty_table deserializes the header
        let header: HeaderJson = serde_json::from_str(header_json).expect("header parse failed");
        assert_eq!(header.name, "Stella");
        assert!(header.course.is_some(), "course field should be present");

        let courses =
            parse_courses_from_header("https://stellabms.xyz/st/table.html", &header.course);
        assert_eq!(courses.len(), 2, "should have parsed 2 courses");
        assert_eq!(courses[0].title, "Stella Skill Simulator 4th st0");
        assert_eq!(courses[0].entries.len(), 4);
        assert_eq!(courses[0].entries[0].md5.as_deref(), Some("349bc491ec40d5595412637d8a4c8d2e"));
    }

    #[test]
    fn parse_dpdelta_charts_course_extracts_sha256_entries() {
        let value = serde_json::json!([[
            {
                "name": "GENOSIDE 2018 DP段位認定 初段",
                "constraint": ["grade_random", "gauge_lr2"],
                "trophy": [],
                "charts": [
                    {
                        "title": "f.alive",
                        "artist": "Is-m",
                        "sha256": "e6c65d09e9e2caf078efd7a1471886a1c916350dde05606a82e00448e988a09d"
                    }
                ]
            }
        ]]);

        let courses = parse_courses_from_header(
            "https://deltabms.yaruki0.net/table/data/dpdelta_head.json",
            &Some(value),
        );

        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].entries[0].title_hint, "f.alive");
        assert_eq!(
            courses[0].entries[0].sha256.as_deref(),
            Some("e6c65d09e9e2caf078efd7a1471886a1c916350dde05606a82e00448e988a09d")
        );
    }

    #[test]
    fn mixed_md5_and_charts_courses_skip_only_invalid_course() {
        let value = serde_json::json!([[
            {
                "name": "MD5 course",
                "constraint": ["grade_mirror"],
                "md5": ["349bc491ec40d5595412637d8a4c8d2e"]
            },
            {
                "name": "Invalid course"
            },
            {
                "name": "Charts course",
                "constraint": ["grade_mirror"],
                "charts": [
                    {
                        "title": "Spectacles Bridge",
                        "md5": "97e192275ca1f8295caf2d7db8145049"
                    }
                ]
            }
        ]]);

        let courses =
            parse_courses_from_header("https://rattoto10.jounin.jp/table.html", &Some(value));

        assert_eq!(courses.len(), 2);
        assert_eq!(courses[0].title, "MD5 course");
        assert!(courses[0].key.ends_with("#0"));
        assert_eq!(courses[1].title, "Charts course");
        assert!(courses[1].key.ends_with("#2"));
        assert_eq!(courses[1].entries[0].title_hint, "Spectacles Bridge");
    }

    #[test]
    fn legacy_grade_header_uses_beatoraja_constraints() {
        let value = serde_json::json!([
            {
                "name": "Legacy Grade",
                "md5": ["349bc491ec40d5595412637d8a4c8d2e"]
            }
        ]);

        let courses =
            parse_legacy_grades_from_header("https://example.com/legacy/table.html", &Some(value));

        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].kind, bmz_core::course::CourseKind::Dan);
        assert_eq!(
            courses[0].constraints.class,
            bmz_core::course::CourseClassConstraint::GradeMirrorAllowed
        );
        assert_eq!(courses[0].constraints.gauge, bmz_core::course::CourseGaugeConstraint::Lr2);
    }
}
