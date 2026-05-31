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
    let value = normalize_lua_skin_document(loaded.value);
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

fn normalize_lua_skin_document(value: JsonValue) -> JsonValue {
    let value = normalize_json_skin_integer_numbers(value);
    normalize_lua_skin_category_map(value)
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
    fn lua_skin_io_stub_reads_skin_alias_and_ignores_writes() {
        let parent = unique_test_dir("bmz-skin-lua");
        let root = parent.join("m_select");
        fs::create_dir_all(root.join("customize/advanced")).unwrap();
        fs::write(root.join("customize/advanced/enable.txt"), "first.lua\nsecond.lua\n").unwrap();
        fs::write(
            root.join("music_select.luaskin"),
            r#"
            local f = io.open("skin/m_select/customize/advanced/enable.txt", "r")
            local out = io.open("skin/m_select/customize/advanced/load_log.txt", "w")
            local count = 0
            for line in f:lines() do
                count = count + 1
                out:write(line)
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

        assert_eq!(loaded.value["text"][0]["constantText"], "4");
        assert!(!root.join("customize/advanced/load_log.txt").exists());
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
        assert_eq!(loaded.value["text"][0]["constantText"], "Player 0.1.0");
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
    fn lua_skin_silently_skips_loader_callback_fields() {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            return {
                type = 0,
                image = {
                    { id = "button", src = "src", x = 0, y = 0, w = 10, h = 10, act = function() return true end }
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

        assert!(loaded.warnings.is_empty());
        assert!(loaded.value["image"][0].get("act").is_none());
        assert!(loaded.value["customTimers"][0].get("timer").is_none());
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
        let bmz_render::skin::DestinationListEntry::Single(destination) =
            &loaded.document.destination[0]
        else {
            panic!("destination should be single");
        };
        assert_eq!(destination.id, "frame-panel");
        assert_eq!(destination.timer, Some(bmz_render::skin::SKIN_DYNAMIC_TIMER_BASE));
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

        let bmz_render::skin::DestinationListEntry::Single(destination) =
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
        let bmz_render::skin::DestinationListEntry::Single(destination) =
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
        let bmz_render::skin::DestinationListEntry::Single(miss) = &loaded.document.destination[0]
        else {
            panic!("expected single destination");
        };
        let bmz_render::skin::DestinationListEntry::Single(mask) = &loaded.document.destination[1]
        else {
            panic!("expected single destination");
        };
        assert_eq!(miss.draw, "number(71) == 0 or number(150) == 0");
        assert_eq!(mask.draw, "number(77) < 0 or number(150) < 0");
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
        let bmz_render::skin::DestinationListEntry::Single(destination) =
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
        let bmz_render::skin::DestinationListEntry::Single(destination) =
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
                bmz_render::skin::DestinationListEntry::Single(d) => Some(d.draw.as_str()),
                _ => None,
            })
            .collect();
        assert!(draws.contains(&"float_number(113) == 0 && number(152) != 0"));
        assert!(draws.contains(&"number(153) == 0"));
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
    }
}
