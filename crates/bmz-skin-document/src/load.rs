//! beatoraja JSON skin のロード/前処理 (include 展開、trailing comma 除去、
//! 整数キー正規化、property 既定オプション算出)。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_skin_assets::SkinPathContext;
use serde_json::{Map as JsonMap, Value as JsonValue};

pub fn load_json_value(path: &Path) -> Result<JsonValue> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read skin json: {}", path.display()))?;
    parse_json_skin_text(&decode_json_skin_bytes(&bytes))
        .with_context(|| format!("failed to parse skin json: {}", path.display()))
}

/// Parses beatoraja JSON skin text after the lenient preprocessing that libGDX's
/// `JsonReader` accepts: comments, missing commas between values, and trailing commas.
pub fn parse_json_skin_text(text: &str) -> Result<JsonValue> {
    let text = strip_json_comments(text)?;
    let text = strip_json_trailing_commas(&text);
    let text = insert_missing_commas_between_json_values(&text);
    Ok(serde_json::from_str(&text)?)
}

/// Decodes skin JSON bytes as UTF-8 (with an optional BOM), falling back to
/// Shift_JIS (CP932). beatoraja reads JSON skins with `FileReader`, whose
/// platform-default charset is MS932 on Japanese Windows, so CP932 skins exist.
pub fn decode_json_skin_bytes(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => std::borrow::Cow::Borrowed(text),
        Err(_) => encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes).0,
    }
}

/// Removes `// ...` line comments and `/* ... */` block comments outside strings.
///
/// A line comment keeps its terminating newline, and a block comment becomes a
/// space plus the newlines it contained, so token boundaries and line numbers in
/// later parse errors stay intact. An unterminated block comment is an error.
pub fn strip_json_comments(input: &str) -> Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }
        if ch != '/' {
            output.push(ch);
            continue;
        }
        match chars.peek() {
            Some('/') => {
                while let Some(next) = chars.peek() {
                    if matches!(next, '\n' | '\r') {
                        break;
                    }
                    chars.next();
                }
            }
            Some('*') => {
                chars.next();
                output.push(' ');
                let mut closed = false;
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                    if matches!(next, '\n' | '\r') {
                        output.push(next);
                    }
                }
                anyhow::ensure!(closed, "unterminated block comment in skin json");
            }
            _ => output.push(ch),
        }
    }

    Ok(output)
}

pub fn strip_json_trailing_commas(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while matches!(lookahead.peek(), Some(next) if next.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some(']' | '}')) {
                continue;
            }
        }

        output.push(ch);
    }

    output
}

pub fn insert_missing_commas_between_json_values(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        output.push(ch);
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch != '}' && ch != ']' {
            continue;
        }

        let mut lookahead = chars.clone();
        let mut whitespace = String::new();
        while let Some(next) = lookahead.peek().copied() {
            if next.is_whitespace() {
                whitespace.push(next);
                lookahead.next();
            } else {
                break;
            }
        }
        if matches!(lookahead.peek(), Some('{') | Some('[')) {
            output.push(',');
        }
    }

    output
}

pub fn normalize_json_skin_integer_numbers(value: JsonValue) -> JsonValue {
    normalize_json_skin_integer_numbers_for_key(None, value)
}

pub fn normalize_json_skin_integer_numbers_for_key(
    key: Option<&str>,
    value: JsonValue,
) -> JsonValue {
    match value {
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
                    let value = if is_json_skin_integer_key(Some(&key)) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(Some(&key), value)
                    };
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

pub fn normalize_json_skin_integer_value(value: JsonValue) -> JsonValue {
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
                    let value = if is_json_skin_integer_key(Some(&key)) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(Some(&key), value)
                    };
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        value => value,
    }
}

/// Lua table / LR2 csv から生成した JSON 値向けの整数キー正規化。
///
/// `normalize_json_skin_integer_numbers` (JSON skin ファイル向け) との違いは、
/// Lua 変換が `x = {10}` のような長さ 1 の配列を作ることがあるため、
/// スカラー期待の整数キー (`op` / `offsets` / `time` 以外) では配列を
/// アンラップしてスカラーへ畳み込む点。`offset` は定義の配列にも使うため、
/// オブジェクトや配列を含む場合は1件でも配列を維持する。
pub fn normalize_lua_json_skin_integer_numbers(value: JsonValue) -> JsonValue {
    normalize_lua_json_skin_integer_numbers_for_key(None, value)
}

fn normalize_lua_json_skin_integer_numbers_for_key(
    key: Option<&str>,
    value: JsonValue,
) -> JsonValue {
    match value {
        JsonValue::Array(mut values)
            if is_json_skin_scalar_integer_key(key)
                && values.len() == 1
                && !matches!(values[0], JsonValue::Object(_) | JsonValue::Array(_)) =>
        {
            normalize_lua_json_skin_integer_value(values.remove(0))
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| {
                    if is_json_skin_integer_key(key) {
                        normalize_lua_json_skin_integer_value(value)
                    } else {
                        normalize_lua_json_skin_integer_numbers_for_key(key, value)
                    }
                })
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = normalize_lua_json_skin_integer_numbers_for_key(Some(&key), value);
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

fn normalize_lua_json_skin_integer_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Number(number) => json_number_to_rounded_i64(&number)
            .and_then(serde_json::Number::from_i128)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Number(number)),
        JsonValue::Array(values) => JsonValue::Array(
            values.into_iter().map(normalize_lua_json_skin_integer_value).collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = normalize_lua_json_skin_integer_numbers_for_key(Some(&key), value);
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        value => value,
    }
}

pub fn json_number_to_rounded_i64(number: &serde_json::Number) -> Option<i128> {
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

pub fn is_json_skin_integer_key(key: Option<&str>) -> bool {
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
                | "clip_x"
                | "clip_y"
                | "clip_w"
                | "clip_h"
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

pub fn expand_json_skin_value(
    value: JsonValue,
    current_dir: &Path,
    root_dir: &Path,
    enabled_options: &[i32],
) -> Result<JsonValue> {
    let context = SkinPathContext::for_directory(root_dir)?;
    expand_json_skin_value_with_context(value, current_dir, &context, enabled_options)
}

pub fn expand_json_skin_value_with_context(
    value: JsonValue,
    current_dir: &Path,
    context: &SkinPathContext,
    enabled_options: &[i32],
) -> Result<JsonValue> {
    let mut budget = ExpansionBudget { files: 1, nodes: 0 };
    let expanded = expand_json_skin_value_inner(
        value,
        current_dir,
        context,
        enabled_options,
        &mut Vec::new(),
        &mut budget,
        0,
    )?;
    budget.add_node()?;
    Ok(expanded)
}

const MAX_EXPANSION_FILES: usize = 4096;
const MAX_EXPANSION_NODES: usize = 1_000_000;

struct ExpansionBudget {
    files: usize,
    nodes: usize,
}

impl ExpansionBudget {
    fn add_node(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.nodes < MAX_EXPANSION_NODES,
            "skin json expansion exceeds maximum nodes ({MAX_EXPANSION_NODES})"
        );
        self.nodes += 1;
        Ok(())
    }

    fn append_included(&mut self, output: &mut Vec<JsonValue>, value: JsonValue) -> Result<()> {
        match value {
            // Children are already counted. The flattened array itself is not retained.
            JsonValue::Array(values) => output.extend(values),
            other => {
                self.add_node()?;
                output.push(other);
            }
        }
        Ok(())
    }
}

fn expand_include(
    include: &JsonValue,
    current_dir: &Path,
    context: &SkinPathContext,
    enabled_options: &[i32],
    active: &mut Vec<PathBuf>,
    budget: &mut ExpansionBudget,
    depth: usize,
) -> Result<JsonValue> {
    let path = load_included_json_with_context(include, current_dir, context)?;
    anyhow::ensure!(!active.contains(&path), "cyclic skin json include: {}", path.display());
    anyhow::ensure!(
        budget.files < MAX_EXPANSION_FILES,
        "skin json expansion exceeds maximum files ({MAX_EXPANSION_FILES})"
    );
    budget.files += 1;
    active.push(path.clone());
    let result = expand_json_skin_value_inner(
        load_json_value(&path)?,
        path.parent().unwrap_or(current_dir),
        context,
        enabled_options,
        active,
        budget,
        depth + 1,
    );
    active.pop();
    result
}

fn expand_json_skin_value_inner(
    value: JsonValue,
    current_dir: &Path,
    context: &SkinPathContext,
    enabled_options: &[i32],
    active: &mut Vec<PathBuf>,
    budget: &mut ExpansionBudget,
    depth: usize,
) -> Result<JsonValue> {
    // Count each value's root when attaching it to its parent. This lets array
    // includes flatten without charging a node for the discarded array wrapper.
    anyhow::ensure!(depth < 64, "skin json expansion exceeds maximum depth (64)");
    match value {
        JsonValue::Array(items) => {
            let mut expanded = Vec::new();
            for item in items {
                if let JsonValue::Object(object) = &item {
                    if let Some(include) = object.get("include") {
                        let included_value = expand_include(
                            include,
                            current_dir,
                            context,
                            enabled_options,
                            active,
                            budget,
                            depth,
                        )?;
                        budget.append_included(&mut expanded, included_value)?;
                        continue;
                    }
                    if object.contains_key("if")
                        && (object.contains_key("value") || object.contains_key("values"))
                    {
                        if test_json_option(object.get("if"), enabled_options) {
                            if let Some(value) = object.get("value") {
                                let value = expand_json_skin_value_inner(
                                    value.clone(),
                                    current_dir,
                                    context,
                                    enabled_options,
                                    active,
                                    budget,
                                    depth + 1,
                                )?;
                                budget.add_node()?;
                                expanded.push(value);
                            }
                            if let Some(values) = object.get("values") {
                                let values = expand_json_skin_value_inner(
                                    values.clone(),
                                    current_dir,
                                    context,
                                    enabled_options,
                                    active,
                                    budget,
                                    depth + 1,
                                )?;
                                budget.append_included(&mut expanded, values)?;
                            }
                        }
                        continue;
                    }
                }
                let value = expand_json_skin_value_inner(
                    item,
                    current_dir,
                    context,
                    enabled_options,
                    active,
                    budget,
                    depth + 1,
                )?;
                budget.add_node()?;
                expanded.push(value);
            }
            Ok(JsonValue::Array(expanded))
        }
        JsonValue::Object(mut object) => {
            if let Some(include) = object.get("include") {
                return expand_include(
                    include,
                    current_dir,
                    context,
                    enabled_options,
                    active,
                    budget,
                    depth,
                );
            }
            if object.contains_key("if") && object.contains_key("value") {
                return if test_json_option(object.get("if"), enabled_options) {
                    expand_json_skin_value_inner(
                        object.remove("value").unwrap_or(JsonValue::Null),
                        current_dir,
                        context,
                        enabled_options,
                        active,
                        budget,
                        depth + 1,
                    )
                } else {
                    Ok(JsonValue::Null)
                };
            }
            let mut expanded = JsonMap::new();
            for (key, value) in object {
                let value = expand_json_skin_value_inner(
                    value,
                    current_dir,
                    context,
                    enabled_options,
                    active,
                    budget,
                    depth + 1,
                )?;
                budget.add_node()?;
                expanded.insert(key, value);
            }
            Ok(JsonValue::Object(expanded))
        }
        other => Ok(other),
    }
}

pub fn load_included_json(
    include: &JsonValue,
    current_dir: &Path,
    root_dir: &Path,
) -> Result<PathBuf> {
    load_included_json_with_context(
        include,
        current_dir,
        &SkinPathContext::for_directory(root_dir)?,
    )
}

fn load_included_json_with_context(
    include: &JsonValue,
    current_dir: &Path,
    context: &SkinPathContext,
) -> Result<PathBuf> {
    let include =
        include.as_str().ok_or_else(|| anyhow::anyhow!("skin json include must be a string"))?;
    context.with_base_directory(current_dir)?.resolve_file(include)
}

pub fn test_json_option(option: Option<&JsonValue>, enabled_options: &[i32]) -> bool {
    let Some(option) = option else {
        return true;
    };
    match option {
        JsonValue::Number(number) => number.as_i64().is_some_and(|value| {
            test_json_option_number(i32::try_from(value).unwrap_or(i32::MIN), enabled_options)
        }),
        JsonValue::Array(values) => values.iter().all(|value| match value {
            JsonValue::Number(number) => number.as_i64().is_some_and(|value| {
                test_json_option_number(i32::try_from(value).unwrap_or(i32::MIN), enabled_options)
            }),
            JsonValue::Array(or_values) => or_values.iter().any(|or_value| {
                let JsonValue::Number(number) = or_value else {
                    return false;
                };
                number.as_i64().is_some_and(|value| {
                    test_json_option_number(
                        i32::try_from(value).unwrap_or(i32::MIN),
                        enabled_options,
                    )
                })
            }),
            _ => false,
        }),
        _ => false,
    }
}

pub fn test_json_option_number(option: i32, enabled_options: &[i32]) -> bool {
    if option >= 0 {
        enabled_options.contains(&option)
    } else {
        !enabled_options.contains(&-option)
    }
}

pub fn default_enabled_options(value: &JsonValue) -> Vec<i32> {
    let Some(properties) = value.get("property").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    properties.iter().filter_map(default_property_option).collect()
}

pub fn default_property_option(property: &JsonValue) -> Option<i32> {
    let items = property.get("item")?.as_array()?;
    let default_name = property.get("def").and_then(JsonValue::as_str).unwrap_or_default();
    if let Some(default_item) = items.iter().find(|item| {
        !default_name.is_empty()
            && item.get("name").and_then(JsonValue::as_str).is_some_and(|name| name == default_name)
    }) {
        return default_item
            .get("op")
            .and_then(JsonValue::as_i64)
            .and_then(|op| i32::try_from(op).ok());
    }
    items
        .first()
        .and_then(|item| item.get("op"))
        .and_then(JsonValue::as_i64)
        .and_then(|op| i32::try_from(op).ok())
}

#[cfg(test)]
mod include_tests {
    use super::*;

    #[test]
    fn exponentially_reused_includes_exhaust_the_shared_file_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        for index in 0..30 {
            let child = format!("{}.json", index + 1);
            std::fs::write(
                root.path().join(format!("{index}.json")),
                serde_json::json!([{"include": child}, {"include": child}]).to_string(),
            )?;
        }
        std::fs::write(root.path().join("30.json"), "[0]")?;
        let error = expand_json_skin_value(
            serde_json::json!({"include": "0.json"}),
            root.path(),
            root.path(),
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("maximum files (4096)"), "{error:#}");
        Ok(())
    }

    #[test]
    fn duplicate_includes_count_against_the_file_limit() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("empty.json"), "[]")?;
        let mut includes =
            vec![serde_json::json!({"include": "empty.json"}); MAX_EXPANSION_FILES - 1];
        assert_eq!(
            expand_json_skin_value(
                JsonValue::Array(includes.clone()),
                root.path(),
                root.path(),
                &[],
            )?,
            serde_json::json!([])
        );
        includes.push(serde_json::json!({"include": "empty.json"}));
        assert!(
            expand_json_skin_value(JsonValue::Array(includes), root.path(), root.path(), &[])
                .unwrap_err()
                .to_string()
                .contains("maximum files")
        );
        Ok(())
    }

    #[test]
    fn expanded_node_budget_counts_retained_arrays_objects_and_scalars() -> Result<()> {
        let root = tempfile::tempdir()?;
        // Each object, its array, and the scalar contribute three retained nodes.
        let values = vec![serde_json::json!({"item": [0]}); (MAX_EXPANSION_NODES - 1) / 3];
        std::fs::write(root.path().join("values.json"), serde_json::to_vec(&values)?)?;
        let expanded = expand_json_skin_value(
            serde_json::json!([{"include": "values.json"}]),
            root.path(),
            root.path(),
            &[],
        )?;
        assert_eq!(expanded.as_array().unwrap().len(), values.len());
        assert!(
            expand_json_skin_value(
                serde_json::json!([{"include": "values.json"}, null]),
                root.path(),
                root.path(),
                &[],
            )
            .unwrap_err()
            .to_string()
            .contains("maximum nodes (1000000)")
        );
        Ok(())
    }

    #[test]
    fn includes_reject_cycles_and_depth_but_allow_reuse() {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root =
            std::env::temp_dir().join(format!("bmz-json-include-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let expand = |value| expand_json_skin_value(value, &root, &root, &[]);
        std::fs::write(root.join("a.json"), r#"{"include":"./b.json"}"#).unwrap();
        std::fs::write(root.join("b.json"), r#"[{"include":"a.json"}]"#).unwrap();
        assert!(
            expand(serde_json::json!({"include":"a.json"}))
                .unwrap_err()
                .to_string()
                .contains("cyclic")
        );
        std::fs::write(root.join("b.json"), "[1,2]").unwrap();
        assert_eq!(
            expand(serde_json::json!([{"include":"a.json"},{"include":"a.json"}])).unwrap(),
            serde_json::json!([1, 2, 1, 2])
        );
        for i in 0..65 {
            std::fs::write(
                root.join(format!("depth{i}.json")),
                format!(r#"{{"include":"depth{}.json"}}"#, i + 1),
            )
            .unwrap();
        }
        assert!(
            expand(serde_json::json!({"include":"depth0.json"}))
                .unwrap_err()
                .to_string()
                .contains("maximum depth")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
