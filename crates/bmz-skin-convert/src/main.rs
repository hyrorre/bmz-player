use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use mlua::{Function, Lua, Table, Value, Variadic};
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
    let header_probe = install_sandbox(&header_lua, &root, options, None)?;
    let header = header_lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    let header_json = lua_value_to_json(header, "$", 0, &mut warnings, &header_probe)?;
    let skin_options = skin_config_options_from_header(&header_json, options, &mut warnings);

    let lua = Lua::new();
    let main_state_probe = install_sandbox(&lua, &root, options, Some(&skin_options))?;
    let value = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin: {}", input.display()))?;
    let json = lua_value_to_json(value, "$", 0, &mut warnings, &main_state_probe)?;

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
) -> Result<Arc<Mutex<MainStateProbe>>> {
    let main_state_probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let globals = lua.globals();
    if let Some(skin_config_options) = skin_config_options {
        let skin_config = lua.create_table()?;
        let option = lua.create_table()?;
        for (key, value) in skin_config_options {
            option.set(key.as_str(), *value)?;
        }
        skin_config.set("option", option)?;
        let root_for_get_path = root.to_path_buf();
        let get_path = lua.create_function(move |_, requested: String| {
            skin_config_get_path(&root_for_get_path, &requested)
                .map(|path| path.to_string_lossy().to_string())
                .map_err(mlua::Error::external)
        })?;
        skin_config.set("get_path", get_path)?;
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
    let probe_for_require = main_state_probe.clone();
    let require = lua.create_function(move |lua, module: String| {
        if module == "main_state" {
            return create_main_state_stub(lua, probe_for_require.clone());
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

    Ok(main_state_probe)
}

#[derive(Debug, Clone, Default)]
struct MainStateProbe {
    mode: MainStateProbeMode,
    number_calls: Vec<i32>,
    number_values: BTreeMap<i32, i32>,
    option_calls: Vec<i32>,
    option_values: BTreeMap<i32, bool>,
    timer_calls: Vec<i32>,
    timer_values: BTreeMap<i32, i32>,
    gauge_type_calls: usize,
    gauge_type_value: i32,
}

#[derive(Debug, Clone, Default)]
enum MainStateProbeMode {
    #[default]
    RuntimeStub,
    SymbolicNumbers {
        base_value: i32,
    },
    RecordNumbers {
        default_value: i32,
    },
}

impl MainStateProbe {
    fn begin_number_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::SymbolicNumbers { base_value: default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn begin_number_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn begin_number_recording_with_value(&mut self, ref_id: i32, value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.number_values.insert(ref_id, value);
    }

    fn begin_number_recording_with_values(&mut self, values: BTreeMap<i32, i32>) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values = values;
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn begin_option_call_recording(&mut self, default_value: bool) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.option_values.insert(i32::MIN, default_value);
    }

    fn begin_option_recording_with_value(&mut self, option_id: i32, value: bool) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.option_values.insert(option_id, value);
    }

    fn begin_timer_option_call_recording(&mut self) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.option_values.insert(i32::MIN, true);
        self.timer_values.insert(i32::MIN, i32::MIN);
    }

    fn begin_timer_option_recording_with_values(
        &mut self,
        timer_id: i32,
        timer_value: i32,
        option_id: i32,
        option_value: bool,
    ) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.timer_values.insert(timer_id, timer_value);
        self.option_values.insert(option_id, option_value);
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn begin_gauge_type_call_recording(&mut self, value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = value;
    }

    fn begin_gauge_type_recording_with_value(&mut self, value: i32) {
        self.begin_gauge_type_call_recording(value);
    }

    fn end_recording(&mut self) {
        self.mode = MainStateProbeMode::RuntimeStub;
        self.number_values.clear();
        self.option_values.clear();
        self.timer_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn number(&mut self, ref_id: i32) -> i32 {
        match self.mode {
            MainStateProbeMode::RuntimeStub => 0,
            MainStateProbeMode::SymbolicNumbers { base_value } => {
                self.number_calls.push(ref_id);
                self.number_values.get(&ref_id).copied().unwrap_or(base_value + ref_id)
            }
            MainStateProbeMode::RecordNumbers { default_value } => {
                self.number_calls.push(ref_id);
                self.number_values.get(&ref_id).copied().unwrap_or(default_value)
            }
        }
    }

    fn option(&mut self, option_id: i32) -> bool {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return false;
        }
        self.option_calls.push(option_id);
        self.option_values
            .get(&option_id)
            .copied()
            .or_else(|| self.option_values.get(&i32::MIN).copied())
            .unwrap_or(false)
    }

    fn timer(&mut self, timer_id: i32) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return i32::MIN;
        }
        self.timer_calls.push(timer_id);
        self.timer_values
            .get(&timer_id)
            .copied()
            .or_else(|| self.timer_values.get(&i32::MIN).copied())
            .unwrap_or(i32::MIN)
    }

    fn gauge_type(&mut self) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 0;
        }
        self.gauge_type_calls += 1;
        self.gauge_type_value
    }
}

fn create_main_state_stub(lua: &Lua, probe: Arc<Mutex<MainStateProbe>>) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("timer_off_value", i32::MIN)?;
    let probe_for_number = probe.clone();
    table.set(
        "number",
        lua.create_function(move |_, ref_id: i32| {
            Ok(probe_for_number
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .number(ref_id))
        })?,
    )?;
    let probe_for_option = probe.clone();
    let probe_for_timer = probe.clone();
    table.set(
        "option",
        lua.create_function(move |_, option_id: i32| {
            Ok(probe_for_option
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .option(option_id))
        })?,
    )?;
    table.set("text", lua.create_function(|_, _: i32| Ok(String::new()))?)?;
    table.set(
        "timer",
        lua.create_function(move |_, timer_id: i32| {
            Ok(probe_for_timer
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .timer(timer_id))
        })?,
    )?;
    let probe_for_gauge_type = probe;
    table.set(
        "gauge_type",
        lua.create_function(move |_, ()| {
            Ok(probe_for_gauge_type
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .gauge_type())
        })?,
    )?;
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

fn skin_config_get_path(root: &Path, requested: &str) -> Result<PathBuf> {
    let relative_path = Path::new(requested);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        bail!("skin_config.get_path escapes skin root: {requested}");
    }

    let Some((prefix, suffix)) = requested.split_once('*') else {
        return Ok(root.join(requested));
    };
    if suffix.contains('*') {
        bail!("skin_config.get_path supports only one wildcard: {requested}");
    }

    let prefix_path = Path::new(prefix);
    let (dir, name_prefix) = if prefix.ends_with('/') || prefix.ends_with('\\') {
        (root.join(prefix_path), String::new())
    } else {
        (
            root.join(prefix_path.parent().unwrap_or_else(|| Path::new(""))),
            prefix_path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
    };
    let suffix = suffix.replace('\\', "/");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("failed to read skin_config path dir: {}", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let candidate_relative = if prefix.ends_with('/') || prefix.ends_with('\\') {
            format!("{prefix}{name}{suffix}")
        } else {
            let parent = prefix_path.parent().unwrap_or_else(|| Path::new(""));
            let parent = parent.to_string_lossy();
            if parent.is_empty() {
                format!("{name_prefix}{name}{suffix}")
            } else {
                format!("{parent}/{name_prefix}{name}{suffix}")
            }
        };
        let candidate = root.join(candidate_relative);
        if candidate.exists() {
            candidates.push(candidate);
        }
    }
    candidates.sort();
    candidates.into_iter().next().ok_or_else(|| anyhow!("skin_config path not found: {requested}"))
}

fn resolve_lua_path(root: &Path, requested: &str, module: bool) -> Result<PathBuf> {
    let relative = if module { requested.replace('.', "/") } else { requested.to_string() };
    let relative_path = Path::new(&relative);
    if relative_path.is_absolute() {
        let canonical = relative_path.canonicalize()?;
        if canonical.starts_with(root) {
            return Ok(canonical);
        }
        bail!("lua path escapes skin root: {requested}");
    }
    if relative_path.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }) {
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
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
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
        Value::Table(table) => {
            lua_table_to_json(table, path, depth + 1, warnings, main_state_probe)?
        }
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
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
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
                main_state_probe,
            )?);
        }
        return Ok(JsonValue::Array(values));
    }

    if !integer_keys.is_empty() {
        warnings.push(format!("mixed lua table converted to object at {path}"));
    }
    let object_id = lua_object_id(&entries);
    let mut object = JsonMap::new();
    for (key, value) in entries {
        let key = lua_key_to_json_key(key, path, warnings)?;
        if matches!(value, Value::Nil) {
            continue;
        }
        if let Value::Function(function) = &value {
            if key == "value" {
                if let Some(ref_id) = infer_main_state_number_ref(function, main_state_probe) {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                } else if let Some(expr) = infer_main_state_number_expr(function, main_state_probe)
                {
                    object.insert("expr".to_string(), JsonValue::String(expr));
                } else {
                    warnings.push(format!("skipping unsupported value function at {path}.{key}"));
                }
                continue;
            }
            if key == "draw" {
                if let Some(draw) = infer_main_state_draw_condition(function, main_state_probe)
                    .or_else(|| {
                        infer_main_state_timer_option_draw_condition(function, main_state_probe)
                    })
                    .or_else(|| {
                        infer_main_state_gauge_type_draw_condition(function, main_state_probe)
                    })
                    .or_else(|| {
                        infer_judge_fast_slow_draw_condition(
                            function,
                            main_state_probe,
                            object_id.as_deref(),
                        )
                    })
                    .or_else(|| infer_main_state_option_draw_condition(function, main_state_probe))
                {
                    object.insert(key.clone(), JsonValue::String(draw));
                } else {
                    warnings.push(format!("skipping unsupported draw function at {path}.{key}"));
                }
                continue;
            }
        }
        if is_unsupported_json_field_value(&value) {
            warnings.push(format!("skipping unsupported field `{key}` at {path}"));
            continue;
        }
        object.insert(
            key.clone(),
            lua_value_to_json(value, &format!("{path}.{key}"), depth, warnings, main_state_probe)?,
        );
    }
    Ok(JsonValue::Object(object))
}

fn lua_object_id(entries: &[(Value, Value)]) -> Option<String> {
    entries.iter().find_map(|(key, value)| {
        if !matches!(key, Value::String(key) if key.to_string_lossy() == "id") {
            return None;
        }
        match value {
            Value::String(value) => Some(value.to_string_lossy()),
            Value::Integer(value) => Some(value.to_string()),
            Value::Number(value) if value.is_finite() => Some(value.to_string()),
            _ => None,
        }
    })
}

fn infer_main_state_number_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    const SENTINEL: i32 = 1_000_000;
    {
        main_state_probe.lock().ok()?.begin_number_recording(SENTINEL);
    }
    let result = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.number_calls.clone();
        probe.end_recording();
        calls
    };
    let ref_id = single_number_call(&calls)?;
    match result? {
        Value::Integer(value) if value == i64::from(SENTINEL + ref_id) => Some(ref_id),
        Value::Number(value) if (value - f64::from(SENTINEL + ref_id)).abs() < f64::EPSILON => {
            Some(ref_id)
        }
        _ => None,
    }
}

fn infer_main_state_number_expr(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.number_calls.clone();
        probe.end_recording();
        calls
    };
    let mut refs = calls;
    refs.sort_unstable();
    refs.dedup();
    if refs.is_empty() || refs.len() > 12 {
        return None;
    }
    let baseline = call_number_expr_with_values(function, main_state_probe, BTreeMap::new())?;
    let mut terms = Vec::new();
    for ref_id in refs {
        let value = call_number_expr_with_values(
            function,
            main_state_probe,
            BTreeMap::from([(ref_id, 1)]),
        )?;
        let coefficient = value - baseline;
        if coefficient != 0 {
            terms.push((ref_id, coefficient));
        }
    }
    if terms.is_empty() {
        return None;
    }
    Some(format_number_expr(baseline, &terms))
}

fn call_number_expr_with_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<i64> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values(values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => Some(value as i64),
        _ => None,
    }
}

fn format_number_expr(constant: i64, terms: &[(i32, i64)]) -> String {
    let mut parts = Vec::new();
    if constant != 0 {
        parts.push(constant.to_string());
    }
    for (ref_id, coefficient) in terms {
        let sign = if *coefficient < 0 { "-" } else { "+" };
        let magnitude = coefficient.unsigned_abs();
        let term = if magnitude == 1 {
            format!("number({ref_id})")
        } else {
            format!("{magnitude}*number({ref_id})")
        };
        if parts.is_empty() {
            parts.push(if *coefficient < 0 { format!("-{term}") } else { term });
        } else {
            parts.push(format!("{sign} {term}"));
        }
    }
    parts.join(" ")
}

fn infer_main_state_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(1);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.number_calls.clone();
        probe.end_recording();
        calls
    };
    let ref_id = single_number_call(&calls)?;
    let samples = [-1, 0, 1, 5];
    let observed = samples
        .iter()
        .map(|sample| call_draw_with_number(function, main_state_probe, ref_id, *sample))
        .collect::<Option<Vec<_>>>()?;

    let candidates = [
        ("> 0", samples.iter().map(|value| *value > 0).collect::<Vec<_>>()),
        ("== 0", samples.iter().map(|value| *value == 0).collect::<Vec<_>>()),
        ("!= 0", samples.iter().map(|value| *value != 0).collect::<Vec<_>>()),
        (">= 0", samples.iter().map(|value| *value >= 0).collect::<Vec<_>>()),
        ("< 0", samples.iter().map(|value| *value < 0).collect::<Vec<_>>()),
        ("<= 0", samples.iter().map(|value| *value <= 0).collect::<Vec<_>>()),
    ];
    candidates.into_iter().find_map(|(operator, expected)| {
        (observed == expected).then(|| format!("number({ref_id}) {operator}"))
    })
}

fn single_number_call(calls: &[i32]) -> Option<i32> {
    let first = *calls.first()?;
    calls.iter().all(|call| *call == first).then_some(first)
}

fn call_draw_with_number(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    ref_id: i32,
    value: i32,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_value(ref_id, value);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn infer_main_state_option_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_option_call_recording(true);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.option_calls.clone();
        probe.end_recording();
        calls
    };
    let option_id = single_number_call(&calls)?;
    let off = call_draw_with_option(function, main_state_probe, option_id, false)?;
    let on = call_draw_with_option(function, main_state_probe, option_id, true)?;
    match (off, on) {
        (false, true) => Some(format!("option({option_id})")),
        (true, false) => Some(format!("!option({option_id})")),
        _ => None,
    }
}

fn call_draw_with_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
    value: bool,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_option_recording_with_value(option_id, value);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn infer_main_state_timer_option_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_timer_option_call_recording();
    }
    let _ = function.call::<Value>(()).ok();
    let (timer_calls, option_calls) = {
        let mut probe = main_state_probe.lock().ok()?;
        let timer_calls = probe.timer_calls.clone();
        let option_calls = probe.option_calls.clone();
        probe.end_recording();
        (timer_calls, option_calls)
    };
    let timer_id = single_number_call(&timer_calls)?;
    let option_id = single_number_call(&option_calls)?;
    let samples =
        [(i32::MIN, false), (i32::MIN, true), (0, false), (0, true), (100, false), (100, true)];
    let observed = samples
        .iter()
        .map(|(timer_value, option_value)| {
            call_draw_with_timer_option(
                function,
                main_state_probe,
                timer_id,
                *timer_value,
                option_id,
                *option_value,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let candidates = [
        (
            format!("timer({timer_id}) == timer_off and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value == i32::MIN && *option_value)
                .collect::<Vec<_>>(),
        ),
        (
            format!("timer({timer_id}) != timer_off and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value != i32::MIN && *option_value)
                .collect::<Vec<_>>(),
        ),
        (
            format!("timer({timer_id}) > 0 and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value > 0 && *option_value)
                .collect::<Vec<_>>(),
        ),
    ];
    candidates
        .into_iter()
        .find_map(|(condition, expected)| (observed == expected).then_some(condition))
}

fn call_draw_with_timer_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    timer_id: i32,
    timer_value: i32,
    option_id: i32,
    option_value: bool,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_timer_option_recording_with_values(
            timer_id,
            timer_value,
            option_id,
            option_value,
        );
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn infer_main_state_gauge_type_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_gauge_type_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.gauge_type_calls;
        probe.end_recording();
        calls
    };
    if calls == 0 {
        return None;
    }
    let samples = [0, 1, 2, 3, 4, 5, 6];
    let observed = samples
        .iter()
        .map(|value| call_draw_with_gauge_type(function, main_state_probe, *value))
        .collect::<Option<Vec<_>>>()?;
    let enabled = samples
        .iter()
        .zip(observed)
        .filter_map(|(value, is_enabled)| is_enabled.then_some(*value))
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return None;
    }
    Some(
        enabled
            .into_iter()
            .map(|value| format!("gauge_type() == {value}"))
            .collect::<Vec<_>>()
            .join(" or "),
    )
}

fn call_draw_with_gauge_type(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    value: i32,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_gauge_type_recording_with_value(value);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn infer_judge_fast_slow_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    object_id: Option<&str>,
) -> Option<String> {
    let object_id = object_id?;
    let suffix = object_id.rsplit_once('_')?.1;
    if !matches!(suffix, "N" | "F" | "S") {
        return None;
    }

    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = unique_numbers_in_order(&probe.number_calls);
        probe.end_recording();
        calls
    };
    if calls.len() != 3 {
        return None;
    }
    let total = calls[0];
    let fast = calls[1];
    let slow = calls[2];

    match suffix {
        "N" if object_id == "PF_N" => {
            Some(format!("number({fast}) == number({slow}) or number({total}) == number({fast})"))
        }
        "N" => Some(format!("number({fast}) == number({slow})")),
        "F" if object_id == "PF_F" => {
            Some(format!("number({fast}) > number({slow}) and number({slow}) >= 1"))
        }
        "F" => Some(format!("number({fast}) > number({slow})")),
        "S" => Some(format!("number({slow}) > number({fast})")),
        _ => None,
    }
}

fn unique_numbers_in_order(values: &[i32]) -> Vec<i32> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(value) {
            unique.push(*value);
        }
    }
    unique
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

        assert!(report.warnings.is_empty());
        assert_eq!(json["type"], 5);
        assert_eq!(json["value"][0]["ref"], 71);
        assert!(json["value"][0].get("value").is_none());
        assert_eq!(json["destination"][0]["draw"], "option(1)");
    }

    #[test]
    fn converts_simple_main_state_number_draw_function() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                source = {{ id = "src", path = "main.png" }},
                image = {{ id = "panel", src = "src", x = 0, y = 0, w = 10, h = 10 }},
                destination = {
                    { id = "panel", draw = function()
                        return main_state.number(425) > 0
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("play7.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["destination"][0]["draw"], "number(425) > 0");
    }

    #[test]
    fn converts_main_state_number_value_expression() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                value = {
                    { id = "remain", src = 1, x = 0, y = 0, w = 10, h = 10, value = function()
                        local total = main_state.number(106)
                        local pgreat = main_state.number(110)
                        local great = main_state.number(111)
                        return total - pgreat - great
                    end }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("play7.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["value"][0]["expr"], "number(106) - number(110) - number(111)");
    }

    #[test]
    fn converts_simple_main_state_option_draw_function() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 7,
                source = {{ id = "src", path = "main.png" }},
                image = {{ id = "replay", src = "src", x = 0, y = 0, w = 10, h = 10 }},
                destination = {
                    { id = "replay", draw = function()
                        return main_state.option(197) == true
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                    { id = "replay", draw = function()
                        return main_state.option(196) == false
                    end, dst = {{ x = 5, y = 6, w = 7, h = 8 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("result.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["destination"][0]["draw"], "option(197)");
        assert_eq!(json["destination"][1]["draw"], "!option(196)");
    }

    #[test]
    fn converts_timer_off_and_option_draw_function() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("result.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 7,
                source = {{ id = "src", path = "main.png" }},
                image = {{ id = "wait", src = "src", x = 0, y = 0, w = 10, h = 10 }},
                destination = {
                    { id = "wait", draw = function()
                        local wait_timer = main_state.timer(173)
                        local wait_op = main_state.option(51)
                        return wait_timer == main_state.timer_off_value and wait_op == true
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("result.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["destination"][0]["draw"], "timer(173) == timer_off and option(51)");
    }

    #[test]
    fn converts_gauge_type_draw_function() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                destination = {
                    { id = "gauge", draw = function()
                        local gauge_type = main_state.gauge_type()
                        return gauge_type == 4 or gauge_type == 5
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("play7.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(json["destination"][0]["draw"], "gauge_type() == 4 or gauge_type() == 5");
    }

    #[test]
    fn converts_judge_fast_slow_draw_functions() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            function is_judge_count_enabled()
                return skin_config.option["ジャッジカウント"] == 906
            end
            return {
                type = 0,
                property = {
                    { name = "ジャッジカウント", item = {
                        { name = "Off", op = 905 },
                        { name = "On", op = 906 }
                    }}
                },
                destination = {
                    { id = "PF_N", op = {906}, draw = function()
                        local total = main_state.number(110)
                        local early = main_state.number(410)
                        local late = main_state.number(411)
                        return is_judge_count_enabled() and (early == late or total == early)
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                    { id = "PF_F", op = {906}, draw = function()
                        local total = main_state.number(110)
                        local early = main_state.number(410)
                        local late = main_state.number(411)
                        return is_judge_count_enabled() and (early > late and late >= 1)
                    end, dst = {{ x = 5, y = 6, w = 7, h = 8 }} },
                    { id = "GR_S", op = {906}, draw = function()
                        local total = main_state.number(111)
                        local early = main_state.number(412)
                        local late = main_state.number(413)
                        return is_judge_count_enabled() and (late > early)
                    end, dst = {{ x = 9, y = 10, w = 11, h = 12 }} }
                }
            }
            "#,
        )
        .unwrap();

        let output = root.join("out.json");
        let report =
            convert_lua_skin_to_json(&root.join("play7.luaskin"), &output, &BTreeMap::new())
                .unwrap();
        let json: JsonValue = serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();

        assert!(report.warnings.is_empty());
        assert_eq!(
            json["destination"][0]["draw"],
            "number(410) == number(411) or number(110) == number(410)"
        );
        assert_eq!(
            json["destination"][1]["draw"],
            "number(410) > number(411) and number(411) >= 1"
        );
        assert_eq!(json["destination"][2]["draw"], "number(413) > number(412)");
    }

    #[test]
    fn skin_config_get_path_resolves_wildcard_under_skin_root() {
        let root = unique_test_dir("bmz-lua-skin");
        fs::create_dir_all(root.join("_font/TYPE-M")).unwrap();
        fs::write(root.join("_font/TYPE-M/_font_set.lua"), "return {}").unwrap();

        let path = skin_config_get_path(&root, "_font/*/_font_set.lua").unwrap();

        assert_eq!(path, root.join("_font/TYPE-M/_font_set.lua"));
        assert!(skin_config_get_path(&root, "../outside").is_err());
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
