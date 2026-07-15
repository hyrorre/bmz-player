use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use bmz_skin_document::SkinDocument;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;

mod lr2;
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
    pub files: BTreeMap<String, String>,
    pub dependencies: SkinLoadDependencies,
}

#[derive(Debug, Clone)]
pub struct LoadedLuaSkinValue {
    pub value: JsonValue,
    pub warnings: Vec<SkinLoadWarning>,
    pub files: BTreeMap<String, String>,
    pub dependencies: SkinLoadDependencies,
    pub internal_enabled_options: Vec<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkinLoadDependencies {
    pub number_values: BTreeMap<i32, i32>,
    pub text_values: BTreeMap<i32, String>,
    pub option_values: BTreeMap<i32, bool>,
    pub files: BTreeSet<String>,
    pub loaded_files: BTreeMap<PathBuf, SkinLoadedFileDependency>,
    /// Read-only virtual files observed through Lua `io.open` / `io.lines`.
    ///
    /// `None` records that no virtual file was present for the requested path,
    /// while `Some` contains the exact contents supplied for that load. Keeping
    /// the distinction lets a document cache invalidate both content changes
    /// and virtual-file additions/removals without granting Lua filesystem
    /// access outside the skin root.
    pub virtual_io_files: BTreeMap<String, Option<String>>,
    pub opaque: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinLoadedFileDependency {
    pub modified: Option<SystemTime>,
    pub len: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuaLoadRuntimeState {
    pub number_values: BTreeMap<i32, i32>,
    pub text_values: BTreeMap<i32, String>,
    pub option_values: BTreeMap<i32, bool>,
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
    load_lua_skin_with_runtime_state(path, options, files, &LuaLoadRuntimeState::default())
}

pub fn load_lua_skin_with_runtime_state(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<LoadedSkinDocument> {
    load_lua_skin_with_runtime_state_and_virtual_io_files(
        path,
        options,
        files,
        runtime_state,
        &BTreeMap::new(),
    )
}

/// Loads a Lua skin with deterministic runtime values and an in-memory,
/// read-only filesystem for compatibility configuration.
///
/// Virtual file keys use skin-style relative paths. Invalid paths, including
/// absolute paths and parent traversal, are rejected before Lua executes.
pub fn load_lua_skin_with_runtime_state_and_virtual_io_files(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<LoadedSkinDocument> {
    let loaded = load_lua_skin_value_with_runtime_state_and_virtual_io_files(
        path,
        options,
        files,
        runtime_state,
        virtual_io_files,
    )?;
    let value = normalize_lua_skin_document(loaded.value);
    let mut document: SkinDocument = serde_path_to_error::deserialize(value)
        .with_context(|| format!("failed to parse lua skin as document: {}", path.display()))?;
    document.internal_enabled_options = loaded.internal_enabled_options;
    Ok(LoadedSkinDocument {
        document,
        warnings: loaded.warnings,
        files: loaded.files,
        dependencies: loaded.dependencies,
    })
}

pub fn load_lr2_csv_skin(
    path: &Path,
    _kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedSkinDocument> {
    let loaded = lr2::load_lr2_csv_skin_value(path, options, files)?;
    let value = bmz_skin_document::normalize_lua_json_skin_integer_numbers(loaded.value);
    let mut document: SkinDocument = serde_path_to_error::deserialize(value)
        .with_context(|| format!("failed to parse lr2 csv skin as document: {}", path.display()))?;
    document.internal_enabled_options = loaded.internal_enabled_options;
    Ok(LoadedSkinDocument {
        document,
        warnings: loaded.warnings,
        files: BTreeMap::new(),
        dependencies: loaded.dependencies,
    })
}

pub fn load_lr2_csv_skin_dependency_option_values(
    path: &Path,
    options: &BTreeMap<String, String>,
    option_ids: impl IntoIterator<Item = i32>,
) -> Result<BTreeMap<i32, bool>> {
    lr2::load_lr2_csv_skin_dependency_option_values(path, options, option_ids)
}

pub fn load_lua_skin_value(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    load_lua_skin_value_with_runtime_state(path, options, files, &LuaLoadRuntimeState::default())
}

pub fn load_lua_skin_value_with_runtime_state(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<LoadedLuaSkinValue> {
    load_lua_skin_value_with_runtime_state_and_virtual_io_files(
        path,
        options,
        files,
        runtime_state,
        &BTreeMap::new(),
    )
}

pub fn load_lua_skin_value_with_runtime_state_and_virtual_io_files(
    path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    lua::load_lua_skin_value(path, options, files, runtime_state, virtual_io_files)
}

pub fn load_lua_skin_header_value(path: &Path) -> Result<LoadedLuaSkinValue> {
    let mut loaded = lua::load_lua_skin_header_value(path)?;
    loaded.value = normalize_lua_skin_document(loaded.value);
    Ok(loaded)
}

fn normalize_lua_skin_document(value: JsonValue) -> JsonValue {
    let value = bmz_skin_document::normalize_lua_json_skin_integer_numbers(value);
    let value = normalize_lua_skin_category_map(value);
    let value = normalize_lua_end_of_note_shadow_destinations(value);
    normalize_lua_skin_offset_map(value)
}

/// Rm-skin の `processHeader()` は `category = { property = {...}, filepath = {...} }` 形式。
/// beatoraja / BMZ の `SkinDocument` は `category: [{ name, item }]` を期待する。
fn normalize_lua_skin_category_map(value: JsonValue) -> JsonValue {
    let JsonValue::Object(mut map) = value else {
        return value;
    };
    if let Some(JsonValue::Object(category_map)) = map.get("category").cloned() {
        let entries: Vec<JsonValue> = category_map.into_values().collect();
        if !entries.is_empty() && entries.iter().all(|entry| matches!(entry, JsonValue::Object(_)))
        {
            map.insert("category".to_string(), JsonValue::Array(entries));
        }
    }
    JsonValue::Object(map)
}

/// `skin_config.offset` is keyed by display name for Lua access, while beatoraja JSON uses an
/// array of offset definitions.
fn normalize_lua_skin_offset_map(value: JsonValue) -> JsonValue {
    normalize_lua_skin_offset_map_for_key(None, value)
}

fn normalize_lua_end_of_note_shadow_destinations(value: JsonValue) -> JsonValue {
    let JsonValue::Object(mut map) = value else {
        return value;
    };
    let Some(JsonValue::Array(mut destinations)) = map.remove("destination") else {
        return JsonValue::Object(map);
    };

    let end_of_note_destinations = destinations
        .iter()
        .filter_map(|destination| {
            let JsonValue::Object(destination) = destination else {
                return None;
            };
            let timer = destination.get("timer").and_then(JsonValue::as_i64)?;
            if !matches!(timer, 143 | 144) {
                return None;
            }
            Some((
                destination.get("id").and_then(JsonValue::as_str)?.to_string(),
                single_dst_geometry(destination)?,
                timer,
            ))
        })
        .collect::<Vec<_>>();

    if end_of_note_destinations.is_empty() {
        map.insert("destination".to_string(), JsonValue::Array(destinations));
        return JsonValue::Object(map);
    }

    for destination in &mut destinations {
        let JsonValue::Object(destination) = destination else {
            continue;
        };
        if destination.contains_key("timer")
            || destination
                .get("draw")
                .and_then(JsonValue::as_str)
                .is_some_and(json_draw_is_restrictive)
            || destination.get("op").is_some_and(json_array_has_entries)
        {
            continue;
        }
        let Some(id) = destination.get("id").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(geometry) = single_dst_geometry(destination) else {
            continue;
        };
        let Some((_, _, timer)) = end_of_note_destinations
            .iter()
            .find(|(end_id, end_geometry, _)| end_id == id && *end_geometry == geometry)
        else {
            continue;
        };
        destination
            .insert("timer".to_string(), JsonValue::Number(serde_json::Number::from(*timer)));
    }

    map.insert("destination".to_string(), JsonValue::Array(destinations));
    JsonValue::Object(map)
}

fn single_dst_geometry(destination: &JsonMap<String, JsonValue>) -> Option<(i64, i64, i64, i64)> {
    let dst = destination.get("dst")?.as_array()?;
    if dst.len() != 1 {
        return None;
    }
    let JsonValue::Object(frame) = &dst[0] else {
        return None;
    };
    Some((
        frame.get("x").and_then(JsonValue::as_i64).unwrap_or(0),
        frame.get("y").and_then(JsonValue::as_i64).unwrap_or(0),
        frame.get("w").and_then(JsonValue::as_i64).unwrap_or(0),
        frame.get("h").and_then(JsonValue::as_i64).unwrap_or(0),
    ))
}

fn json_array_has_entries(value: &JsonValue) -> bool {
    value.as_array().is_some_and(|entries| !entries.is_empty())
}

fn json_draw_is_restrictive(draw: &str) -> bool {
    let draw = draw.trim();
    !draw.is_empty() && draw != "number(0) >= 0"
}

fn normalize_lua_skin_offset_map_for_key(key: Option<&str>, value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| normalize_lua_skin_offset_map_for_key(None, value))
                .collect(),
        ),
        JsonValue::Object(map) => {
            let map = map
                .into_iter()
                .map(|(key, value)| {
                    let value = normalize_lua_skin_offset_map_for_key(Some(&key), value);
                    (key, value)
                })
                .collect::<JsonMap<_, _>>();
            if matches!(key, Some("offset")) {
                if map.values().all(|entry| matches!(entry, JsonValue::Object(_))) {
                    JsonValue::Array(map.into_values().collect())
                } else {
                    JsonValue::Array(vec![JsonValue::Object(map)])
                }
            } else {
                JsonValue::Object(map)
            }
        }
        value => value,
    }
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
        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.draw, "option(1)");
    }

    #[test]
    fn lua_skin_runtime_option_is_available_during_load() {
        let root = unique_test_dir("bmz-skin-lua-runtime-option");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            local y = 18
            if main_state.option(1008) then
                y = 45
            end
            return {
                type = 7,
                destination = {
                    { id = "panel", dst = {{ x = 1, y = y, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin_with_runtime_state(
            &root.join("result.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                number_values: BTreeMap::new(),
                text_values: BTreeMap::new(),
                option_values: BTreeMap::from([(1008, true)]),
            },
        )
        .unwrap();

        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        let bmz_skin_document::SkinDstEntry::Frame(frame) = &destination.dst[0] else {
            panic!("destination frame should be static");
        };
        assert_eq!(frame.y, Some(45));
    }

    #[test]
    fn lua_skin_infers_option_and_number_draw_conditions() {
        let root = unique_test_dir("bmz-skin-lua-option-number-draw");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play.luaskin"),
            r#"
            local main_state = require("main_state")
            local function nonzero(ref)
                return main_state.number(ref) ~= 0
            end
            return {
                type = 0,
                destination = {
                    { id = "fast", draw = function()
                        return main_state.option(1242) and nonzero(525)
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                    { id = "ms", draw = function()
                        return not main_state.option(241) and nonzero(525)
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                }
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("play.luaskin"),
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();

        let bmz_skin_document::DestinationListEntry::Single(fast) = &loaded.document.destination[0]
        else {
            panic!("expected fast destination");
        };
        let bmz_skin_document::DestinationListEntry::Single(ms) = &loaded.document.destination[1]
        else {
            panic!("expected ms destination");
        };
        assert_eq!(fast.draw, "option(1242) && number(525) != 0");
        assert_eq!(ms.draw, "!option(241) && number(525) != 0");
    }

    #[test]
    fn lua_skin_records_required_module_skin_config_option_dependency() {
        let root = unique_test_dir("bmz-skin-lua-required-option-dependency");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play.luaskin"),
            "local parts = require('parts')\nreturn parts.build()",
        )
        .unwrap();
        fs::write(
            root.join("parts.lua"),
            r#"
            local M = {}
            function M.build()
                local branch = 910
                if skin_config and skin_config.option then
                    branch = skin_config.option["Branch"] or 910
                end
                return {
                    type = 0,
                    property = {
                        { name = "Branch", item = {{ name = "Off", op = 910 }, { name = "On", op = 911 }}, def = "Off" },
                    },
                    source = {
                        { id = "bg", path = branch == 911 and "on.png" or "off.png" },
                    },
                }
            end
            return M
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin_with_runtime_state(
            &root.join("play.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
        )
        .unwrap();

        assert_eq!(loaded.document.source[0].path, "off.png");
        assert!(loaded.dependencies.option_values.contains_key(&910));
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
    fn lua_skin_config_get_path_applies_directory_selection_to_child_wildcard() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("Theme/a/_lua")).unwrap();
        fs::create_dir_all(root.join("Theme/z/_lua")).unwrap();
        fs::write(
            root.join("Theme/a/_lua/frame.lua"),
            r#"return { source = { { id = "frame", path = "Theme/a/frame.png" } } }"#,
        )
        .unwrap();
        fs::write(
            root.join("Theme/z/_lua/frame.lua"),
            r#"return { source = { { id = "frame", path = "Theme/z/frame.png" } } }"#,
        )
        .unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            if skin_config then
                local parts = dofile(skin_config.get_path("Theme/*/_lua") .. "/frame.lua")
                return {
                    type = 7,
                    filepath = {
                        { name = "Theme", path = "Theme/*", def = "a" }
                    },
                    source = parts.source
                }
            end
            return {
                type = 7,
                filepath = {
                    { name = "Theme", path = "Theme/*", def = "a" }
                }
            }
            "#,
        )
        .unwrap();

        let files = BTreeMap::from([("Theme".to_string(), "Theme/z".to_string())]);
        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &files).unwrap();

        assert_eq!(loaded.value["source"][0]["path"], "Theme/z/frame.png");
    }

    #[test]
    fn lua_skin_config_offset_exposes_zero_defaults_by_name() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local alpha = 255
            if skin_config then
                alpha = skin_config.offset["Panel alpha"].a
            end
            return {
                type = 0,
                offset = {
                    { name = "Panel alpha", id = 42, a = true }
                },
                destination = {
                    { id = -110, dst = {{ x = 1, y = 2, w = 3, h = 4, a = alpha }} }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["destination"][0]["dst"][0]["a"], 0);
    }

    #[test]
    fn lua_skin_main_state_offset_exposes_zero_defaults_by_id() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            if skin_config then
                local offset = main_state.offset(4)
                return {
                    type = 0,
                    destination = {
                        { id = -110, dst = {{
                            x = offset.x,
                            y = 1080 + offset.y,
                            w = offset.w,
                            h = offset.h,
                            r = offset.r,
                            a = offset.a
                        }} }
                    }
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["destination"][0]["dst"][0]["x"], 0);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["y"], 1080);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["w"], 0);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["h"], 0);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["r"], 0);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["a"], 0);
    }

    #[test]
    fn lua_skin_runtime_stub_treats_normal_play_as_autoplay_off() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            if skin_config then
                local graph = {}
                if main_state.option(32) then
                    table.insert(graph, { id = "score", src = 1, x = 0, y = 0, w = 1, h = 10, type = 110 })
                end
                return {
                    type = 0,
                    graph = graph,
                    image = main_state.option(33) and {{ id = "autoplay", src = 1, x = 0, y = 0, w = 1, h = 1 }} or {}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["graph"][0]["id"], "score");
        assert_eq!(loaded.value["image"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn lua_skin_os_clock_after_draw_becomes_elapsed_timer_condition() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local function after(ms)
                local start_time = nil
                return function()
                    start_time = start_time or os.clock()
                    return (os.clock() - start_time) * 1000 >= ms
                end
            end
            if skin_config then
                return {
                    type = 0,
                    image = {{ id = "keyflash", src = 1, x = 0, y = 0, w = 1, h = 1 }},
                    destination = {{
                        id = "keyflash",
                        timer = 101,
                        draw = after(1800),
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    }}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["destination"][0]["draw"], "timer(0) >= 1800");
    }

    #[test]
    fn lua_skin_os_clock_after_and_option_draw_becomes_elapsed_timer_and_option_condition() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            local function after_and_op(ms, ...)
                local start_time = nil
                local ops = {...}
                return function()
                    start_time = start_time or os.clock()
                    if (os.clock() - start_time) * 1000 < ms then
                        return false
                    end
                    for _, op in ipairs(ops) do
                        if not main_state.option(op) then
                            return false
                        end
                    end
                    return true
                end
            end
            if skin_config then
                return {
                    type = 0,
                    value = {{ id = "lanecover-value", src = 1, x = 0, y = 0, w = 10, h = 1, divx = 10, digit = 3, ref = 14 }},
                    destination = {{
                        id = "lanecover-value",
                        draw = after_and_op(1800, 270, 177),
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    }}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["destination"][0]["draw"],
            "timer(0) >= 1800 and option(270) and option(177)"
        );
    }

    #[test]
    fn lua_skin_load_time_table_level_text_ref_is_preserved() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            local table_text = main_state.text(1002)
            if skin_config then
                return {
                    type = 0,
                    text = {{
                        id = "tableLevel",
                        font = 3,
                        size = 18,
                        value = function()
                            return table_text
                        end
                    }}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["ref"], 1002);
        assert!(loaded.value["text"][0].get("constantText").is_none());
    }

    #[test]
    fn lua_skin_mz_select_result_title_becomes_runtime_expr() {
        let root = unique_test_dir("bmz-skin-lua-mz-select-result-title");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            local title = main_state.text(1002) .. " " .. main_state.text(1001)
            if title then title = title .. " " end
            title = title .. main_state.text(12)
            return {
                type = 7,
                text = {{ id = "title", font = 0, size = 24, constantText = title }},
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["text"][0]["value_expr"],
            bmz_skin_document::SKIN_EXPR_RESULT_TABLE_TITLE
        );
        assert!(loaded.value["text"][0].get("constantText").is_none());
    }

    #[test]
    fn lua_skin_event_util_module_loads_custom_event_helpers() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local event_util = require("event_util")
            local count = 0
            local action = event_util.event_observe_turn_true(
                function() return true end,
                function() count = count + 1 end
            )
            action()
            action()
            return {
                type = 0,
                text = {
                    { id = "event-count", font = 1, size = 16, constantText = tostring(count) }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "1");
    }

    #[test]
    fn lua_skin_os_stub_supports_date_and_clock() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local t = os.date("*t", 0)
            local elapsed = os.clock()
            return {
                type = 0,
                text = {
                    {
                        id = "timestamp",
                        font = 1,
                        size = 16,
                        constantText = os.date("%Y-%m-%d %H:%M:%S", 0) .. "|" .. t.year .. "|" .. tostring(elapsed >= 0)
                    }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "1970-01-01 00:00:00|1970|true");
    }

    #[test]
    fn lua_skin_io_stub_reads_skin_alias_from_renamed_root_and_ignores_writes() {
        let parent = unique_test_dir("bmz-skin-lua");
        let root = parent.join("mz-select");
        fs::create_dir_all(root.join("customize/advanced")).unwrap();
        fs::write(root.join("customize/advanced/enable.txt"), "parts.lua\n").unwrap();
        fs::write(
            root.join("customize/advanced/parts.lua"),
            r#"
            return {
                load = function()
                    return "loaded"
                end
            }
            "#,
        )
        .unwrap();
        fs::write(
            root.join("music_select.luaskin"),
            r#"
            local f = io.open("skin/m_select/customize/advanced/enable.txt", "r")
            local out = io.open("skin/m_select/customize/advanced/load_log.txt", "w")
            local count = 0
            for line in f:lines() do
                count = count + 1
                out:write(line)
                local parts = dofile("skin/m_select/customize/advanced/" .. line)
                if parts.load() == "loaded" then
                    count = count + 1
                end
            end
            for _ in io.lines("skin/m_select/customize/advanced/enable.txt") do
                count = count + 1
            end
            io.close(f)
            out:close()
            return {
                type = 0,
                text = {
                    { id = "line-count", font = 1, size = 16, constantText = tostring(count) }
                }
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin_value(
            &root.join("music_select.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "3");
        assert!(!root.join("customize/advanced/load_log.txt").exists());
    }

    #[test]
    fn lua_skin_io_read_all_lines_and_close_share_a_read_only_cursor() {
        let root = unique_test_dir("bmz-skin-lua-io-read");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("config.txt"), "alpha\r\nbeta\n").unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local f = io.open("config.txt", "r")
            local all = f:read("*a")
            local eof = f:read("*all")
            f:close()
            local read_after_close = pcall(function() f:read("*a") end)
            local lines = {}
            for line in io.lines("config.txt") do
                table.insert(lines, line)
            end
            return {
                type = 7,
                text = {{
                    id = "io",
                    font = 1,
                    size = 16,
                    constantText = all .. "|" .. eof .. "|" .. tostring(read_after_close) .. "|" .. table.concat(lines, ",")
                }}
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "alpha\r\nbeta\n||false|alpha,beta");
        assert!(
            loaded
                .dependencies
                .loaded_files
                .contains_key(&root.join("config.txt").canonicalize().unwrap())
        );
    }

    #[test]
    fn lua_skin_virtual_io_loads_wmii_style_player_config_without_host_access() {
        let root = unique_test_dir("bmz-skin-lua-virtual-io");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local sys = io.open("config_sys.json", "r")
            local player = sys:read("*a"):match('"playername"%s*:%s*"([^"]+)"')
            sys:close()
            local path = "player/" .. player .. "/config_player.json"
            local config = io.open(path, "r")
            local contents = config:read("*all")
            config:close()
            return {
                type = 7,
                text = {{ id = "config", font = 1, size = 16, constantText = path .. "|" .. contents }}
            }
            "#,
        )
        .unwrap();
        let virtual_files = BTreeMap::from([
            ("config_sys.json".to_string(), r#"{"playername":"bmz"}"#.to_string()),
            (
                "player\\bmz\\config_player.json".to_string(),
                r#"{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#.to_string(),
            ),
        ]);

        let loaded = load_lua_skin_value_with_runtime_state_and_virtual_io_files(
            &root.join("result.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &virtual_files,
        )
        .unwrap();

        assert_eq!(
            loaded.value["text"][0]["constantText"],
            r#"player/bmz/config_player.json|{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#
        );
        assert_eq!(
            loaded.dependencies.virtual_io_files,
            BTreeMap::from([
                ("config_sys.json".to_string(), Some(r#"{"playername":"bmz"}"#.to_string())),
                (
                    "player/bmz/config_player.json".to_string(),
                    Some(r#"{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#.to_string())
                ),
            ])
        );
        assert!(!loaded.dependencies.opaque);
    }

    #[test]
    fn lua_skin_virtual_io_dependency_snapshot_changes_with_contents() {
        let root = unique_test_dir("bmz-skin-lua-virtual-io-dependency");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local f = io.open("config_sys.json", "r")
            local contents = f:read("*a")
            f:close()
            return { type = 7, text = {{ id = "config", font = 1, size = 16, constantText = contents }} }
            "#,
        )
        .unwrap();
        let load = |contents: &str| {
            load_lua_skin_value_with_runtime_state_and_virtual_io_files(
                &root.join("result.luaskin"),
                &BTreeMap::new(),
                &BTreeMap::new(),
                &LuaLoadRuntimeState::default(),
                &BTreeMap::from([("config_sys.json".to_string(), contents.to_string())]),
            )
            .unwrap()
        };

        let first = load("first");
        let second = load("second");
        assert_ne!(first.dependencies.virtual_io_files, second.dependencies.virtual_io_files);
        assert_eq!(
            second.dependencies.virtual_io_files["config_sys.json"],
            Some("second".to_string())
        );
    }

    #[test]
    fn lua_skin_io_rejects_traversal_and_oversized_virtual_files() {
        let parent = unique_test_dir("bmz-skin-lua-io-security");
        let root = parent.join("skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(parent.join("secret.txt"), "secret").unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local paths = { "../secret.txt", "C:\\secret.txt", "//server/share/secret.txt" }
            local opened = 0
            for _, path in ipairs(paths) do
                if io.open(path, "r") then opened = opened + 1 end
            end
            return { type = 7, text = {{ id = "opened", font = 1, size = 16, constantText = tostring(opened) }} }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();
        assert_eq!(loaded.value["text"][0]["constantText"], "0");

        let error = load_lua_skin_value_with_runtime_state_and_virtual_io_files(
            &root.join("result.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &BTreeMap::from([("config_sys.json".to_string(), "x".repeat(8 * 1024 * 1024 + 1))]),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("exceeds 8388608 byte limit"));
    }

    #[test]
    fn lua_skin_main_state_stubs_audio_volume_helpers() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            local ok = main_state.audio_play("sound.wav", main_state.volume_sys())
            return {
                type = 0,
                text = {
                    { id = "volume", font = 1, size = 16, constantText = tostring(main_state.volume_key() + main_state.volume_bg()) .. "|" .. tostring(ok) }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "2.0|true");
    }

    #[test]
    fn lua_skin_luajava_stub_loads_legacy_sound_helper() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local luajava = require("luajava")
            local gdx = luajava.bindClass("com.badlogic.gdx.Gdx")
            pcall(function() gdx.app:getApplicationListener():getAudioProcessor():play("x", 1) end)
            return {
                type = 0,
                text = {
                    { id = "loaded", font = 1, size = 16, constantText = "ok" }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "ok");
    }

    #[test]
    fn lua_skin_luajava_input_stubs_are_neutral_during_load() {
        let root = unique_test_dir("bmz-skin-lua-luajava-input");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local luajava = require("luajava")
            local Gdx = luajava.bindClass("com.badlogic.gdx.Gdx")
            local Controllers = luajava.bindClass("com.badlogic.gdx.controllers.Controllers")
            local Expand_op = 2
            local function input_handler()
                if Gdx.input:isKeyPressed(1) or Controllers:getControllers().size > 0 then
                    Expand_op = 1
                end
            end
            input_handler()
            return {
                type = 7,
                text = {{ id = "panel", font = 1, size = 16, constantText = tostring(Expand_op) }}
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["text"][0]["constantText"], "2");
    }

    #[test]
    fn lua_skin_non_finite_numbers_warn_and_convert_to_zero() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            return {
                type = 0,
                destination = {
                    { id = -110, dst = {{ x = 0 / 0, y = 1 / 0, w = 1, h = 1 }} }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.value["destination"][0]["dst"][0]["x"], 0);
        assert_eq!(loaded.value["destination"][0]["dst"][0]["y"], 0);
        assert!(
            loaded
                .warnings
                .iter()
                .any(|warning| warning.message.contains("non-finite lua number converted to 0"))
        );
    }

    #[test]
    fn lua_skin_m_select_result_graph_heights_become_runtime_exprs() {
        let root = unique_test_dir("bmz-skin-lua-m-select-result");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local destinations = {}
            for i = 1, 39 do
                destinations[i] = { id = "dummy", dst = {{ x = 0, y = 0, w = 1, h = 1 }} }
            end
            for i = 40, 51 do
                destinations[i] = {
                    id = "graph",
                    dst = {
                        { time = 0, x = 0, y = 0, w = 1, h = 0 },
                        { time = 500, h = 0 },
                        { time = 1000, h = 0 / 0 },
                    },
                }
            end
            return { type = 7, destination = destinations }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(
            loaded
                .warnings
                .iter()
                .all(|warning| !warning.message.contains("non-finite lua number converted to 0"))
        );
        assert_eq!(
            loaded.value["destination"][39]["dst"][2]["h_expr"],
            "bmz:fast_slow_breakdown_height(422)"
        );
        assert_eq!(
            loaded.value["destination"][50]["dst"][2]["h_expr"],
            "bmz:fast_slow_breakdown_height(421)"
        );
    }

    #[test]
    fn lua_skin_value_functions_fall_back_to_load_time_constants() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            return {
                type = 0,
                value = {
                    { id = "num", src = 1, x = 0, y = 0, w = 10, h = 10, value = function() return 42 end }
                },
                graph = {
                    { id = "graph", src = 1, x = 0, y = 0, w = 10, h = 10, value = function() return 0.25 end }
                },
                text = {
                    { id = "text", font = 1, size = 16, value = function() return "ready" end }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.value["value"][0]["value_expr"], "42");
        assert_eq!(loaded.value["graph"][0]["value_expr"], "0.25");
        assert_eq!(loaded.value["text"][0]["constantText"], "ready");
    }

    #[test]
    fn lua_skin_volume_value_functions_map_to_number_refs() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                value = {
                    { id = "master", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_sys() * 100 end },
                    { id = "key", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_key() * 100 end },
                    { id = "bgm", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_bg() * 100 end },
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.value["value"][0]["ref"], 57);
        assert_eq!(loaded.value["value"][1]["ref"], 58);
        assert_eq!(loaded.value["value"][2]["ref"], 59);
    }

    #[test]
    fn lua_skin_main_state_version_text_is_available_during_load() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            local version = main_state.text(1010)
            version = string.sub(version, (string.find(version, " ") + 1))
            return {
                type = 0,
                text = {
                    { id = "version", constantText = version },
                },
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.value["text"][0]["constantText"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn lua_skin_main_state_player_name_is_available_during_load() {
        let root = unique_test_dir("bmz-skin-lua-player-name");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                text = {
                    { id = "player", constantText = main_state.text(2) },
                },
            }
            "#,
        )
        .unwrap();
        let runtime_state = LuaLoadRuntimeState {
            text_values: BTreeMap::from([(2, "Player One".to_string())]),
            ..LuaLoadRuntimeState::default()
        };

        let loaded = load_lua_skin_with_runtime_state(
            &root.join("select.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .unwrap();

        assert_eq!(loaded.document.text[0].constant_text, "Player One");
        assert_eq!(loaded.dependencies.text_values.get(&2).map(String::as_str), Some("Player One"));
    }

    #[test]
    fn lua_skin_main_state_current_date_numbers_are_available_during_load() {
        let root = unique_test_dir("bmz-skin-lua-date");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 5,
                text = {
                    { id = "date", constantText = main_state.number(21) .. "/" .. main_state.number(22) .. "/" .. main_state.number(23) },
                },
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();
        let date = loaded.value["text"][0]["constantText"].as_str().unwrap_or_default();
        let current_year = unix_epoch_year_for_test();

        assert!(loaded.warnings.is_empty());
        assert!(date.starts_with(&format!("{current_year}/")), "unexpected date: {date}");
    }

    fn unix_epoch_year_for_test() -> i32 {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
            .unwrap_or_default();
        let days = seconds.div_euclid(86_400);
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let month = mp + if mp < 10 { 3 } else { -9 };
        (y + if month <= 2 { 1 } else { 0 }) as i32
    }

    #[test]
    fn lua_skin_nil_integer_keys_do_not_warn_as_mixed_table() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local skin = { type = 0, image = {} }
            skin[1] = nil
            return skin
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(
            loaded.warnings.iter().all(|warning| !warning.message.contains("mixed lua table")),
            "warnings: {:?}",
            loaded.warnings
        );
    }

    #[test]
    fn lua_skin_header_pass_mixed_table_warning_is_suppressed() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            if skin_config then
                return { type = 0, image = {} }
            end
            return {
                type = 0,
                { image = {} },
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(
            loaded.warnings.iter().all(|warning| !warning.message.contains("mixed lua table")),
            "warnings: {:?}",
            loaded.warnings
        );
    }

    #[test]
    fn lua_skin_preserves_constant_act_and_skips_loader_callback_fields() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            return {
                type = 0,
                image = {
                    { id = "button", src = "src", x = 0, y = 0, w = 10, h = 10, act = 15 },
                    { id = "sort", src = "src", x = 0, y = 0, w = 10, h = 10, act = function() return 12 end },
                    { id = "callback", src = "src", x = 0, y = 0, w = 10, h = 10, act = function() return true end }
                },
                customTimers = {
                    { id = 9001, timer = function() return 0 end }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(
            loaded.warnings[0].message,
            "skipping unsupported custom timer function id 9001 at $.customTimers[1].timer"
        );
        assert_eq!(loaded.value["image"][0]["act"], serde_json::json!(15));
        assert_eq!(loaded.value["image"][1]["act"], serde_json::json!(12));
        assert!(loaded.value["image"][2].get("act").is_none());
        assert!(loaded.value["customTimers"][0].get("timer").is_none());
    }

    #[test]
    fn lua_skin_does_not_execute_mutating_act_during_conversion() {
        let root = unique_test_dir("bmz-skin-lua-mutating-act");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            Panel = 2
            return {
                type = 7,
                image = {
                    {
                        id = "switch",
                        src = "src",
                        x = 0, y = 0, w = 10, h = 10,
                        act = function() Panel = 1 end,
                    },
                    { id = "graph", src = "src", x = 0, y = 0, w = 10, h = 10 },
                    { id = "ir", src = "src", x = 0, y = 0, w = 10, h = 10 },
                },
                destination = {
                    {
                        id = "graph",
                        draw = function() return Panel == 2 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                    {
                        id = "ir",
                        draw = function() return Panel == 1 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert!(loaded.value["image"][0].get("act").is_none());
        assert_eq!(loaded.value["destination"][0]["draw"], "number(0) >= 0");
        assert_eq!(loaded.value["destination"][1]["draw"], "number(0) < 0");
    }

    #[test]
    fn lua_skin_maps_result_panel_act_without_mutating_default() {
        let root = unique_test_dir("bmz-skin-lua-result-panel-act");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            Expand_op = 2
            return {
                type = 7,
                image = {
                    {
                        id = "BtnGraphData", src = "src", x = 0, y = 0, w = 10, h = 10,
                        act = function() Expand_op = 2 end,
                    },
                    {
                        id = "BtnIrData", src = "src", x = 0, y = 0, w = 10, h = 10,
                        act = function() Expand_op = 1 end,
                    },
                },
                destination = {
                    {
                        id = "BtnGraphData",
                        draw = function() return Expand_op == 1 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                    {
                        id = "BtnIrData",
                        draw = function() return Expand_op == 2 end,
                        dst = {{ x = 10, y = 0, w = 10, h = 10 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["image"][0]["act"],
            serde_json::json!(bmz_skin_document::SKIN_EVENT_RESULT_PANEL_GRAPH)
        );
        assert_eq!(
            loaded.value["image"][1]["act"],
            serde_json::json!(bmz_skin_document::SKIN_EVENT_RESULT_PANEL_IR)
        );
        assert_eq!(loaded.value["resultPanelDefault"], serde_json::json!(2));
        assert_eq!(loaded.value["destination"][0]["draw"], "result_panel(1)");
        assert_eq!(loaded.value["destination"][1]["draw"], "result_panel(2)");
    }

    #[test]
    fn lua_skin_infers_fixed_delay_custom_timer() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                customTimers = {
                    { id = 11900, timer = function()
                        local off = main_state.timer_off_value
                        local source = main_state.timer(143)
                        if source == off then return off end
                        local start = source + 1000000
                        if main_state.time() < start then return off end
                        return start
                    end },
                    { id = 11901, timer = function() return main_state.timer(150) end },
                    { id = 11902, timer = function() return main_state.timer(150) + 1 end }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["fixedDelayTimer"],
            serde_json::json!([
                { "id": 11900, "sourceTimer": 143, "delayMs": 1000 },
                { "id": 11901, "sourceTimer": 150, "delayMs": 0 }
            ])
        );
        assert!(loaded.value["customTimers"][0].get("timer").is_none());
        assert!(loaded.value["customTimers"][1].get("timer").is_none());
        assert!(loaded.value["customTimers"][2].get("timer").is_none());
        assert!(loaded.warnings.iter().any(|warning| {
            warning.message
                == "skipping unsupported custom timer function id 11902 at $.customTimers[3].timer"
        }));
    }

    #[test]
    fn lua_skin_warns_when_timer_observe_callback_needs_runtime_lua() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local timer_util = require("timer_util")
            local menu_open = false
            local menu_timer = timer_util.timer_observe_boolean(function()
                return menu_open
            end)
            return {
                type = 0,
                destination = {
                    { id = "menu", dst = { { timer = menu_timer } } }
                }
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .unwrap();

        assert_eq!(
            loaded.value["dynamicTimer"],
            serde_json::json!([{ "id": 9000, "observe": "number(0) < 0" }])
        );
        assert!(loaded.warnings.iter().any(|warning| {
            warning.message
                == "timer_util.timer_observe_boolean callback for generated timer 9000 was fixed to its load-time value; runtime Lua state changes are unsupported"
        }));
    }

    #[test]
    fn lua_skin_config_get_path_prefers_filepath_default() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/aaa.png"), []).unwrap();
        fs::write(root.join("parts/default.png"), []).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local image_path = "parts/*.png"
            if skin_config then
                image_path = skin_config.get_path(image_path)
            end
            return {
                type = 0,
                filepath = {
                    { name = "Notes", path = "parts/*.png", def = "default" }
                },
                source = {
                    { id = "notes", path = image_path }
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

        // 存在しないファイルを選択 → beatoraja と同じく列挙候補へフォールバック。
        let files = BTreeMap::from([("Cover".to_string(), "parts/missing.png".to_string())]);
        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &files).unwrap();

        let filename = loaded.value["source"][0]["path"]
            .as_str()
            .and_then(|path| std::path::Path::new(path).file_name().and_then(|name| name.to_str()));
        assert!(matches!(filename, Some("a.png" | "z.png")));
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
    fn lua_skin_timer_util_supports_observe_boolean_for_dofile_parts() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(root.join("parts/frame")).unwrap();
        fs::write(
            root.join("parts/frame/mod.lua"),
            r#"
            local timer_util = require("timer_util")
            return {
                destination = {
                    {
                        id = "frame-panel",
                        timer = timer_util.timer_observe_boolean(function()
                            return true
                        end),
                        dst = { { x = 1, y = 2, w = 3, h = 4 } },
                    },
                },
            }
            "#,
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
        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.id, "frame-panel");
        assert_eq!(destination.timer, Some(bmz_skin_document::SKIN_DYNAMIC_TIMER_BASE));
        assert_eq!(loaded.document.dynamic_timers.len(), 1);
        assert_eq!(loaded.document.dynamic_timers[0].observe, "number(0) >= 0");
    }

    #[test]
    fn lua_skin_timer_observe_infers_is_gauge_iidx_global() {
        let root = unique_test_dir("bmz-skin-lua-iidx");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local timer_util = require("timer_util")
            return {
                type = 0,
                destination = {
                    {
                        id = "groove_frame",
                        timer = timer_util.timer_observe_boolean(function()
                            return not is_gauge_iidx
                        end),
                        dst = { { x = 0, y = 0, w = 1, h = 1 } },
                    },
                    {
                        id = "groove_frame_iidx",
                        timer = timer_util.timer_observe_boolean(function()
                            return is_gauge_iidx
                        end),
                        dst = { { x = 0, y = 0, w = 1, h = 1 } },
                    },
                },
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

        assert_eq!(loaded.document.dynamic_timers.len(), 2);
        assert_eq!(
            loaded.document.dynamic_timers[0].observe,
            "gauge_type() != 4 and gauge_type() != 5"
        );
        assert_eq!(
            loaded.document.dynamic_timers[1].observe,
            "gauge_type() == 4 or gauge_type() == 5"
        );
    }

    #[test]
    fn lua_skin_timer_observe_infers_starseeker_default_gauge_iidx_global_as_constant() {
        let root = unique_test_dir("bmz-skin-lua-iidx-gauge-default");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local timer_util = require("timer_util")
            return {
                type = 0,
                property = {
                    {
                        name = "グルーヴゲージ表示",
                        def = "default",
                        item = {
                            { name = "default", op = 930 },
                            { name = "gauge_off", op = 931 },
                            { name = "all_off", op = 932 },
                        },
                    },
                },
                destination = {
                    {
                        id = "groove_frame",
                        timer = timer_util.timer_observe_boolean(function()
                            return not is_gauge_iidx
                        end),
                        dst = { { x = 0, y = 0, w = 1, h = 1 } },
                    },
                    {
                        id = "groove_frame_iidx",
                        timer = timer_util.timer_observe_boolean(function()
                            return is_gauge_iidx
                        end),
                        dst = { { x = 0, y = 0, w = 1, h = 1 } },
                    },
                },
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

        assert_eq!(loaded.document.dynamic_timers.len(), 2);
        assert_eq!(loaded.document.dynamic_timers[0].observe, "number(0) >= 0");
        assert_eq!(loaded.document.dynamic_timers[1].observe, "number(0) < 0");
    }

    #[test]
    fn lua_skin_infers_gauge_type_class_predicate_covers_ids_6_7_8() {
        // 段位ゲージ用 skin が `gauge_type() >= 6` のような draw 条件を書いたとき、
        // probe は 6 / 7 / 8 すべてを検出して or 連結する必要がある。
        let root = unique_test_dir("bmz-skin-lua-class-gauge");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {
                    {
                        id = "class_gauge_overlay",
                        draw = function() return main_state.gauge_type() >= 6 end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
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

        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.draw, "gauge_type() == 6 or gauge_type() == 7 or gauge_type() == 8");
    }

    #[test]
    fn lua_skin_infers_or_draw_and_division_graph_value() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                graph = {
                    {
                        id = "ratio",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local fast = main_state.number(410)
                            local slow = main_state.number(411)
                            local total = fast + slow
                            if total == 0 then return 0 end
                            return fast / total
                        end,
                    },
                },
                destination = {
                    {
                        id = "panel",
                        draw = function()
                            return main_state.number(77) > 0 or main_state.number(150) > 0
                        end,
                        dst = {{ x = 1, y = 2, w = 3, h = 4 }},
                    },
                },
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

        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|warning| warning.message.as_str()).collect::<Vec<_>>()
        );
        assert_eq!(loaded.document.graph[0].value_expr, "(number(410))/(number(410)+number(411))");
        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.draw, "number(77) > 0 or number(150) > 0");
    }

    #[test]
    fn lua_skin_infers_option_weighted_graph_value() {
        let root = unique_test_dir("bmz-skin-lua-option-weighted");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                graph = {
                    {
                        id = "difficulty",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local rank
                            if main_state.option(180) then
                                rank = 1.7
                            elseif main_state.option(181) then
                                rank = 1.5
                            elseif main_state.option(182) then
                                rank = 1.3
                            end
                            if rank < 0 then rank = 0 end
                            return (main_state.number(350) / 25 + main_state.number(351) / 8.3) * rank * 1.5
                        end,
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("select.luaskin"),
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|warning| warning.message.as_str()).collect::<Vec<_>>()
        );
        let expr = &loaded.document.graph[0].value_expr;
        assert!(expr.contains("*option(180)*number(350)"));
        assert!(expr.contains("*option(181)*number(351)"));
        assert!(expr.contains("*option(182)*number(350)"));
    }

    #[test]
    fn lua_skin_infers_or_eq_zero_and_lt_zero_draw() {
        let root = unique_test_dir("bmz-skin-lua-or-zero");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {
                    {
                        id = "miss-f",
                        draw = function()
                            return main_state.number(71) == 0 or main_state.number(150) == 0
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                    {
                        id = "zero-mask",
                        draw = function()
                            return main_state.number(77) < 0 or main_state.number(150) < 0
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("result.luaskin"),
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
        );
        let bmz_skin_document::DestinationListEntry::Single(miss) = &loaded.document.destination[0]
        else {
            panic!("expected single destination");
        };
        let bmz_skin_document::DestinationListEntry::Single(mask) = &loaded.document.destination[1]
        else {
            panic!("expected single destination");
        };
        assert_eq!(miss.draw, "number(71) == 0 or number(150) == 0");
        assert_eq!(mask.draw, "number(77) < 0 or number(150) < 0");
    }

    #[test]
    fn lua_skin_infers_result_average_timing_sign_draw() {
        let root = unique_test_dir("bmz-skin-lua-average-timing-sign");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 7,
                image = {
                    { id = "judge_adv_f", src = "src", x = 0, y = 0, w = 52, h = 12 },
                    { id = "judge_adv_s", src = "src", x = 0, y = 12, w = 52, h = 12 },
                    { id = "judge_adv_non_negative", src = "src", x = 0, y = 24, w = 52, h = 12 },
                },
                destination = {
                    {
                        id = "judge_adv_s",
                        draw = function()
                            local ave_timing = main_state.number(374) + (main_state.number(375) * 0.01)
                            return ave_timing < 0
                        end,
                        dst = {{ x = 424, y = 132, w = 52, h = 12 }},
                    },
                    {
                        id = "judge_adv_f",
                        draw = function()
                            local ave_timing = main_state.number(374) + (main_state.number(375) * 0.01)
                            return 0 < ave_timing
                        end,
                        dst = {{ x = 424, y = 132, w = 52, h = 12 }},
                    },
                    {
                        id = "judge_adv_non_negative",
                        draw = function()
                            local ave_timing = main_state.number(374) + (main_state.number(375) * 0.01)
                            return ave_timing >= 0
                        end,
                        dst = {{ x = 424, y = 132, w = 52, h = 12 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("result.luaskin"),
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
        );
        let bmz_skin_document::DestinationListEntry::Single(slow) = &loaded.document.destination[0]
        else {
            panic!("expected slow destination");
        };
        let bmz_skin_document::DestinationListEntry::Single(fast) = &loaded.document.destination[1]
        else {
            panic!("expected fast destination");
        };
        let bmz_skin_document::DestinationListEntry::Single(non_negative) =
            &loaded.document.destination[2]
        else {
            panic!("expected non-negative destination");
        };
        assert_eq!(slow.draw, "number(374) < 0 or number(375) < 0");
        assert_eq!(fast.draw, "number(374) > 0 or number(375) > 0");
        assert_eq!(non_negative.draw, "number(374) >= 0 and number(375) >= 0");
    }

    #[test]
    fn lua_skin_infers_all_terminal_timers_off_draw() {
        let root = unique_test_dir("bmz-skin-lua-all-timers-off");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 7,
                image = {
                    { id = "irWait", src = "src", x = 0, y = 0, w = 10, h = 10 },
                },
                destination = {
                    {
                        id = "irWait",
                        timer = 172,
                        draw = function()
                            return main_state.timer(173) == main_state.timer_off_value
                                and main_state.timer(174) == main_state.timer_off_value
                        end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("result.luaskin"),
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let bmz_skin_document::DestinationListEntry::Single(wait) = &loaded.document.destination[0]
        else {
            panic!("expected wait destination");
        };
        assert_eq!(wait.draw, "timer(173) == timer_off and timer(174) == timer_off");
    }

    #[test]
    fn lua_skin_infers_draw_with_skin_config_option_and_number() {
        let root = unique_test_dir("bmz-skin-lua-skin-config-draw");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                property = {
                    {
                        name = "mybest スコアが存在しない時",
                        def = "976",
                        item = {
                            { name = "976", op = 976 },
                            { name = "off", op = 0 },
                        },
                    },
                },
                destination = {
                    {
                        id = "score-diff",
                        draw = function()
                            return main_state.number(150) == 0
                                and skin_config.option["mybest スコアが存在しない時"] == 976
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
            }
            "#,
        )
        .unwrap();

        let mut options = BTreeMap::new();
        options.insert("mybest スコアが存在しない時".to_string(), "976".to_string());
        let loaded =
            load_lua_skin(&root.join("play7.luaskin"), SkinKind::Play, &options, &BTreeMap::new())
                .unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
        );
        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("expected single destination");
        };
        assert_eq!(destination.draw, "number(150) == 0");
    }

    #[test]
    fn lua_skin_infers_skin_config_only_draw() {
        let root = unique_test_dir("bmz-skin-lua-skin-config-only-draw");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            return {
                type = 0,
                property = {
                    {
                        name = "グルーヴゲージ表示",
                        def = "default",
                        item = {
                            { name = "default", op = 930 },
                            { name = "all_off", op = 932 },
                        },
                    },
                },
                destination = {
                    {
                        id = "gaugevalue",
                        draw = function()
                            return skin_config.option["グルーヴゲージ表示"] ~= 932
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
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
        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
        );
        let bmz_skin_document::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("expected single destination");
        };
        assert_eq!(destination.draw, "number(0) >= 0");
    }

    #[test]
    fn lua_skin_infers_fast_slow_ratio_graph_type() {
        let root = unique_test_dir("bmz-skin-lua-fs-graph");
        fs::create_dir_all(&root).unwrap();
        let refs = [410, 411, 412, 413, 414, 415, 416, 417, 418, 419, 421, 422];
        let sum_lines: String = refs
            .iter()
            .map(|ref_id| format!("main_state.number({ref_id})"))
            .collect::<Vec<_>>()
            .join(" + ");
        fs::write(
            root.join("select.luaskin"),
            format!(
                r#"
            local main_state = require("main_state")
            return {{
                type = 0,
                graph = {{
                    {{
                        id = "fast",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local fastall = main_state.number(410) + main_state.number(412)
                                + main_state.number(414) + main_state.number(416)
                                + main_state.number(418) + main_state.number(421)
                            local fsall = {sum_lines}
                            if fsall == 0 then return 0 end
                            return fastall / fsall
                        end,
                    }},
                    {{
                        id = "slow",
                        src = 1,
                        x = 0,
                        y = 0,
                        w = 10,
                        h = 10,
                        value = function()
                            local slowall = main_state.number(411) + main_state.number(413)
                                + main_state.number(415) + main_state.number(417)
                                + main_state.number(419) + main_state.number(422)
                            local fsall = {sum_lines}
                            if fsall == 0 then return 0 end
                            return slowall / fsall
                        end,
                    }},
                }},
            }}
            "#
            ),
        )
        .unwrap();

        let loaded = load_lua_skin(
            &root.join("select.luaskin"),
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "warnings: {:?}",
            loaded.warnings.iter().map(|w| w.message.as_str()).collect::<Vec<_>>()
        );
        assert_eq!(loaded.document.graph[0].graph_type, 148);
        assert_eq!(loaded.document.graph[1].graph_type, 149);
        assert!(loaded.document.graph[0].value_expr.is_empty());
        assert!(loaded.document.graph[1].value_expr.is_empty());
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

    #[test]
    fn lua_skin_stops_infinite_inference_callback() {
        let root = unique_test_dir("bmz-skin-lua-inference-limit");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            return {
                type = 0,
                value = {{
                    id = "loop",
                    value = function() while true do end end,
                }},
            }
            "#,
        )
        .unwrap();

        let loaded =
            load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
                .expect("an uninferrable callback should be dropped without hanging the loader");
        assert!(
            loaded
                .warnings
                .iter()
                .any(|warning| warning.message.contains("unsupported value function"))
        );
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
    }

    #[test]
    fn normalize_lua_skin_category_map_converts_rm_skin_shape() {
        let value = JsonValue::Object(JsonMap::from_iter([(
            "category".to_string(),
            JsonValue::Object(JsonMap::from_iter([
                (
                    "property".to_string(),
                    JsonValue::Object(JsonMap::from_iter([
                        ("name".to_string(), JsonValue::String("Option".to_string())),
                        ("item".to_string(), JsonValue::Array(vec![])),
                    ])),
                ),
                (
                    "filepath".to_string(),
                    JsonValue::Object(JsonMap::from_iter([
                        ("name".to_string(), JsonValue::String("Image".to_string())),
                        ("item".to_string(), JsonValue::Array(vec![])),
                    ])),
                ),
            ])),
        )]));
        let normalized = normalize_lua_skin_category_map(value);
        let JsonValue::Object(map) = normalized else {
            panic!("expected object");
        };
        let JsonValue::Array(categories) = map.get("category").expect("category") else {
            panic!("expected category array");
        };
        assert_eq!(categories.len(), 2);
    }

    #[test]
    fn normalize_lua_skin_offset_map_converts_skin_config_shape() {
        let value = JsonValue::Object(JsonMap::from_iter([(
            "offset".to_string(),
            JsonValue::Object(JsonMap::from_iter([(
                "Song title".to_string(),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::Number(serde_json::Number::from(60))),
                    ("name".to_string(), JsonValue::String("Song title".to_string())),
                    ("y".to_string(), JsonValue::Bool(true)),
                ])),
            )])),
        )]));
        let normalized = normalize_lua_skin_offset_map(value);
        let JsonValue::Object(map) = normalized else {
            panic!("expected object");
        };
        let JsonValue::Array(offsets) = map.get("offset").expect("offset") else {
            panic!("expected offset array");
        };
        assert_eq!(offsets.len(), 1);
    }

    #[test]
    fn normalize_lua_skin_offset_map_wraps_single_offset_def() {
        let value = JsonValue::Object(JsonMap::from_iter([(
            "offset".to_string(),
            JsonValue::Object(JsonMap::from_iter([
                ("id".to_string(), JsonValue::Number(serde_json::Number::from(60))),
                ("name".to_string(), JsonValue::String("Song title".to_string())),
                ("y".to_string(), JsonValue::Bool(true)),
            ])),
        )]));
        let normalized = normalize_lua_skin_offset_map(value);
        let JsonValue::Object(map) = normalized else {
            panic!("expected object");
        };
        let JsonValue::Array(offsets) = map.get("offset").expect("offset") else {
            panic!("expected offset array");
        };
        assert_eq!(offsets.len(), 1);
    }

    #[test]
    fn m_select_lua_select_skin_loads_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/mz-select/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let loaded =
            load_lua_skin(&skin_path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new())
                .unwrap();
        assert_eq!(loaded.document.skin_type, 5);
        assert!(loaded.document.songlist.is_some());
        let version = loaded
            .document
            .text
            .iter()
            .find(|text| text.id == "default_version")
            .expect("m-select version text should decode");
        assert_eq!(version.constant_text, env!("CARGO_PKG_VERSION"));
        for ref_id in 27..=29 {
            assert!(
                loaded.document.value.iter().any(|value| value.ref_id == ref_id),
                "m-select should retain operating-time ref {ref_id}"
            );
        }
    }

    #[test]
    fn lua_skin_infers_rm_skin_score_diff_draw() {
        let root = unique_test_dir("bmz-skin-rm-score-diff");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {
                    {
                        id = "score-diff-best",
                        draw = function()
                            return main_state.float_number(113) == 0 and main_state.number(152) ~= 0
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                    {
                        id = "score-diff-zero",
                        draw = function()
                            return not (main_state.number(153) ~= 0)
                        end,
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
                    },
                },
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
        let draws: Vec<_> = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(d) => Some(d.draw.as_str()),
                _ => None,
            })
            .collect();
        assert!(draws.contains(&"float_number(113) == 0 && number(152) != 0"));
        assert!(draws.contains(&"number(153) == 0"));
    }

    #[test]
    fn lua_skin_inherits_end_of_note_timer_for_duplicate_shadow_layer() {
        let root = unique_test_dir("bmz-skin-eon-shadow");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            return {
                type = 0,
                source = {
                    { id = "system", path = "system.png" },
                },
                image = {
                    { id = "eon", src = "system", x = 0, y = 0, w = 390, h = 35 },
                },
                destination = {
                    {
                        id = "eon",
                        draw = function() return true end,
                        dst = {{ x = 693, y = 522, w = 390, h = 35, r = 64, g = 64, b = 64 }},
                    },
                    {
                        id = "eon",
                        timer = 143,
                        dst = {{ x = 693, y = 522, w = 390, h = 35 }},
                    },
                },
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
        let timers: Vec<_> = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination) => destination.timer,
                _ => None,
            })
            .collect();

        assert_eq!(timers, vec![143, 143]);
    }

    /// Rm-skin 互換作業のベースライン。`data/skins/Rm-skin` が無い環境では skip する。
    #[test]
    fn rm_skin_play7_convert_warnings_baseline() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rm-skin/play7main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin_value(&skin_path, &BTreeMap::new(), &BTreeMap::new())
            .expect("Rm-skin play7 should convert");
        let messages: Vec<_> =
            loaded.warnings.iter().map(|warning| warning.message.as_str()).collect();
        assert!(
            messages.is_empty(),
            "Rm-skin play7 should convert without unsupported-function warnings: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("destination[51].draw")),
            "score diff draw should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("value[14].value")),
            "getDummyNumber values should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("value[31].value")),
            "adjusted-rate should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("slider[3].value")),
            "adjustedcover slider should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("value[50].value")),
            "threshold-num should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("text[4].value")),
            "course table text should be inferred: {messages:?}"
        );
        assert!(
            !messages.iter().any(|message| message.contains("`process`")),
            "loader process callback should be silently skipped: {messages:?}"
        );
    }

    /// WMII FHD result の Lua table が document schema まで decode できることを確認する。
    /// 外部スキンが無い環境では skip する。
    #[test]
    fn wmii_fhd_result_lua_skin_decodes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let runtime_state = LuaLoadRuntimeState {
            number_values: BTreeMap::new(),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::from([(50, false), (51, true)]),
        };
        let loaded = load_lua_skin_with_runtime_state(
            &skin_path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("WMII FHD result should decode as a skin document");

        assert!(!loaded.document.destination.is_empty());
        assert_eq!(loaded.document.result_panel_default, Some(1));
        assert!(loaded.document.graph.iter().any(|graph| {
            graph.id == "ir_scoreGraph1" && graph.value_expr == "bmz:ir_score_rate:1"
        }));
        assert!(loaded.document.value.iter().any(|value| {
            value.id == "ir_diff_score1" && value.value_expr == "bmz:ir_score_diff:1"
        }));
        assert!(
            loaded.document.text.iter().any(|text| text.id == "ir_username1" && text.ref_id == 120)
        );

        let ir_score_draws = loaded.document.destination.iter().filter_map(|entry| match entry {
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "ir_scoreGraph1" =>
            {
                Some(destination.draw.as_str())
            }
            _ => None,
        });
        assert!(ir_score_draws.into_iter().any(|draw| {
            draw.contains("result_panel(1)") && draw.contains("ir_score_rate_band(1,")
        }));
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "irYouFrame"
                    && destination.draw.contains("result_panel(1)")
                    && destination.draw.contains("ir_ranking_user(1)")
        )));
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "irWait"
                    && destination.timer == Some(172)
                    && destination.draw.contains("result_panel(1)")
                    && destination.draw.contains(
                        "timer(173) == timer_off and timer(174) == timer_off"
                    )
        )));
        let p2_random_draws = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id.starts_with("randomKeySet2P_") =>
                {
                    Some(destination.draw.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(p2_random_draws.len(), 7);
        assert!(p2_random_draws.iter().all(|draw| {
            draw.contains("event_index(43) == 2 and option(162)")
                && draw.contains("event_index(43) == 3 and option(163)")
                && !draw.contains("number(0) < 0")
        }));

        let graph_options =
            BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
        let graph_loaded = load_lua_skin_with_runtime_state(
            &skin_path,
            &graph_options,
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("WMII FHD graph panel should decode as a skin document");
        assert_eq!(graph_loaded.document.result_panel_default, Some(2));
        assert!(graph_loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "graphDataFrame"
                    && destination.draw.contains("result_panel(2)")
        )));
        assert!(graph_loaded.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "irDataFrame"
                    && destination.draw.contains("result_panel(1)")
        )));
        let timing_average_draws =
            graph_loaded.document.destination.iter().filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == "timingAvg" =>
                {
                    Some(destination.draw.as_str())
                }
                _ => None,
            });
        let timing_average_draws = timing_average_draws.collect::<Vec<_>>();
        assert!(timing_average_draws.iter().any(|draw| {
            *draw == "result_panel(2) and number(374) < 0 or result_panel(2) and number(375) < 0"
        }));
        assert!(
            timing_average_draws.iter().any(|draw| {
                draw.contains("result_panel(2)")
                    && draw.contains("number(374) >= 0 and number(375) >= 0")
            }),
            "WMII timing average layers must remain mutually exclusive: {timing_average_draws:?}"
        );
        assert!(!timing_average_draws.contains(&"number(0) >= 0"));
    }

    /// Rm-skin ロード成功と destination 非空を確認する。
    #[test]
    fn rm_skin_play7_decodes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rm-skin/play7main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("Rm-skin play7 should decode");
        assert!(!loaded.document.destination.is_empty());
        assert_eq!(loaded.document.skin_type, 0);
        let eon_shadow_draw = "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0";
        let eon_destinations: Vec<_> = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == "eon" =>
                {
                    Some((destination.timer, destination.draw.as_str()))
                }
                _ => None,
            })
            .collect();
        assert!(
            eon_destinations.iter().any(|(timer, _)| *timer == Some(143)),
            "Rm-skin end-of-note animation should use timer 143: {eon_destinations:?}"
        );
        assert!(
            eon_destinations
                .iter()
                .all(|(timer, draw)| timer.is_some() || *draw == eon_shadow_draw),
            "Rm-skin end-of-note shadow layers should keep their runtime draw gate: {eon_destinations:?}"
        );
    }

    #[test]
    fn rmz_skin_play6_decodes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play6main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("Rmz-skin play6 should decode");
        assert_eq!(loaded.document.skin_type, 23);
        assert!(!loaded.document.destination.is_empty());
        let fast_slow_draws = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == "fast" || destination.id == "slow" =>
                {
                    Some((destination.id.as_str(), destination.draw.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            fast_slow_draws.contains(&("fast", "option(1242) && number(525) != 0")),
            "Rmz play6 FAST draw should remain runtime-gated: {fast_slow_draws:?}"
        );
        assert!(
            fast_slow_draws.contains(&("slow", "option(1243) && number(525) != 0")),
            "Rmz play6 SLOW draw should remain runtime-gated: {fast_slow_draws:?}"
        );
        let eon_shadow_draw = "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0";
        let eon_destinations = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id == "eon" =>
                {
                    Some((destination.timer, destination.draw.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            eon_destinations.iter().any(|(timer, _)| *timer == Some(143)),
            "Rmz play6 END_OF_NOTES animation should use timer 143: {eon_destinations:?}"
        );
        assert!(
            eon_destinations
                .iter()
                .any(|(timer, draw)| timer.is_none() && *draw == eon_shadow_draw),
            "Rmz play6 END_OF_NOTES shadow should stay gated by remaining playable notes: {eon_destinations:?}"
        );
        let note = loaded.document.note.expect("play6 note definition");
        assert_eq!(note.note.len(), 6);
        assert_eq!(note.dst.len(), 6);
    }

    #[test]
    fn rmz_skin_play5_keeps_default_lane_colors_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play5main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("Rmz-skin play5 should decode");
        assert_eq!(loaded.document.skin_type, 1);
        assert!(
            loaded.document.property.iter().any(|property| property.name == "Notes 5Key Color"),
            "play5 should expose the lane color option"
        );
        let note = loaded.document.note.expect("play5 note definition");
        assert_eq!(
            note.note,
            vec!["note-Wh", "note-Bl", "note-Ye", "note-Bl", "note-Wh", "note-Sc"]
        );
        assert_eq!(note.dst.len(), 6);
    }

    #[test]
    fn rmz_skin_play5_6key_like_colors_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play5main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Notes 5Key Color".to_string(), "6Key-like".to_string())]);
        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .expect("Rmz-skin play5 6Key-like colors should decode");
        let note = loaded.document.note.expect("play5 note definition");
        assert_eq!(
            note.note,
            vec!["note-Bl", "note-Wh", "note-Wh", "note-Bl", "note-Wh", "note-Wh"]
        );
        assert_eq!(note.dst.len(), 6);

        let options = BTreeMap::from([
            ("Scratch Side".to_string(), "Right".to_string()),
            ("Notes 5Key Color".to_string(), "6Key-like".to_string()),
        ]);
        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .expect("Rmz-skin play5 6Key-like right scratch colors should decode");
        let note = loaded.document.note.expect("play5 note definition");
        assert_eq!(
            note.note,
            vec!["note-Wh", "note-Bl", "note-Wh", "note-Wh", "note-Bl", "note-Wh"]
        );
        assert_eq!(note.dst.len(), 6);
    }

    #[test]
    fn rmz_skin_play6_enlarge_uses_wide_note_lanes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play6main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Notes 6Key Align".to_string(), "Enlarge".to_string())]);
        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .expect("Rmz-skin play6 enlarge should decode");
        let note = loaded.document.note.expect("play6 note definition");
        let widths: Vec<_> = note
            .dst
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::SkinDstEntry::Frame(frame) => frame.w,
                _ => None,
            })
            .collect();

        assert_eq!(widths, vec![132; 6]);
    }

    #[test]
    fn rmz_skin_play4_decodes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play4main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("Rmz-skin play4 should decode");
        assert_eq!(loaded.document.skin_type, 22);
        assert!(!loaded.document.destination.is_empty());
        let note = loaded.document.note.expect("play4 note definition");
        assert_eq!(note.note, vec!["note-Wh", "note-Bl", "note-Bl", "note-Wh"]);
        assert_eq!(note.dst.len(), 4);
    }

    #[test]
    fn rmz_skin_play4_enlarge_uses_wide_note_lanes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play4main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Notes 4Key Align".to_string(), "Enlarge".to_string())]);
        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .expect("Rmz-skin play4 enlarge should decode");
        let note = loaded.document.note.expect("play4 note definition");
        let widths: Vec<_> = note
            .dst
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::SkinDstEntry::Frame(frame) => frame.w,
                _ => None,
            })
            .collect();

        assert_eq!(widths, vec![132; 4]);
    }

    #[test]
    fn peaceful_play_integral_property_ops_are_selectable_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/PeacefulPlay/play9.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let loaded = load_lua_skin(&skin_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
            .expect("PeacefulPlay play9 should decode");
        let property_warnings = loaded
            .warnings
            .iter()
            .filter(|warning| warning.message.contains("has no selectable op"))
            .map(|warning| warning.message.as_str())
            .collect::<Vec<_>>();

        assert!(
            property_warnings.is_empty(),
            "PeacefulPlay properties should accept integral Lua-number ops: {property_warnings:?}"
        );
        let duration_info = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if matches!(
                        destination.id.as_str(),
                        "val-duration" | "val-lanecover-amount" | "val-duration-green"
                    ) =>
                {
                    Some(destination)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(duration_info.len(), 3);
        assert!(
            duration_info.iter().all(|destination| {
                destination.draw == "option(80) or option(81) and timer(40) == timer_off"
            }),
            "duration info: {duration_info:?}"
        );
        assert_eq!(
            loaded
                .document
                .value
                .iter()
                .find(|value| value.id == "val-hits-per-sec")
                .map(|value| value.value_expr.as_str()),
            Some("bmz:keylogger_nps")
        );
        let keylogger_graphs = loaded
            .document
            .graph
            .iter()
            .filter(|graph| graph.id.starts_with("keylogger-graph-"))
            .collect::<Vec<_>>();
        assert!(!keylogger_graphs.is_empty());
        assert!(
            keylogger_graphs
                .iter()
                .all(|graph| { graph.value_expr.starts_with("bmz:keylogger_graph:") })
        );
        let judge_color = load_lua_skin(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::from([("ノーツ色 Note Color".to_string(), "JUDGE".to_string())]),
            &BTreeMap::new(),
        )
        .expect("PeacefulPlay judge-color key logger should decode");
        let keylogger_notes = judge_color
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id.starts_with("keylogger-note-judge-") =>
                {
                    Some(destination)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!keylogger_notes.is_empty());
        assert!(keylogger_notes.iter().all(|destination| {
            destination.timer_expr.starts_with("bmz:keylogger_event:")
                && destination.draw.starts_with("keylogger_judge(")
        }));
        let keybeams = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id.starts_with("key-beam-") =>
                {
                    Some(destination)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(keybeams.len(), 9 * 4 * 2);
        for pair in keybeams.chunks_exact(2) {
            assert!(pair[0].timer.is_none());
            assert!(pair[0].draw.starts_with("keybeam_hold("), "hold: {:?}", pair[0]);
            assert!(matches!(pair[1].timer, Some(120..=129)));
            assert!(pair[1].draw.starts_with("keybeam_fade("), "fade: {:?}", pair[1]);
        }
        assert_eq!(loaded.warnings.len(), 8, "warnings: {:?}", loaded.warnings);
        assert!(loaded.warnings.iter().all(|warning| {
            warning.message.starts_with("skipping unsupported custom timer function id 1190")
        }));
        let gauge_lead_glow = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_skin_document::DestinationListEntry::Single(destination)
                    if destination.id.starts_with("gauge-lead-glow-") =>
                {
                    Some(destination)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(gauge_lead_glow.len(), 216);
        assert!(
            gauge_lead_glow
                .iter()
                .all(|destination| { destination.draw.starts_with("gauge_lead_glow(") }),
            "unexpected gauge predicates: {:?}",
            gauge_lead_glow
                .iter()
                .filter(|destination| !destination.draw.starts_with("gauge_lead_glow("))
                .map(|destination| (&destination.id, &destination.draw))
                .collect::<Vec<_>>()
        );
        let sevenkeys_path = skin_path.with_file_name("play7_9lane.luaskin");
        let sevenkeys =
            load_lua_skin(&sevenkeys_path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new())
                .expect("PeacefulPlay play7_9lane should decode");
        assert!(sevenkeys.document.destination.iter().any(|entry| matches!(
            entry,
            bmz_skin_document::DestinationListEntry::Single(destination)
                if destination.id == "gauge-lead-glow-groove-below"
                    && destination.draw.starts_with("gauge_lead_glow(groove,")
        )));
        assert_eq!(
            loaded.document.fixed_delay_timers,
            vec![bmz_skin_document::SkinFixedDelayTimerDef {
                id: 11900,
                source_timer: 143,
                delay_ms: 2000,
            }],
            "only PeacefulPlay's end-of-note fixed-delay timer should be inferred"
        );
    }

    #[test]
    fn peaceful_play_gauge_overlay_keeps_one_destination_per_integer_width() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/PeacefulPlay/play9.luaskin");
        if !skin_path.is_file() {
            return;
        }

        for (display, mode, integer_id) in [
            ("%", "percent", "val-gauge-percent-integer"),
            ("Value", "amount", "val-gauge-amount-integer"),
        ] {
            let properties = BTreeMap::from([
                ("ゲージ量オーバーレイ Gauge Value Overlay".to_string(), "ON(100%)".to_string()),
                ("ゲージ量表示方式 Gauge Value Display Mode".to_string(), display.to_string()),
            ]);
            let loaded = load_lua_skin(&skin_path, SkinKind::Play, &properties, &BTreeMap::new())
                .expect("PeacefulPlay gauge overlay should decode");
            assert_eq!(
                loaded.warnings.len(),
                8,
                "{display} overlay warnings: {:?}",
                loaded.warnings
            );
            assert!(loaded.warnings.iter().all(|warning| {
                warning.message.starts_with("skipping unsupported custom timer function id 1190")
            }));
            let predicates = loaded
                .document
                .destination
                .iter()
                .filter_map(|entry| match entry {
                    bmz_skin_document::DestinationListEntry::Single(destination)
                        if destination.id == integer_id =>
                    {
                        Some(destination.draw.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                predicates,
                (1..=3)
                    .map(|digits| format!("gauge_value_digits({mode},{digits})"))
                    .collect::<Vec<_>>()
            );
        }
    }
}
