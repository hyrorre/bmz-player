pub(super) fn skin_config_options_from_header(
    header: &JsonValue,
    selected: &BTreeMap<String, String>,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, i64> {
    let mut result = BTreeMap::new();
    let Some(properties) = header.get("property").and_then(JsonValue::as_array) else {
        return result;
    };

    for property in properties {
        let Some(name) = property.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(items) = property.get("item").and_then(JsonValue::as_array) else {
            continue;
        };
        let selected_value = selected.get(name).map(String::as_str);
        let op = selected_value
            .and_then(|value| option_value_to_op(items, value))
            .or_else(|| default_property_op(property, items));
        if let Some(op) = op {
            result.insert(name.to_string(), op);
        } else {
            warnings.push(format!("property `{name}` has no selectable op"));
        }
    }

    for (key, value) in selected {
        if !result.contains_key(key) && value.parse::<i64>().is_err() {
            warnings.push(format!("option `{key}` did not match a skin property"));
        }
    }

    result
}

pub(super) fn option_value_to_op(items: &[JsonValue], value: &str) -> Option<i64> {
    if let Ok(op) = value.parse::<i64>() {
        return items
            .iter()
            .find_map(|item| (item.get("op").and_then(json_integer) == Some(op)).then_some(op));
    }
    items.iter().find_map(|item| {
        (item.get("name").and_then(JsonValue::as_str) == Some(value))
            .then(|| item.get("op").and_then(json_integer))
            .flatten()
    })
}

pub(super) fn default_property_op(property: &JsonValue, items: &[JsonValue]) -> Option<i64> {
    if let Some(default_name) = property.get("def").and_then(JsonValue::as_str)
        && let Some(op) = option_name_to_op(items, default_name)
    {
        return Some(op);
    }
    items.first().and_then(|item| item.get("op")).and_then(json_integer)
}

pub(super) fn option_name_to_op(items: &[JsonValue], value: &str) -> Option<i64> {
    items.iter().find_map(|item| {
        (item.get("name").and_then(JsonValue::as_str) == Some(value))
            .then(|| item.get("op").and_then(json_integer))
            .flatten()
    })
}

pub(super) fn json_integer(value: &JsonValue) -> Option<i64> {
    value.as_i64().or_else(|| {
        let value = value.as_f64()?;
        (value.is_finite()
            && value.fract() == 0.0
            && value >= i64::MIN as f64
            && value <= i64::MAX as f64)
            .then_some(value as i64)
    })
}

pub(super) fn skin_config_offsets_from_header(
    header: &JsonValue,
    runtime_state: &LuaLoadRuntimeState,
) -> BTreeMap<String, LuaSkinOffsetValue> {
    let mut result = BTreeMap::new();
    for (name, id) in skin_offset_definitions_from_header(header) {
        result.insert(name.clone(), lua_skin_offset_value(runtime_state, &name, Some(id)));
    }
    result
}

pub(super) fn skin_offset_definitions_from_header(header: &JsonValue) -> Vec<(String, i32)> {
    let mut result = Vec::new();
    if let Some(offsets) = header.get("offset").and_then(JsonValue::as_array) {
        for offset in offsets {
            let Some(name) = offset.get("name").and_then(JsonValue::as_str) else {
                continue;
            };
            let id = offset
                .get("id")
                .and_then(json_integer)
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or_default();
            result.push((name.to_string(), id));
        }
    }

    let skin_type = header.get("type").and_then(json_integer).and_then(|id| i32::try_from(id).ok());
    if matches!(skin_type, Some(0 | 1 | 2 | 3 | 4 | 12 | 13 | 16 | 17 | 21 | 22 | 23 | 24)) {
        // JSONSkinLoader appends these after custom definitions before
        // SkinLuaAccessor exports skin_config. BMZ offset 34 is intentionally
        // renderer-only and is not part of beatoraja's Lua configuration.
        for (name, id) in [
            ("All offset(%)", 10),
            ("Notes offset", 30),
            ("Judge offset", 32),
            ("Judge Detail offset", 33),
        ] {
            result.push((name.to_string(), id));
        }
    }

    result
}

pub(super) fn lua_skin_offset_value(
    runtime_state: &LuaLoadRuntimeState,
    name: &str,
    id: Option<i32>,
) -> LuaSkinOffsetValue {
    runtime_state
        .offset_values
        .get(name)
        .copied()
        .or_else(|| id.and_then(|id| runtime_state.offset_id_values.get(&id).copied()))
        .unwrap_or_default()
}

/// スキン設定パネルで選んだファイル選択を、filepath 定義の `path` グロブごとに
/// 集める。キーは `path` グロブ (区切りを `/` に正規化)、値は選択ファイルの
/// スキンルート相対パス。選択が無い / 空の定義は含めない。
pub(super) fn skin_files_from_header(
    path_context: &SkinPathContext,
    header: &JsonValue,
    selected: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let Some(filepaths) = header.get("filepath").and_then(JsonValue::as_array) else {
        return result;
    };
    for filepath in filepaths {
        let Some(name) = filepath.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(path) = filepath.get("path").and_then(JsonValue::as_str) else {
            continue;
        };
        let normalized_path = path.replace('\\', "/");
        let choice =
            selected.get(name).filter(|choice| !choice.is_empty()).cloned().or_else(|| {
                default_skin_file_from_filepath(path_context, &normalized_path, filepath)
            });
        if let Some(choice) = choice {
            result.insert(normalized_path, choice);
        }
    }
    result
}

pub(super) fn skin_named_files_from_header(
    path_context: &SkinPathContext,
    header: &JsonValue,
    selected: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let Some(filepaths) = header.get("filepath").and_then(JsonValue::as_array) else {
        return result;
    };
    for filepath in filepaths {
        let Some(name) = filepath.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(path) = filepath.get("path").and_then(JsonValue::as_str) else {
            continue;
        };
        let normalized_path = path.replace('\\', "/");
        let choice =
            selected.get(name).filter(|choice| !choice.is_empty()).cloned().or_else(|| {
                default_skin_file_from_filepath(path_context, &normalized_path, filepath)
            });
        if let Some(choice) = choice {
            result.insert(name.to_string(), choice);
        }
    }
    result
}

pub(super) fn skin_file_dependency_names_from_header(
    header: &JsonValue,
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let Some(filepaths) = header.get("filepath").and_then(JsonValue::as_array) else {
        return result;
    };
    for filepath in filepaths {
        let Some(name) = filepath.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(path) = filepath.get("path").and_then(JsonValue::as_str) else {
            continue;
        };
        result.insert(path.replace('\\', "/"), name.to_string());
    }
    result
}

/// beatoraja のファイル選択カスタマイズで「ランダム」を表す番兵値。
/// `skin_files` の値がこれのとき、`skin_config.get_path` はロードごとに候補から
/// ランダムに選ぶ。
pub(super) const RANDOM_FILE_SELECTION: &str = "Random";

/// ロードごとにOS乱数で抽選する。取得失敗時は警告して先頭候補を使う。
pub(super) fn random_skin_file_index(len: usize) -> usize {
    debug_assert!(len > 0);
    crate::random_file_index(len).unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to select random skin file; using first candidate");
        0
    })
}

pub(super) fn default_skin_file_from_filepath(
    path_context: &SkinPathContext,
    normalized_path: &str,
    filepath: &JsonValue,
) -> Option<String> {
    let candidates = skin_file_candidates(path_context, normalized_path);
    if candidates.is_empty() {
        return None;
    }
    let default_name = filepath.get("def").and_then(JsonValue::as_str).unwrap_or_default();
    if !default_name.is_empty() {
        // def="Random" は具体ファイルへ固定せず、ランダム番兵を既定にする。
        if default_name.eq_ignore_ascii_case(RANDOM_FILE_SELECTION) {
            return Some(RANDOM_FILE_SELECTION.to_string());
        }
        if let Some(candidate) = candidates.iter().find(|candidate| {
            filename_matches_def(candidate.to_string_lossy().as_ref(), default_name)
        }) {
            return Some(candidate_file_name(candidate.to_string_lossy().as_ref()));
        }
    } else if let Some(candidate) = candidates
        .iter()
        .find(|candidate| filename_matches_def(candidate.to_string_lossy().as_ref(), "default"))
    {
        return Some(candidate_file_name(candidate.to_string_lossy().as_ref()));
    }
    candidates
        .into_iter()
        .next()
        .map(|candidate| candidate_file_name(candidate.to_string_lossy().as_ref()))
}
use super::*;
