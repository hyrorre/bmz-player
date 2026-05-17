use anyhow::{Result, bail};
use serde::Deserialize;

pub struct FetchedDifficultyTable {
    pub source_url: String,
    pub head_url: String,
    pub name: String,
    pub symbol: String,
    pub level_order: Vec<String>,
    pub entries: Vec<FetchedTableEntry>,
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
    #[serde(default)]
    level_order: Vec<String>,
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

    let header: HeaderJson = client.get(&head_url).send().await?.json().await?;

    let data_urls: Vec<String> =
        header.data_url.into_vec().into_iter().map(|u| resolve_url(&head_url, &u)).collect();

    if data_urls.is_empty() {
        bail!("difficulty table header has no data_url: {head_url}");
    }

    let mut entries = Vec::new();
    let mut level_order = header.level_order;

    for data_url in &data_urls {
        let data: Vec<DataEntry> = client.get(data_url).send().await?.json().await?;

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

    Ok(FetchedDifficultyTable {
        source_url: source_url.to_string(),
        head_url,
        name: header.name,
        symbol: header.symbol,
        level_order,
        entries,
        fetched_at,
    })
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
}
