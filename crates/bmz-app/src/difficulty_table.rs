use anyhow::{Result, bail};
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

pub struct FetchedTableEntry {
    pub level: String,
    pub md5: String,
    pub sha256: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
}

#[derive(Deserialize)]
struct HeaderJson {
    name: String,
    symbol: String,
    #[serde(default)]
    data_url: DataUrl,
    #[serde(default, deserialize_with = "deserialize_level_order")]
    level_order: Vec<String>,
    /// Embedded course definitions in beatoraja table format.
    /// Stored as raw JSON to reuse `parse_beatoraja_course_json`.
    #[serde(default)]
    course: Option<serde_json::Value>,
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
}

/// Fetches and parses a BMS difficulty table.
///
/// `source_url` can be either the HTML page containing
/// `<meta name="bmstable" content="...">` or a direct header JSON URL.
pub async fn fetch_difficulty_table(
    source_url: &str,
    fetched_at: i64,
) -> Result<FetchedDifficultyTable> {
    let client = reqwest::Client::builder().user_agent("bmz-player/0.1").build()?;

    let head_url = if source_url.ends_with(".json") {
        source_url.to_string()
    } else {
        let html = client.get(source_url).send().await?.text().await?;
        let rel = find_bmstable_meta(&html)
            .ok_or_else(|| anyhow::anyhow!("no <meta name=\"bmstable\"> at {source_url}"))?;
        resolve_url(source_url, &rel)
    };

    let header_body = client.get(&head_url).send().await?.text().await?;
    let header: HeaderJson = serde_json::from_str(strip_bom(&header_body))?;

    let data_urls: Vec<String> =
        header.data_url.into_vec().into_iter().map(|u| resolve_url(&head_url, &u)).collect();

    if data_urls.is_empty() {
        bail!("difficulty table header has no data_url: {head_url}");
    }

    let mut entries = Vec::new();
    let mut level_order = header.level_order;

    for data_url in &data_urls {
        let data_body = client.get(data_url).send().await?.text().await?;
        let data: Vec<DataEntry> = serde_json::from_str(strip_bom(&data_body))?;

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
            });
        }
    }

    let courses = parse_courses_from_header(source_url, &header.course);

    Ok(FetchedDifficultyTable {
        source_url: source_url.to_string(),
        head_url,
        name: header.name,
        symbol: header.symbol,
        level_order,
        entries,
        courses,
        fetched_at,
    })
}

fn parse_courses_from_header(
    source_url: &str,
    course_json: &Option<serde_json::Value>,
) -> Vec<CourseDefinition> {
    let Some(value) = course_json else {
        return Vec::new();
    };

    // Some tables (e.g. Stella) wrap the courses in an extra outer array: [[c1, c2, ...]]
    // Flatten one level so that parse_beatoraja_course_json always receives [c1, c2, ...].
    let flat = match value {
        serde_json::Value::Array(outer) => {
            let all_inner_arrays = outer.iter().all(|v| v.is_array());
            if all_inner_arrays {
                // Flatten [[c1, c2], [c3, c4]] → [c1, c2, c3, c4]
                let flat: Vec<serde_json::Value> =
                    outer.iter().flat_map(|v| v.as_array().cloned().unwrap_or_default()).collect();
                serde_json::Value::Array(flat)
            } else {
                value.clone()
            }
        }
        other => other.clone(),
    };

    let json_str = match serde_json::to_string(&flat) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let source = format!("table:{source_url}");
    match crate::course::parse_beatoraja_course_json(&source, &json_str) {
        Ok(courses) => courses,
        Err(err) => {
            tracing::warn!(%err, %source_url, "failed to parse courses from difficulty table header");
            Vec::new()
        }
    }
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
}
