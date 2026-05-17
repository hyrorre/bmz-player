use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use mlua::{Lua, Table, Value, Variadic};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = CliArgs::parse(std::env::args().skip(1))?;
    match args.command {
        Command::LuaToJson { input, output, options } => {
            let report = convert_lua_skin_to_json(&input, &output, &options)?;
            for warning in report.warnings {
                eprintln!("warning: {warning}");
            }
            eprintln!("converted {} -> {}", input.display(), output.display());
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    LuaToJson { input: PathBuf, output: PathBuf, options: BTreeMap<String, String> },
}

impl CliArgs {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            bail!("{}", help_text());
        };
        if command == "-h" || command == "--help" {
            bail!("{}", help_text());
        }
        if command != "lua-to-json" {
            bail!("unknown command `{command}`\n{}", help_text());
        }

        let Some(input) = args.next() else {
            bail!("lua-to-json requires an input .luaskin path");
        };
        let mut output = None;
        let mut options = BTreeMap::new();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => {
                    let Some(path) = args.next() else {
                        bail!("--out requires a path");
                    };
                    output = Some(PathBuf::from(path));
                }
                "--option" => {
                    let Some(option) = args.next() else {
                        bail!("--option requires key=value");
                    };
                    let (key, value) = parse_option_pair(&option)?;
                    options.insert(key, value);
                }
                _ if arg.starts_with("--out=") => {
                    output = Some(PathBuf::from(arg.trim_start_matches("--out=")));
                }
                _ if arg.starts_with("--option=") => {
                    let (key, value) = parse_option_pair(arg.trim_start_matches("--option="))?;
                    options.insert(key, value);
                }
                _ => bail!("unknown argument `{arg}`"),
            }
        }

        let Some(output) = output else {
            bail!("lua-to-json requires --out <path>");
        };

        Ok(Self { command: Command::LuaToJson { input: PathBuf::from(input), output, options } })
    }
}

fn help_text() -> &'static str {
    "usage: bmz-skin-convert lua-to-json <input.luaskin> --out <output.json> [--option key=value]"
}

fn parse_option_pair(input: &str) -> Result<(String, String)> {
    let Some((key, value)) = input.split_once('=') else {
        bail!("option `{input}` must be key=value");
    };
    let key = key.trim();
    if key.is_empty() {
        bail!("option key must not be empty");
    }
    Ok((key.to_string(), value.trim().to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConvertReport {
    warnings: Vec<String>,
}

fn convert_lua_skin_to_json(
    input: &Path,
    output: &Path,
    options: &BTreeMap<String, String>,
) -> Result<ConvertReport> {
    let input = input
        .canonicalize()
        .with_context(|| format!("failed to canonicalize input: {}", input.display()))?;
    let root = input
        .parent()
        .ok_or_else(|| anyhow!("input path has no parent: {}", input.display()))?
        .canonicalize()
        .with_context(|| format!("failed to canonicalize skin root: {}", input.display()))?;

    let mut warnings = Vec::new();
    let source = fs::read_to_string(&input)
        .with_context(|| format!("failed to read lua skin: {}", input.display()))?;

    let header_lua = Lua::new();
    install_sandbox(&header_lua, &root, options, None)?;
    let header = header_lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    let header_json = lua_value_to_json(header, "$", 0, &mut warnings)?;
    let skin_options = skin_config_options_from_header(&header_json, options, &mut warnings);

    let lua = Lua::new();
    install_sandbox(&lua, &root, options, Some(&skin_options))?;
    let value = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin: {}", input.display()))?;
    let json = lua_value_to_json(value, "$", 0, &mut warnings)?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output dir: {}", parent.display()))?;
    }
    fs::write(output, serde_json::to_string_pretty(&json)? + "\n")
        .with_context(|| format!("failed to write json skin: {}", output.display()))?;

    Ok(ConvertReport { warnings })
}

fn install_sandbox(
    lua: &Lua,
    root: &Path,
    options: &BTreeMap<String, String>,
    skin_config_options: Option<&BTreeMap<String, i64>>,
) -> Result<()> {
    let globals = lua.globals();
    if let Some(skin_config_options) = skin_config_options {
        let skin_config = lua.create_table()?;
        let option = lua.create_table()?;
        for (key, value) in skin_config_options {
            option.set(key.as_str(), *value)?;
        }
        skin_config.set("option", option)?;
        globals.set("skin_config", skin_config)?;
    }
    globals.set("os", Value::Nil)?;
    globals.set("io", Value::Nil)?;
    globals.set("debug", Value::Nil)?;
    if let Ok(package) = globals.get::<Table>("package") {
        package.set("loadlib", Value::Nil)?;
    }

    let print = lua.create_function(|_, args: Variadic<Value>| {
        let parts =
            args.into_iter().map(|value| lua_value_to_log_string(&value)).collect::<Vec<_>>();
        eprintln!("lua: {}", parts.join("\t"));
        Ok(())
    })?;
    globals.set("print", print)?;

    let option_table = lua.create_table()?;
    for (key, value) in options {
        option_table.set(key.as_str(), value.as_str())?;
    }
    let bmz = lua.create_table()?;
    bmz.set("option", option_table.clone())?;
    let options_for_getter = options.clone();
    let get_option = lua.create_function(move |_, (name, default): (String, Option<String>)| {
        Ok(options_for_getter.get(&name).cloned().or(default).unwrap_or_default())
    })?;
    bmz.set("get_option", get_option)?;
    globals.set("bmz", bmz)?;

    let sandbox_root = root.to_path_buf();
    let root_for_dofile = sandbox_root.clone();
    let dofile = lua.create_function(move |lua, path: String| {
        let path =
            resolve_lua_path(&root_for_dofile, &path, false).map_err(mlua::Error::external)?;
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).eval::<Value>()
    })?;
    globals.set("dofile", dofile)?;

    let root_for_loadfile = sandbox_root.clone();
    let loadfile = lua.create_function(move |lua, path: String| {
        let path =
            resolve_lua_path(&root_for_loadfile, &path, false).map_err(mlua::Error::external)?;
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).into_function()
    })?;
    globals.set("loadfile", loadfile)?;

    let root = sandbox_root;
    let require = lua.create_function(move |lua, module: String| {
        if module == "main_state" {
            return create_main_state_stub(lua);
        }
        let globals = lua.globals();
        let package: Table = globals.get("package")?;
        let loaded: Table = package.get("loaded")?;
        if let Ok(cached) = loaded.get::<Value>(module.as_str())
            && !matches!(cached, Value::Nil)
        {
            return Ok(cached);
        }

        let path = resolve_lua_path(&root, &module, true).map_err(mlua::Error::external)?;
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        let value = lua.load(&source).set_name(path.to_string_lossy().as_ref()).eval::<Value>()?;
        let value = if matches!(value, Value::Nil) { Value::Boolean(true) } else { value };
        loaded.set(module, value.clone())?;
        Ok(value)
    })?;
    globals.set("require", require)?;

    Ok(())
}

fn create_main_state_stub(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("timer_off_value", i32::MIN)?;
    table.set("number", lua.create_function(|_, _: i32| Ok(0))?)?;
    table.set("option", lua.create_function(|_, _: i32| Ok(false))?)?;
    table.set("text", lua.create_function(|_, _: i32| Ok(String::new()))?)?;
    table.set("timer", lua.create_function(|_, _: i32| Ok(i32::MIN))?)?;
    table.set("gauge_type", lua.create_function(|_, ()| Ok(0))?)?;
    Ok(Value::Table(table))
}

fn skin_config_options_from_header(
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

fn option_value_to_op(items: &[JsonValue], value: &str) -> Option<i64> {
    if let Ok(op) = value.parse::<i64>() {
        return Some(op);
    }
    items.iter().find_map(|item| {
        (item.get("name").and_then(JsonValue::as_str) == Some(value))
            .then(|| item.get("op").and_then(JsonValue::as_i64))
            .flatten()
    })
}

fn default_property_op(property: &JsonValue, items: &[JsonValue]) -> Option<i64> {
    if let Some(default_name) = property.get("def").and_then(JsonValue::as_str)
        && let Some(op) = option_value_to_op(items, default_name)
    {
        return Some(op);
    }
    items.first().and_then(|item| item.get("op")).and_then(JsonValue::as_i64)
}

fn resolve_lua_path(root: &Path, requested: &str, module: bool) -> Result<PathBuf> {
    let relative = if module { requested.replace('.', "/") } else { requested.to_string() };
    let relative_path = Path::new(&relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        bail!("lua path escapes skin root: {requested}");
    }
    let candidates = if module {
        vec![format!("{relative}.lua"), format!("{relative}/init.lua")]
    } else if relative.ends_with(".lua") || relative.ends_with(".luaskin") {
        vec![relative]
    } else {
        vec![relative.clone(), format!("{relative}.lua")]
    };

    for candidate in candidates {
        let path = root.join(candidate);
        if path.is_file() {
            let canonical = path.canonicalize()?;
            if !canonical.starts_with(root) {
                bail!("lua path escapes skin root: {}", canonical.display());
            }
            return Ok(canonical);
        }
    }

    bail!("lua file not found: {requested}");
}

fn lua_value_to_log_string(value: &Value) -> String {
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

fn lua_value_to_json(
    value: Value,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
) -> Result<JsonValue> {
    if depth > 64 {
        bail!("lua table nesting is too deep at {path}");
    }

    Ok(match value {
        Value::Nil => JsonValue::Null,
        Value::Boolean(value) => JsonValue::Bool(value),
        Value::Integer(value) => JsonValue::Number(JsonNumber::from(value)),
        Value::Number(value) => {
            JsonNumber::from_f64(value).map(JsonValue::Number).ok_or_else(|| {
                anyhow!("non-finite lua number cannot be represented as JSON at {path}")
            })?
        }
        Value::String(value) => JsonValue::String(value.to_string_lossy()),
        Value::Table(table) => lua_table_to_json(table, path, depth + 1, warnings)?,
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

fn lua_table_to_json(
    table: Table,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
) -> Result<JsonValue> {
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        entries.push(pair?);
    }

    if entries.is_empty() {
        return Ok(JsonValue::Array(Vec::new()));
    }

    let mut integer_keys = Vec::new();
    let mut has_non_integer_key = false;
    for (key, _) in &entries {
        match key {
            Value::Integer(index) if *index > 0 => integer_keys.push(*index),
            _ => has_non_integer_key = true,
        }
    }
    integer_keys.sort_unstable();
    let is_array = !has_non_integer_key
        && integer_keys.iter().enumerate().all(|(offset, index)| *index == offset as i64 + 1);

    if is_array {
        let mut values = Vec::new();
        entries.sort_by_key(|(key, _)| match key {
            Value::Integer(index) => *index,
            _ => i64::MAX,
        });
        for (index, (_, value)) in entries.into_iter().enumerate() {
            values.push(lua_value_to_json(
                value,
                &format!("{path}[{}]", index + 1),
                depth,
                warnings,
            )?);
        }
        return Ok(JsonValue::Array(values));
    }

    if !integer_keys.is_empty() {
        warnings.push(format!("mixed lua table converted to object at {path}"));
    }
    let mut object = JsonMap::new();
    for (key, value) in entries {
        let key = lua_key_to_json_key(key, path, warnings)?;
        if matches!(value, Value::Nil) {
            continue;
        }
        if is_unsupported_json_field_value(&value) {
            warnings.push(format!("skipping unsupported field `{key}` at {path}"));
            continue;
        }
        object.insert(
            key.clone(),
            lua_value_to_json(value, &format!("{path}.{key}"), depth, warnings)?,
        );
    }
    Ok(JsonValue::Object(object))
}

fn is_unsupported_json_field_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Function(_)
            | Value::Thread(_)
            | Value::UserData(_)
            | Value::LightUserData(_)
            | Value::Error(_)
            | Value::Other(_)
    )
}

fn lua_key_to_json_key(key: Value, path: &str, warnings: &mut Vec<String>) -> Result<String> {
    match key {
        Value::String(value) => Ok(value.to_string_lossy()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => {
            warnings.push(format!("unsupported table key converted with debug fallback at {path}"));
            Ok(lua_value_to_log_string(&key))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn cli_parses_lua_to_json_options() {
        let args = CliArgs::parse([
            "lua-to-json".to_string(),
            "skin.luaskin".to_string(),
            "--out".to_string(),
            "skin.json".to_string(),
            "--option".to_string(),
            "Play Side=1P".to_string(),
        ])
        .unwrap();

        assert_eq!(
            args,
            CliArgs {
                command: Command::LuaToJson {
                    input: PathBuf::from("skin.luaskin"),
                    output: PathBuf::from("skin.json"),
                    options: BTreeMap::from([("Play Side".to_string(), "1P".to_string())])
                }
            }
        );
    }

    #[test]
    fn converts_lua_table_to_json_with_options_and_require() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("parts.lua"), "return { id = 'panel', src = 1 }").unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local parts = require("parts")
            return {
                type = 0,
                name = bmz.option["Play Side"],
                image = { parts },
                destination = {
                    { id = parts.id, dst = {{ x = 10, y = 20, w = 30, h = 40 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report = convert_lua_skin_to_json(
            &root.join("play7.luaskin"),
            &output,
            &BTreeMap::from([("Play Side".to_string(), "1P".to_string())]),
        )
        .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["name"], "1P");
        assert_eq!(json["image"][0]["id"], "panel");
        assert_eq!(json["destination"][0]["dst"][0]["w"], 30);
    }

    #[test]
    fn converts_lua_skin_with_main_state_stub() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 5,
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

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("select.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.iter().any(|warning| warning.contains("unsupported field")));
        assert_eq!(json["type"], 5);
        assert!(json["value"][0].get("value").is_none());
        assert!(json["destination"][0].get("draw").is_none());
    }

    #[test]
    fn rejects_lua_paths_outside_skin_root() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("play7.luaskin"), "return dofile('../outside.lua')").unwrap();
        fs::write(root.parent().unwrap().join("outside.lua"), "return {}").unwrap();

        let err = convert_lua_skin_to_json(
            &root.join("play7.luaskin"),
            &root.join("out.json"),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("escapes skin root"));
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
    }
}
