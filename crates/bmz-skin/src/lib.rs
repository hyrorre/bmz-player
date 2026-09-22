use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use bmz_skin_document::SkinDocument;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;

mod lr2;
mod lua;
mod path_context;
mod pm_chara;

pub use path_context::SkinPathContext;
pub use pm_chara::{LoadedPmChara, load_pm_chara};

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

#[derive(Debug)]
pub struct LoadedSkinDocument {
    pub document: SkinDocument,
    /// Lua skin にだけ付属する、シリアライズ不能な callback runtime。
    ///
    /// `SkinDocument` には callback ID だけを含む描画条件を残し、Lua VM と
    /// registry key はこの sidecar の寿命内に閉じ込める。
    pub lua_runtime: Option<LuaSkinRuntime>,
    pub warnings: Vec<SkinLoadWarning>,
    pub files: BTreeMap<String, String>,
    pub dependencies: SkinLoadDependencies,
}

#[derive(Debug)]
pub struct LoadedLuaSkinValue {
    pub value: JsonValue,
    pub lua_runtime: Option<LuaSkinRuntime>,
    pub runtime_callback_paths: Vec<String>,
    /// Backward-compatible subset containing only `draw` callback paths.
    pub runtime_draw_paths: Vec<String>,
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
    pub event_index_values: BTreeMap<i32, i32>,
    pub offset_values: BTreeMap<String, LuaSkinOffsetValue>,
    pub offset_id_values: BTreeMap<i32, LuaSkinOffsetValue>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LuaSkinOffsetValue {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub r: i32,
    pub a: i32,
}

/// Lua function fields の評価方法。
///
/// `Auto` は推論できる function を宣言的な skin document へ変換し、推論できない
/// 対応済み field だけ runtime callback に残す。`Compat` はスキン開発・beatoraja
/// 比較向けに、対応済み function field を可能な限り永続 Lua VM で評価する。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LuaSkinRuntimeMode {
    #[default]
    Auto,
    Compat,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuaLoadRuntimeState {
    pub runtime_mode: LuaSkinRuntimeMode,
    pub number_values: BTreeMap<i32, i32>,
    pub text_values: BTreeMap<i32, String>,
    pub option_values: BTreeMap<i32, bool>,
    pub event_index_values: BTreeMap<i32, i32>,
    /// Header display name keyed values used by `skin_config.offset`.
    pub offset_values: BTreeMap<String, LuaSkinOffsetValue>,
    /// Numeric ID keyed values used by `main_state.offset`.
    pub offset_id_values: BTreeMap<i32, LuaSkinOffsetValue>,
    /// App-owned, read-only files exposed to Lua `io.open` during skin load.
    ///
    /// This is used for compatibility data whose original implementation is
    /// written by Java/Lua at runtime.  Keeping it in the load state makes the
    /// document cache invalidate when the supplied result data changes.
    pub virtual_io_files: BTreeMap<String, String>,
}

impl LuaLoadRuntimeState {
    /// Resolves ordered header definitions against ordered saved values.
    ///
    /// beatoraja selects the first saved value with a matching display name,
    /// while its numeric runtime map overwrites duplicate IDs in header order.
    /// Missing names receive the all-zero value.
    pub fn set_offset_definitions(
        &mut self,
        definitions: impl IntoIterator<Item = (String, i32)>,
        saved_values: impl IntoIterator<Item = (String, LuaSkinOffsetValue)>,
    ) {
        let mut saved_by_name = BTreeMap::new();
        for (name, value) in saved_values {
            saved_by_name.entry(name).or_insert(value);
        }

        self.offset_values.clear();
        self.offset_id_values.clear();
        for (name, id) in definitions {
            let value = saved_by_name.get(&name).copied().unwrap_or_default();
            self.offset_values.entry(name).or_insert(value);
            self.offset_id_values.insert(id, value);
        }
    }
}

/// Runtime callback から参照できる、現在フレームの読み取り専用状態。
///
/// 実装側は renderer の snapshot などを借用してよい。Lua へ Rust オブジェクト
/// 自体を渡さず、callback 実行中にこの accessor を同期的に読むだけにする。
pub trait LuaMainState {
    fn option(&self, id: i32) -> bool;
    fn number(&self, id: i32) -> i64;
    /// Returns the current EX score used by Lua skins.
    ///
    /// beatoraja exposes this as `main_state.exscore()`, while the numeric
    /// state reference is 71. Keep the default so existing runtime adapters
    /// remain source-compatible.
    fn exscore(&self) -> i64 {
        self.number(71)
    }
    fn float(&self, id: i32) -> f64;
    fn text(&self, id: i32) -> String;
    fn timer(&self, id: i32) -> Option<i32>;
    fn event_index(&self, id: i32) -> i32;
    fn gauge_type(&self) -> i32;
    fn time_us(&self) -> i32;

    fn total_play_counts_in_session(&self) -> i64 {
        0
    }
    fn total_play_notes_in_session(&self) -> i64 {
        0
    }
    /// Unix seconds of the selected score, or zero if unavailable.
    fn score_date_sec_time(&self) -> i64 {
        0
    }

    fn judge(&self, index: i32) -> i64 {
        main_state_judge_ref(index).map_or(0, |id| self.number(id))
    }

    fn offset(&self, _id: i32) -> LuaSkinOffsetValue {
        LuaSkinOffsetValue::default()
    }
}

fn main_state_judge_ref(index: i32) -> Option<i32> {
    match index {
        0 => Some(110),
        1 => Some(111),
        2 => Some(112),
        3 => Some(113),
        4 => Some(114),
        5 => Some(420),
        _ => None,
    }
}

pub use lua::LuaSkinRuntime;

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
    let path_context = SkinPathContext::for_entry(path)?;
    load_lua_skin_with_path_context(&path_context, options, files, runtime_state, virtual_io_files)
}

/// Loads a Lua skin using explicit package library roots shared by every Lua VM
/// and by the caller's subsequent asset resolution.
pub fn load_lua_skin_with_path_context(
    path_context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<LoadedSkinDocument> {
    let loaded = lua::load_lua_skin_value_with_path_context(
        path_context,
        options,
        files,
        runtime_state,
        virtual_io_files,
    )?;
    let value = normalize_lua_skin_document(loaded.value);
    let mut document: SkinDocument =
        serde_path_to_error::deserialize(value).with_context(|| {
            format!("failed to parse lua skin as document: {}", path_context.entry_file().display())
        })?;
    document.internal_enabled_options = loaded.internal_enabled_options;
    Ok(LoadedSkinDocument {
        document,
        lua_runtime: loaded.lua_runtime,
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
        lua_runtime: None,
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

pub fn load_lua_skin_header_value_with_path_context(
    path_context: &SkinPathContext,
) -> Result<LoadedLuaSkinValue> {
    let mut loaded = lua::load_lua_skin_header_value_with_path_context(path_context)?;
    loaded.value = normalize_lua_skin_document(loaded.value);
    Ok(loaded)
}

fn normalize_lua_skin_document(value: JsonValue) -> JsonValue {
    let value = bmz_skin_document::normalize_lua_json_skin_integer_numbers(value);
    let value = normalize_lua_skin_category_map(value);
    let value = normalize_lua_skin_offset_map(value);
    let value = normalize_lua_skin_category_labels(value);
    normalize_lua_skin_offset_flags(value)
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

/// LuaSkinLoader は Java の String フィールドへ Lua の数値を渡すと `tojstring()` で
/// 文字列化する。ModernChic は category ID に数値を使うため、厳密な JSON decode の
/// 前に同じ変換を行う。
fn normalize_lua_skin_category_labels(value: JsonValue) -> JsonValue {
    let JsonValue::Object(mut map) = value else {
        return value;
    };

    if let Some(JsonValue::Array(categories)) = map.get_mut("category") {
        for category in categories {
            let JsonValue::Object(category) = category else {
                continue;
            };
            stringify_json_scalar(category.get_mut("name"));
            if let Some(JsonValue::Array(items)) = category.get_mut("item") {
                for item in items {
                    stringify_json_scalar(Some(item));
                }
            }
        }
    }

    for key in ["property", "filepath", "offset"] {
        if let Some(JsonValue::Array(definitions)) = map.get_mut(key) {
            for definition in definitions {
                let JsonValue::Object(definition) = definition else {
                    continue;
                };
                stringify_json_scalar(definition.get_mut("category"));
            }
        }
    }

    JsonValue::Object(map)
}

fn stringify_json_scalar(value: Option<&mut JsonValue>) {
    let Some(value) = value else {
        return;
    };
    let replacement = match value {
        JsonValue::Number(number) => Some(number.to_string()),
        JsonValue::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    };
    if let Some(replacement) = replacement {
        *value = JsonValue::String(replacement);
    }
}

/// LuaSkinLoader の boolean 変換は Lua の truthiness (`toboolean`) に従う。
/// Lua では数値の 0 も true なので、JSON の 0/1 判定にはしない。
fn normalize_lua_skin_offset_flags(value: JsonValue) -> JsonValue {
    let JsonValue::Object(mut map) = value else {
        return value;
    };
    if let Some(JsonValue::Array(offsets)) = map.get_mut("offset") {
        for offset in offsets {
            let JsonValue::Object(offset) = offset else {
                continue;
            };
            for key in ["x", "y", "w", "h", "r", "a"] {
                let Some(flag) = offset.get_mut(key) else {
                    continue;
                };
                if !flag.is_boolean() {
                    *flag = JsonValue::Bool(!flag.is_null());
                }
            }
        }
    }
    JsonValue::Object(map)
}

/// `skin_config.offset` is keyed by display name for Lua access, while beatoraja JSON uses an
/// array of offset definitions.
fn normalize_lua_skin_offset_map(value: JsonValue) -> JsonValue {
    normalize_lua_skin_offset_map_for_key(None, value)
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

pub fn convert_lua_skin_to_json_file_with_path_context(
    path_context: &SkinPathContext,
    output: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<Vec<SkinLoadWarning>> {
    let report =
        lua::convert_lua_skin_to_json_with_path_context(path_context, output, options, files)?;
    Ok(report.warnings.into_iter().map(|message| SkinLoadWarning { message }).collect())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
