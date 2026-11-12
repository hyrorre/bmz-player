use super::*;

mod function_field;

use function_field::{ObjectFunctionMetadata, handle_function_field};

pub(super) fn lua_value_to_log_string(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.to_string_lossy(),
        Value::Table(_) => "<table>".to_string(),
        Value::Function(_) => "<function>".to_string(),
        Value::Thread(_) => "<thread>".to_string(),
        Value::UserData(_) => "<userdata>".to_string(),
        Value::LightUserData(_) => "<lightuserdata>".to_string(),
        Value::Error(error) => format!("<error:{error}>"),
        Value::Other(_) => "<other>".to_string(),
    }
}

pub(super) fn infer_m_select_result_graph_height_expr(path: &str) -> Option<String> {
    const DESTINATION_FIRST: i64 = 40;
    const FAST_SLOW_REFS: [i32; 12] = [422, 419, 417, 415, 413, 411, 410, 412, 414, 416, 418, 421];
    let destination_index = lua_path_array_index(path, "$.destination[")?;
    let dst_index = lua_path_array_index(path, "].dst[")?;
    if dst_index != 3 {
        return None;
    }
    let ref_index = usize::try_from(destination_index - DESTINATION_FIRST).ok()?;
    let ref_id = *FAST_SLOW_REFS.get(ref_index)?;
    Some(format!("{SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT}({ref_id})"))
}

pub(super) fn lua_path_array_index(path: &str, marker: &str) -> Option<i64> {
    let (_, rest) = path.split_once(marker)?;
    let (index, _) = rest.split_once(']')?;
    index.parse().ok()
}

pub(super) fn lua_value_to_json(
    lua: &Lua,
    value: Value,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    instruction_budget: &LuaInstructionBudget,
    table_budget: &mut TableBudget,
) -> Result<JsonValue> {
    if depth > LUA_MAX_TABLE_DEPTH {
        bail!("lua table nesting is too deep at {path}");
    }

    Ok(match value {
        Value::Nil => JsonValue::Null,
        Value::Boolean(value) => JsonValue::Bool(value),
        Value::Integer(value) => JsonValue::Number(JsonNumber::from(value)),
        Value::Number(value) => match JsonNumber::from_f64(value) {
            Some(number) => JsonValue::Number(number),
            None => {
                warnings.push(format!("non-finite lua number converted to 0 at {path}"));
                JsonValue::Number(JsonNumber::from(0))
            }
        },
        Value::String(value) => {
            let bytes = value.as_bytes().to_vec();
            table_budget.consume_string_bytes(&bytes, path)?;
            JsonValue::String(lossy_utf8_string(bytes))
        }
        Value::Table(table) => lua_table_to_json(
            lua,
            table,
            path,
            depth + 1,
            warnings,
            main_state_probe,
            instruction_budget,
            table_budget,
        )?,
        Value::Function(_) => {
            warnings.push(format!("skipping function at {path}"));
            JsonValue::Null
        }
        Value::Thread(_) => {
            warnings.push(format!("skipping thread at {path}"));
            JsonValue::Null
        }
        Value::UserData(_) | Value::LightUserData(_) => {
            warnings.push(format!("skipping userdata at {path}"));
            JsonValue::Null
        }
        Value::Error(error) => {
            warnings.push(format!("skipping lua error value at {path}: {error}"));
            JsonValue::Null
        }
        Value::Other(_) => {
            warnings.push(format!("skipping unsupported lua value at {path}"));
            JsonValue::Null
        }
    })
}

pub(super) fn peaceful_gauge_lead_glow_id(id: &str) -> Option<(&str, bool)> {
    let (group, side) = id.strip_prefix("gauge-lead-glow-")?.rsplit_once('-')?;
    if !matches!(group, "assist_easy" | "easy" | "groove" | "hard" | "exhard" | "hazard") {
        return None;
    }
    Some((
        group,
        match side {
            "above" => false,
            "below" => true,
            _ => return None,
        },
    ))
}

pub(super) fn is_peaceful_gauge_lead_glow_destination(entries: &[LuaTableEntry]) -> bool {
    let Some(Value::Table(dst)) = entries
        .iter()
        .find_map(|entry| (entry.string_key() == Some(b"dst".as_slice())).then_some(&entry.value))
    else {
        return false;
    };
    let frames =
        [1, 2, 3].into_iter().map(|index| dst.get::<Table>(index).ok()).collect::<Option<Vec<_>>>();
    let Some(frames) = frames else { return false };
    let expected = [(0, 0), (750, 255), (1500, 0)];
    let rect = frames[0]
        .get::<f64>("x")
        .ok()
        .zip(frames[0].get::<f64>("y").ok())
        .zip(frames[0].get::<f64>("w").ok())
        .zip(frames[0].get::<f64>("h").ok());
    frames.iter().zip(expected).all(|(frame, (time, alpha))| {
        frame.get::<i32>("time").ok() == Some(time)
            && frame.get::<i32>("a").ok() == Some(alpha)
            && frame
                .get::<f64>("x")
                .ok()
                .zip(frame.get::<f64>("y").ok())
                .zip(frame.get::<f64>("w").ok())
                .zip(frame.get::<f64>("h").ok())
                == rect
    })
}

/// A table key read from the VM once, so sorting and naming it need no further
/// Lua calls (each one takes the VM lock).
pub(in crate::lua) enum LuaTableKey {
    Integer(i64),
    Number(f64),
    String(Vec<u8>),
    Other(Value),
}

impl LuaTableKey {
    fn from_value(key: Value) -> Self {
        match key {
            Value::Integer(key) => Self::Integer(key),
            Value::Number(key) => Self::Number(key),
            Value::String(key) => Self::String(key.as_bytes().to_vec()),
            key => Self::Other(key),
        }
    }

    /// Integer keys by value, then other numbers, then strings by bytes.
    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(key: &LuaTableKey) -> u8 {
            match key {
                LuaTableKey::Integer(_) => 0,
                LuaTableKey::Number(_) => 1,
                LuaTableKey::String(_) => 2,
                LuaTableKey::Other(_) => 3,
            }
        }
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => left.cmp(right),
            (Self::Number(left), Self::Number(right)) => left.total_cmp(right),
            (Self::String(left), Self::String(right)) => left.cmp(right),
            _ => rank(self).cmp(&rank(other)),
        }
    }
}

pub(in crate::lua) struct LuaTableEntry {
    pub(in crate::lua) key: LuaTableKey,
    pub(in crate::lua) value: Value,
}

impl LuaTableEntry {
    pub(in crate::lua) fn string_key(&self) -> Option<&[u8]> {
        match &self.key {
            LuaTableKey::String(key) => Some(key),
            _ => None,
        }
    }
}

/// The same text as `mlua::String::to_string_lossy`, without copying valid UTF-8.
fn lossy_utf8_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

pub(super) fn lua_table_to_json(
    lua: &Lua,
    table: Table,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    instruction_budget: &LuaInstructionBudget,
    table_budget: &mut TableBudget,
) -> Result<JsonValue> {
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        table_budget.consume(1, path)?;
        let (key, value) = pair?;
        entries.push(LuaTableEntry { key: LuaTableKey::from_value(key), value });
    }
    // `pairs` order follows a per-VM hash seed. Inference calls skin functions
    // while converting, and those calls can write globals that later probes
    // read, so convert fields in a fixed order. It also keeps callback IDs stable.
    entries.sort_by(|left, right| left.key.compare(&right.key));

    if entries.is_empty() {
        return Ok(JsonValue::Array(Vec::new()));
    }

    let mut integer_keys = Vec::new();
    let mut has_non_integer_key = false;
    for entry in &entries {
        if matches!(entry.value, Value::Nil) {
            continue;
        }
        match entry.key {
            LuaTableKey::Integer(index) if index > 0 => integer_keys.push(index),
            _ => has_non_integer_key = true,
        }
    }
    integer_keys.sort_unstable();
    let is_array = !has_non_integer_key
        && integer_keys.iter().enumerate().all(|(offset, index)| *index == offset as i64 + 1);

    if is_array {
        let mut values = Vec::new();
        entries.sort_by_key(|entry| match entry.key {
            LuaTableKey::Integer(index) => index,
            _ => i64::MAX,
        });
        for (index, LuaTableEntry { value, .. }) in entries.into_iter().enumerate() {
            // LuaSkinLoader's DestinationOption serializer ignores non-scalar
            // entries (a nested table is not an OR group). In particular, do
            // not let SocialSkin's legacy nested options reject the document.
            if path.ends_with(".op")
                && matches!(value, Value::Table(_) | Value::Boolean(_) | Value::Nil)
            {
                continue;
            }
            values.push(lua_value_to_json(
                lua,
                value,
                &format!("{path}[{}]", index + 1),
                depth,
                warnings,
                main_state_probe,
                instruction_budget,
                table_budget,
            )?);
        }
        return Ok(JsonValue::Array(values));
    }

    if !integer_keys.is_empty() {
        warnings.push(format!("mixed lua table converted to object at {path}"));
    }
    let function_metadata = ObjectFunctionMetadata::from_entries(&entries, path, main_state_probe);
    let mut object = JsonMap::new();
    for LuaTableEntry { key, value } in entries {
        let key = match key {
            LuaTableKey::String(key) => {
                table_budget.consume_string_bytes(&key, path)?;
                lossy_utf8_string(key)
            }
            LuaTableKey::Integer(key) => key.to_string(),
            LuaTableKey::Number(key) => key.to_string(),
            LuaTableKey::Other(key) => lua_key_to_json_key(key, path, warnings)?,
        };
        if matches!(value, Value::Nil) {
            continue;
        }
        // LuaSkinLoader maps these header fields through LuaValue.toint() /
        // tojstring(). Keep this coercion at the Lua boundary so ordinary JSON
        // documents remain strict, and header and full-document loads agree.
        if key == "op" && is_lua_property_item_path(path) {
            let number = lua.coerce_number(value.clone())?.unwrap_or(0.0) as i32;
            object.insert(key, JsonValue::from(number));
            continue;
        }
        if key == "def"
            && ["$.property[", "$.filepath["].iter().any(|prefix| is_lua_index_path(path, prefix))
            && matches!(value, Value::Integer(_) | Value::Number(_) | Value::Boolean(_))
        {
            object.insert(key, JsonValue::String(lua_value_to_log_string(&value)));
            continue;
        }
        // LuaSkinLoader.serializeLuaScript maps a Lua boolean to null, and a null
        // `draw` adds no condition. ModernChic's `draw = not func` is `false` and
        // still draws.
        if key == "draw" && matches!(value, Value::Boolean(_)) {
            tracing::debug!(field_path = %format!("{path}.{key}"), classification = "STATIC", "ignored Lua boolean draw");
            continue;
        }
        let value = if (matches!(key.as_str(), "act" | "timer")
            || (key == "event" && path.starts_with("$.slider[")))
            && matches!(value, Value::String(_))
        {
            if let Some(number) = lua.coerce_number(value.clone())? {
                Value::Integer(i64::from(number as i32))
            } else {
                match lua_script_field_function(lua, value, &key) {
                    Ok(Some(function)) => Value::Function(function),
                    Ok(None) => continue,
                    Err(error) => {
                        warnings.push(format!("invalid Lua script at {path}.{key}: {error}"));
                        continue;
                    }
                }
            }
        } else {
            value
        };
        if let Value::Function(function) = &value
            && handle_function_field(
                lua,
                function,
                &key,
                path,
                &function_metadata,
                &mut object,
                warnings,
                main_state_probe,
                instruction_budget,
            )?
        {
            continue;
        }
        if is_unsupported_json_field_value(&value) {
            if should_silently_skip_loader_field(&key, &value) {
                continue;
            }
            warnings.push(format!("skipping unsupported field `{key}` at {path}"));
            continue;
        }
        if key == "h"
            && let Value::Number(number) = &value
            && !number.is_finite()
            && let Some(expr) = infer_m_select_result_graph_height_expr(path)
        {
            object.insert(key.clone(), JsonValue::Number(JsonNumber::from(0)));
            object.insert("h_expr".to_string(), JsonValue::String(expr));
            continue;
        }
        object.insert(
            key.clone(),
            lua_value_to_json(
                lua,
                value,
                &format!("{path}.{key}"),
                depth,
                warnings,
                main_state_probe,
                instruction_budget,
                table_budget,
            )?,
        );
    }
    repair_result_table_title_text(path, &mut object);
    Ok(JsonValue::Object(object))
}

fn is_lua_index_path(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .and_then(|index| index.strip_suffix(']'))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_lua_property_item_path(path: &str) -> bool {
    let Some((property, item)) = path.split_once(".item[") else { return false };
    is_lua_index_path(property, "$.property[") && is_lua_index_path(item, "")
}

/// The same script forms as SkinLuaAccessor.loadEvent / loadTimerProperty.
/// Called independently in the conversion and clean runtime VMs.
pub(super) fn lua_script_field_function(
    lua: &Lua,
    value: Value,
    field: &str,
) -> mlua::Result<Option<Function>> {
    match value {
        Value::Function(function) => Ok(Some(function)),
        Value::String(script) if lua.coerce_number(Value::String(script.clone()))?.is_some() => {
            Ok(None)
        }
        Value::String(script) if matches!(field, "act" | "event") => lua
            .load(script.to_str()?.as_ref())
            .set_name("Lua act script")
            .set_mode(ChunkMode::Text)
            .into_function()
            .map(Some),
        Value::String(script) if field == "timer" => {
            let function = lua
                .load(format!("return {}", script.to_str()?))
                .set_name("Lua timer script")
                .set_mode(ChunkMode::Text)
                .into_function()?;
            match function.call::<Value>(())? {
                Value::Function(timer) => Ok(Some(timer)),
                _ => Ok(Some(function)),
            }
        }
        _ => Ok(None),
    }
}
