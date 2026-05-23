use anyhow::{Result, bail};
use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseTrophy};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BeatorajaCourseFile {
    One(BeatorajaCourse),
    Many(Vec<BeatorajaCourse>),
}

#[derive(Debug, Deserialize)]
struct BeatorajaCourse {
    #[serde(default)]
    name: String,
    #[serde(default, alias = "song")]
    hash: Vec<BeatorajaCourseSong>,
    #[serde(default)]
    constraint: Vec<String>,
    #[serde(default)]
    trophy: Vec<BeatorajaTrophy>,
    #[serde(default = "default_release")]
    release: bool,
}

#[derive(Debug, Deserialize)]
struct BeatorajaCourseSong {
    #[serde(default)]
    title: String,
    #[serde(default)]
    md5: String,
    #[serde(default)]
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct BeatorajaTrophy {
    #[serde(default)]
    name: String,
    #[serde(default)]
    missrate: f32,
    #[serde(default)]
    scorerate: f32,
}

fn default_release() -> bool {
    true
}

pub fn parse_beatoraja_course_json(source: &str, json: &str) -> Result<Vec<CourseDefinition>> {
    let file: BeatorajaCourseFile = serde_json::from_str(json)?;
    let courses = match file {
        BeatorajaCourseFile::One(course) => vec![course],
        BeatorajaCourseFile::Many(courses) => courses,
    };

    courses
        .into_iter()
        .enumerate()
        .map(|(index, course)| convert_beatoraja_course(source, index, course))
        .collect()
}

fn convert_beatoraja_course(
    source: &str,
    index: usize,
    course: BeatorajaCourse,
) -> Result<CourseDefinition> {
    if course.hash.is_empty() {
        bail!("course has no entries");
    }

    let title =
        if course.name.trim().is_empty() { "No Course Title".to_string() } else { course.name };
    let constraints =
        CourseConstraints::from_beatoraja_names(course.constraint.iter().map(String::as_str));
    let kind = CourseDefinition::derive_kind_from_constraints(&constraints);
    let entries = course
        .hash
        .into_iter()
        .enumerate()
        .map(|(entry_index, song)| CourseEntry {
            title_hint: if song.title.trim().is_empty() {
                format!("course {}", entry_index + 1)
            } else {
                song.title
            },
            md5: normalize_hash(song.md5, 32),
            sha256: normalize_hash(song.sha256, 64),
            chart_id: None,
        })
        .collect();
    let trophies = course
        .trophy
        .into_iter()
        .map(|trophy| CourseTrophy {
            name: trophy.name,
            max_miss_rate: trophy.missrate,
            min_score_rate: trophy.scorerate,
        })
        .collect();

    Ok(CourseDefinition {
        key: format!("{source}#{index}"),
        title,
        kind,
        entries,
        constraints,
        trophies,
        release: course.release,
    })
}

fn normalize_hash(hash: String, expected_len: usize) -> Option<String> {
    let trimmed = hash.trim().to_ascii_lowercase();
    (trimmed.len() == expected_len && trimmed.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use bmz_core::course::{
        CourseClassConstraint, CourseGaugeConstraint, CourseKind, CourseSpeedConstraint,
    };

    use super::*;

    #[test]
    fn parses_beatoraja_course_array() {
        let json = r#"[
          {
            "name": "七段",
            "constraint": ["grade_mirror", "no_speed", "gauge_7k"],
            "hash": [
              {"title": "Song A", "md5": "00112233445566778899aabbccddeeff", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
            ],
            "trophy": [{"name": "gold", "missrate": 2.5, "scorerate": 88.0}]
          }
        ]"#;

        let courses = parse_beatoraja_course_json("course/default.json", json).unwrap();

        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].key, "course/default.json#0");
        assert_eq!(courses[0].title, "七段");
        assert_eq!(courses[0].kind, CourseKind::Dan);
        assert_eq!(courses[0].constraints.class, CourseClassConstraint::GradeMirrorAllowed);
        assert_eq!(courses[0].constraints.speed, CourseSpeedConstraint::NoSpeed);
        assert_eq!(courses[0].constraints.gauge, CourseGaugeConstraint::Keys7);
        assert_eq!(courses[0].entries[0].title_hint, "Song A");
        assert_eq!(courses[0].trophies[0].name, "gold");
    }

    #[test]
    fn parses_single_course_and_normalizes_defaults() {
        let json = r#"{"hash":[{"sha256":"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"}]}"#;

        let courses = parse_beatoraja_course_json("single.json", json).unwrap();

        assert_eq!(courses[0].title, "No Course Title");
        assert_eq!(courses[0].kind, CourseKind::Course);
        assert_eq!(courses[0].entries[0].title_hint, "course 1");
        assert_eq!(
            courses[0].entries[0].sha256.as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }
}
