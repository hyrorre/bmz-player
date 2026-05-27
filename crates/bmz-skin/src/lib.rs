use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use bmz_render::skin::SkinDocument;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;

mod lua;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinKind {
    Play,
    Select,
    Decide,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinLoadWarning {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LoadedSkinDocument {
    pub document: SkinDocument,
    pub warnings: Vec<SkinLoadWarning>,
}

#[derive(Debug, Clone)]
pub struct LoadedLuaSkinValue {
    pub value: JsonValue,
    pub warnings: Vec<SkinLoadWarning>,
}

pub fn load_beatoraja_json_skin(path: &Path, enabled_options: &[i32]) -> Result<SkinDocument> {
    SkinDocument::load_beatoraja_json_with_options(path, enabled_options)
}

pub fn load_beatoraja_json_skin_with_defaults(path: &Path) -> Result<SkinDocument> {
    SkinDocument::load_beatoraja_json(path)
}

pub fn load_lua_skin(
    path: &Path,
    _kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedSkinDocument> {
    let loaded = load_lua_skin_value(path, options, files)?;
    let value = normalize_json_skin_integer_numbers(loaded.value);
    let document = serde_json::from_value(value)
        .with_context(|| format!("failed to parse lua skin as document: {}", path.display()))?;
    Ok(LoadedSkinDocument { document, warnings: loaded.warnings })
}

pub fn load_lua_skin_value(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    lua::load_lua_skin_value(path, options, files)
}

pub fn load_lua_skin_header_value(path: &Path) -> Result<LoadedLuaSkinValue> {
    lua::load_lua_skin_header_value(path)
}

fn normalize_json_skin_integer_numbers(value: JsonValue) -> JsonValue {
    normalize_json_skin_integer_numbers_for_key(None, value)
}

fn normalize_json_skin_integer_numbers_for_key(key: Option<&str>, value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(mut values)
            if is_json_skin_scalar_integer_key(key) && values.len() == 1 =>
        {
            normalize_json_skin_integer_value(values.remove(0))
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| {
                    if is_json_skin_integer_key(key) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(key, value)
                    }
                })
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = normalize_json_skin_integer_numbers_for_key(Some(&key), value);
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        JsonValue::Number(number) if is_json_skin_integer_key(key) => {
            json_number_to_rounded_i64(&number)
                .and_then(serde_json::Number::from_i128)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Number(number))
        }
        value => value,
    }
}

fn is_json_skin_scalar_integer_key(key: Option<&str>) -> bool {
    is_json_skin_integer_key(key) && !matches!(key, Some("op" | "offsets" | "time"))
}

fn normalize_json_skin_integer_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Number(number) => json_number_to_rounded_i64(&number)
            .and_then(serde_json::Number::from_i128)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Number(number)),
        JsonValue::Array(values) => {
            JsonValue::Array(values.into_iter().map(normalize_json_skin_integer_value).collect())
        }
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = normalize_json_skin_integer_numbers_for_key(Some(&key), value);
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        value => value,
    }
}

fn json_number_to_rounded_i64(number: &serde_json::Number) -> Option<i128> {
    if let Some(value) = number.as_i64() {
        return Some(value as i128);
    }
    if let Some(value) = number.as_u64() {
        return Some(value as i128);
    }
    let value = number.as_f64()?;
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    Some(value.round() as i128)
}

fn is_json_skin_integer_key(key: Option<&str>) -> bool {
    matches!(
        key,
        Some(
            "a" | "acc"
                | "align"
                | "angle"
                | "b"
                | "blend"
                | "center"
                | "click"
                | "cycle"
                | "digit"
                | "disapearLine"
                | "divx"
                | "divy"
                | "endtime"
                | "filter"
                | "g"
                | "h"
                | "index"
                | "len"
                | "loop"
                | "max"
                | "min"
                | "offset"
                | "offsets"
                | "op"
                | "padding"
                | "parts"
                | "r"
                | "range"
                | "ref"
                | "size"
                | "space"
                | "starttime"
                | "stretch"
                | "time"
                | "timer"
                | "type"
                | "w"
                | "x"
                | "y"
                | "zeropadding"
        )
    )
}

pub fn convert_lua_skin_to_json_file(
    input: &Path,
    output: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<Vec<SkinLoadWarning>> {
    let report = lua::convert_lua_skin_to_json(input, output, options, files)?;
    Ok(report.warnings.into_iter().map(|message| SkinLoadWarning { message }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn lua_skin_loads_main_state_draw_and_value_functions() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                value = {
                    { id = "score", src = 1, x = 0, y = 0, w = 10, h = 10, value = function()
                        return main_state.number(71)
                    end }
                },
                destination = {
                    { id = "panel", draw = function() return main_state.option(1) end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("play7.luaskin"),
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.document.value[0].ref_id, 71);
        let bmz_render::skin::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.draw, "option(1)");
    }

    #[test]
    fn lua_skin_rejects_paths_outside_root() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("play7.luaskin"), "return dofile('../outside.lua')").unwrap();
        fs::write(root.parent().unwrap().join("outside.lua"), "return {}").unwrap();

        let err = load_lua_skin(
            &root.join("play7.luaskin"),
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("escapes skin root"));
    }

    #[test]
    fn lua_skin_config_get_path_ignores_beatoraja_filter_suffix() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
        fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local cover_path = "parts/lanecover_lift/*.png|lanecover|"
            if skin_config then
                cover_path = skin_config.get_path(cover_path)
            end
            return {
                type = 0,
                source = {
                    {
                        id = "cover",
                        path = cover_path
                    }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["source"][0]["path"].as_str().and_then(|path| {
                std::path::Path::new(path).file_name().and_then(|name| name.to_str())
            }),
            Some("default.png")
        );
    }

    #[test]
    fn lua_skin_header_load_skips_skin_config_body() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/frame.lua"), "return {}").unwrap();
        fs::write(
            root.join("play5.luaskin"),
            r#"
            if skin_config then
                dofile(skin_config.get_path("parts/*") .. "/frame.lua")
            end
            return {
                name = "Header Only",
                type = 1
            }
            "#,
        )
        .unwrap();

        let header = load_lua_skin_header_value(&root.join("play5.luaskin")).unwrap();

        assert_eq!(header.value["name"], "Header Only");
        assert_eq!(header.value["type"], 1);
    }

    #[test]
    fn lua_skin_config_get_path_applies_user_file_selection() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/a.png"), []).unwrap();
        fs::write(root.join("parts/z.png"), []).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local cover_path = "parts/*.png"
            if skin_config then
                cover_path = skin_config.get_path(cover_path)
            end
            return {
                type = 0,
                filepath = {
                    { name = "Cover", path = "parts/*.png", def = "a" }
                },
                source = {
                    { id = "cover", path = cover_path }
                }
            }
            "#,
        )
        .unwrap();

        let files = BTreeMap::from([("Cover".to_string(), "parts/z.png".to_string())]);
        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &files).unwrap();

        assert_eq!(
            loaded.value["source"][0]["path"].as_str().and_then(|path| {
                std::path::Path::new(path).file_name().and_then(|name| name.to_str())
            }),
            // ユーザ選択 (z.png) を採用する。ソート先頭候補は a.png。
            Some("z.png")
        );
    }

    #[test]
    fn lua_skin_config_get_path_falls_back_when_selection_missing() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/a.png"), []).unwrap();
        fs::write(root.join("parts/z.png"), []).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local cover_path = "parts/*.png"
            if skin_config then
                cover_path = skin_config.get_path(cover_path)
            end
            return {
                type = 0,
                filepath = {
                    { name = "Cover", path = "parts/*.png", def = "a" }
                },
                source = {
                    { id = "cover", path = cover_path }
                }
            }
            "#,
        )
        .unwrap();

        // 存在しないファイルを選択 → 従来通りソート先頭候補へフォールバック。
        let files = BTreeMap::from([("Cover".to_string(), "parts/missing.png".to_string())]);
        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &files).unwrap();

        assert_eq!(
            loaded.value["source"][0]["path"].as_str().and_then(|path| {
                std::path::Path::new(path).file_name().and_then(|name| name.to_str())
            }),
            Some("a.png")
        );
    }

    #[test]
    fn lua_skin_dofile_resolves_get_path_joined_with_forward_slash() {
        // Regression: `skin_config.get_path` returns an absolute path and skins
        // build the dofile target by concatenating `"/sub.lua"`. On Windows the
        // skin root must not be a `\\?\` verbatim path, or the mixed-separator
        // path fails to canonicalize and the dofile is silently lost.
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts/frame")).unwrap();
        fs::write(
            root.join("parts/frame/mod.lua"),
            r#"return { destination = { { id = "x", dst = {{ x = 1, y = 2, w = 3, h = 4 }} } } }"#,
        )
        .unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            if skin_config then
                local dir = skin_config.get_path("parts/*")
                local sub = dofile(dir .. "/mod.lua")
                return { type = 0, destination = sub.destination }
            else
                return { type = 0 }
            end
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("play7.luaskin"),
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(loaded.document.destination.len(), 1);
    }

    #[test]
    fn lua_skin_stops_infinite_loop() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("play7.luaskin"), "while true do end").unwrap();

        let err = load_lua_skin(
            &root.join("play7.luaskin"),
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("instruction limit"));
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
    }
}
