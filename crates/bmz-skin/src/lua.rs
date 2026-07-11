use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use mlua::{Function, HookTriggers, Lua, Table, Value, Variadic, VmState};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use bmz_skin_document::{
    SKIN_DYNAMIC_TIMER_BASE, SKIN_EXPR_ADJUSTED_COVER, SKIN_EXPR_ADJUSTED_RATE,
    SKIN_EXPR_ADJUSTED_RATE_ADOT, SKIN_EXPR_COURSE_TABLE_TEXT,
    SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT, SKIN_EXPR_FS_THRESHOLD, SKIN_EXPR_GAUGE_AMOUNT_FRACTION,
    SKIN_EXPR_GAUGE_AMOUNT_INTEGER, SKIN_EXPR_GAUGE_PERCENT_FRACTION,
    SKIN_EXPR_GAUGE_PERCENT_INTEGER, SKIN_REF_PLAY_GAUGE_TYPE,
};

use crate::{
    LoadedLuaSkinValue, LuaLoadRuntimeState, SkinLoadDependencies, SkinLoadWarning,
    SkinLoadedFileDependency,
};

const LUA_INSTRUCTION_LIMIT: i64 = 2_000_000;
const LUA_HOOK_INTERVAL: u32 = 1_000;
const LUA_MAX_TABLE_DEPTH: usize = 64;
const LUA_MAX_TABLE_ENTRIES: usize = 200_000;
const TIMER_OFF_VALUE: i32 = i32::MIN;

/// beatoraja fast/slow 判定カウント ref (graph 比率推論用)
const FAST_SLOW_FAST_REFS: [i32; 6] = [410, 412, 414, 416, 418, 421];
const FAST_SLOW_SLOW_REFS: [i32; 6] = [411, 413, 415, 417, 419, 422];

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertReport {
    pub warnings: Vec<String>,
}

pub fn load_lua_skin_value(
    input: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<LoadedLuaSkinValue> {
    let (value, warnings, files, dependencies) =
        execute_lua_skin(input, options, files, runtime_state)?;
    Ok(LoadedLuaSkinValue {
        value,
        warnings: warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
        files,
        dependencies,
        internal_enabled_options: Vec::new(),
    })
}

pub fn load_lua_skin_header_value(input: &Path) -> Result<LoadedLuaSkinValue> {
    let (value, warnings) = execute_lua_skin_header(input)?;
    Ok(LoadedLuaSkinValue {
        value,
        warnings: warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
        files: BTreeMap::new(),
        dependencies: SkinLoadDependencies::default(),
        internal_enabled_options: Vec::new(),
    })
}

pub fn convert_lua_skin_to_json(
    input: &Path,
    output: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<ConvertReport> {
    let (json, warnings, _, _) =
        execute_lua_skin(input, options, files, &LuaLoadRuntimeState::default())?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output dir: {}", parent.display()))?;
    }
    fs::write(output, serde_json::to_string_pretty(&json)? + "\n")
        .with_context(|| format!("failed to write json skin: {}", output.display()))?;

    Ok(ConvertReport { warnings })
}

fn execute_lua_skin_header(input: &Path) -> Result<(JsonValue, Vec<String>)> {
    let input = canonicalize_skin_path(input)
        .with_context(|| format!("failed to canonicalize input: {}", input.display()))?;
    let parent =
        input.parent().ok_or_else(|| anyhow!("input path has no parent: {}", input.display()))?;
    let root = canonicalize_skin_path(parent)
        .with_context(|| format!("failed to canonicalize skin root: {}", input.display()))?;

    let mut warnings = Vec::new();
    let mut table_budget = TableBudget::default();
    let source = fs::read_to_string(&input)
        .with_context(|| format!("failed to read lua skin: {}", input.display()))?;

    let lua = Lua::new();
    install_instruction_limit(&lua);
    let probe = install_sandbox(
        &lua,
        &root,
        &BTreeMap::new(),
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
    )?;
    let header = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    let header_json =
        lua_value_to_json(&lua, header, "$", 0, &mut warnings, &probe, &mut table_budget)?;

    Ok((header_json, warnings))
}

fn execute_lua_skin(
    input: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<(JsonValue, Vec<String>, BTreeMap<String, String>, SkinLoadDependencies)> {
    let input = canonicalize_skin_path(input)
        .with_context(|| format!("failed to canonicalize input: {}", input.display()))?;
    let parent =
        input.parent().ok_or_else(|| anyhow!("input path has no parent: {}", input.display()))?;
    let root = canonicalize_skin_path(parent)
        .with_context(|| format!("failed to canonicalize skin root: {}", input.display()))?;

    let mut warnings = Vec::new();
    let mut table_budget = TableBudget::default();
    let source = fs::read_to_string(&input)
        .with_context(|| format!("failed to read lua skin: {}", input.display()))?;

    let header_lua = Lua::new();
    install_instruction_limit(&header_lua);
    let header_probe = install_sandbox(
        &header_lua,
        &root,
        options,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
    )?;
    let header = header_lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    let header_json = lua_value_to_json(
        &header_lua,
        header,
        "$",
        0,
        &mut warnings,
        &header_probe,
        &mut table_budget,
    )?;
    let skin_options = skin_config_options_from_header(&header_json, options, &mut warnings);
    let skin_files = skin_files_from_header(&root, &header_json, files);
    let skin_named_files = skin_named_files_from_header(&root, &header_json, files);
    let skin_offsets = skin_config_offsets_from_header(&header_json);
    // ヘッダ pass では skin_config / 全 option が未注入のため draw/value 推論が失敗しうる。
    // 本 pass の警告だけ残す。
    warnings.retain(|warning| {
        !warning.starts_with("skipping unsupported draw function at ")
            && !warning.starts_with("skipping unsupported value function at ")
            && !warning.starts_with("mixed lua table converted to object at ")
    });

    let lua = Lua::new();
    install_instruction_limit(&lua);
    let dependencies = Arc::new(Mutex::new(SkinLoadDependencies::default()));
    let main_state_probe = install_sandbox(
        &lua,
        &root,
        options,
        Some(&skin_options),
        &skin_files,
        &skin_file_dependency_names_from_header(&header_json),
        &skin_offsets,
        runtime_state,
        Some(dependencies.clone()),
    )?;
    let value = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin: {}", input.display()))?;
    let mut json = lua_value_to_json(
        &lua,
        value,
        "$",
        0,
        &mut warnings,
        &main_state_probe,
        &mut table_budget,
    )?;
    record_static_skin_config_option_dependencies(&source, &skin_options, &dependencies);

    if let JsonValue::Object(ref mut root) = json {
        postprocess_lua_skin_json(root);

        let timers = main_state_probe
            .lock()
            .ok()
            .map(|probe| probe.dynamic_timers.clone())
            .unwrap_or_default();
        if !timers.is_empty() {
            let entries = timers.into_iter().map(|(id, observe)| {
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::Number(JsonNumber::from(id))),
                    ("observe".to_string(), JsonValue::String(observe)),
                ]))
            });
            root.insert("dynamicTimer".to_string(), JsonValue::Array(entries.collect()));
        }
        let fixed_delay_timers = main_state_probe
            .lock()
            .ok()
            .map(|probe| probe.fixed_delay_timers.clone())
            .unwrap_or_default();
        if !fixed_delay_timers.is_empty() {
            let entries = fixed_delay_timers.into_iter().map(|(id, source_timer, delay_ms)| {
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::Number(JsonNumber::from(id))),
                    ("sourceTimer".to_string(), JsonValue::Number(JsonNumber::from(source_timer))),
                    ("delayMs".to_string(), JsonValue::Number(JsonNumber::from(delay_ms))),
                ]))
            });
            root.insert("fixedDelayTimer".to_string(), JsonValue::Array(entries.collect()));
        }
    }

    let dependencies =
        dependencies.lock().map_err(|_| anyhow!("lua dependency tracker lock poisoned"))?.clone();
    Ok((json, warnings, skin_named_files, dependencies))
}

fn postprocess_lua_skin_json(root: &mut JsonMap<String, JsonValue>) {
    repair_keybeam_destination_draws(root);
}

fn install_instruction_limit(lua: &Lua) {
    let remaining = AtomicI64::new(LUA_INSTRUCTION_LIMIT);
    lua.set_hook(HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL), move |_, _| {
        if remaining.fetch_sub(i64::from(LUA_HOOK_INTERVAL), Ordering::Relaxed)
            <= i64::from(LUA_HOOK_INTERVAL)
        {
            Err(mlua::Error::runtime("lua skin instruction limit exceeded"))
        } else {
            Ok(VmState::Continue)
        }
    });
}

#[derive(Debug)]
struct TableBudget {
    remaining_entries: usize,
}

impl Default for TableBudget {
    fn default() -> Self {
        Self { remaining_entries: LUA_MAX_TABLE_ENTRIES }
    }
}

impl TableBudget {
    fn consume(&mut self, count: usize, path: &str) -> Result<()> {
        if count > self.remaining_entries {
            bail!("lua table entry limit exceeded at {path}");
        }
        self.remaining_entries -= count;
        Ok(())
    }
}

fn create_skin_config_option_table(
    lua: &Lua,
    skin_config_options: &BTreeMap<String, i64>,
    load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> Result<Table> {
    let option = lua.create_table()?;
    let option_values = skin_config_options.clone();
    let dependencies_for_index = load_dependencies.clone();
    let index = lua.create_function(move |_, (_table, key): (Table, Value)| {
        let Value::String(key) = key else {
            return Ok(Value::Nil);
        };
        let key = key.to_str()?;
        let Some(value) = option_values.get(key.as_ref()) else {
            return Ok(Value::Nil);
        };
        if let Ok(option_id) = i32::try_from(*value) {
            record_load_dependency_option(dependencies_for_index.as_ref(), option_id, true);
        }
        Ok(Value::Integer(*value))
    })?;
    let option_values_for_pairs = skin_config_options.clone();
    let dependencies_for_pairs = load_dependencies;
    let pairs = lua.create_function(move |lua, _: Table| {
        let pairs_table = lua.create_table()?;
        for (key, value) in &option_values_for_pairs {
            pairs_table.set(key.as_str(), *value)?;
            if let Ok(option_id) = i32::try_from(*value) {
                record_load_dependency_option(dependencies_for_pairs.as_ref(), option_id, true);
            }
        }
        let next = lua.globals().get::<Function>("next")?;
        Ok((next, pairs_table, Value::Nil))
    })?;
    let metatable = lua.create_table()?;
    metatable.set("__index", index)?;
    metatable.set("__pairs", pairs)?;
    option.set_metatable(Some(metatable));
    Ok(option)
}

fn record_load_dependency_option(
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
    option_id: i32,
    value: bool,
) {
    if let Some(dependencies) = dependencies
        && let Ok(mut dependencies) = dependencies.lock()
    {
        dependencies.option_values.insert(option_id, value);
    }
}

fn record_skin_config_file_dependency(
    requested: &str,
    skin_file_dependency_names: &BTreeMap<String, String>,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
) {
    let requested = requested.replace('\\', "/");
    let Some(name) = skin_config_file_dependency_name(&requested, skin_file_dependency_names)
    else {
        return;
    };
    if let Some(dependencies) = dependencies
        && let Ok(mut dependencies) = dependencies.lock()
    {
        dependencies.files.insert(name);
    }
}

fn skin_config_file_dependency_name(
    requested: &str,
    skin_file_dependency_names: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(name) = skin_file_dependency_names.get(requested) {
        return Some(name.clone());
    }
    let (requested_prefix, _) = requested.split_once('*')?;
    skin_file_dependency_names.iter().find_map(|(configured, name)| {
        let (configured_prefix, _) = configured.split_once('*')?;
        (requested_prefix == configured_prefix).then(|| name.clone())
    })
}

fn record_lua_loaded_file_dependency(
    path: &Path,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
) {
    let Some(dependencies) = dependencies else {
        return;
    };
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dependency =
        SkinLoadedFileDependency { modified: metadata.modified().ok(), len: metadata.len() };
    if let Ok(mut dependencies) = dependencies.lock() {
        dependencies.loaded_files.insert(path, dependency);
    }
}

fn record_static_skin_config_option_dependencies(
    source: &str,
    skin_config_options: &BTreeMap<String, i64>,
    dependencies: &Arc<Mutex<SkinLoadDependencies>>,
) {
    if !source.contains("skin_config.option") {
        return;
    }
    let mut matched_literal = false;
    for quote in ['"', '\''] {
        let pattern = format!("skin_config.option[{quote}");
        let mut rest = source;
        while let Some(start) = rest.find(&pattern) {
            let value_start = start + pattern.len();
            let after_start = &rest[value_start..];
            let Some(end) = after_start.find(quote) else {
                break;
            };
            let name = &after_start[..end];
            if let Some(option_id) =
                skin_config_options.get(name).and_then(|value| i32::try_from(*value).ok())
            {
                record_load_dependency_option(Some(dependencies), option_id, true);
                matched_literal = true;
            }
            rest = &after_start[end + quote.len_utf8()..];
        }
    }
    if !matched_literal {
        for option_id in skin_config_options.values().filter_map(|value| i32::try_from(*value).ok())
        {
            record_load_dependency_option(Some(dependencies), option_id, true);
        }
    }
}

fn install_sandbox(
    lua: &Lua,
    root: &Path,
    options: &BTreeMap<String, String>,
    skin_config_options: Option<&BTreeMap<String, i64>>,
    skin_files: &BTreeMap<String, String>,
    skin_file_dependency_names: &BTreeMap<String, String>,
    skin_offsets: &BTreeMap<String, LuaSkinOffsetValue>,
    runtime_state: &LuaLoadRuntimeState,
    load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> Result<Arc<Mutex<MainStateProbe>>> {
    let main_state_probe = Arc::new(Mutex::new(MainStateProbe::default()));
    if let Some(load_dependencies) = load_dependencies.clone() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.load_dependencies = Some(load_dependencies);
    }
    if !runtime_state.number_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.number_values = runtime_state.number_values.clone();
    }
    if !runtime_state.option_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.option_values = runtime_state.option_values.clone();
    }
    let globals = lua.globals();
    if let Some(skin_config_options) = skin_config_options {
        let skin_config = lua.create_table()?;
        let option =
            create_skin_config_option_table(lua, skin_config_options, load_dependencies.clone())?;
        skin_config.set("option", option)?;
        let offset = lua.create_table()?;
        for (name, value) in skin_offsets {
            let offset_value = lua.create_table()?;
            offset_value.set("x", value.x)?;
            offset_value.set("y", value.y)?;
            offset_value.set("w", value.w)?;
            offset_value.set("h", value.h)?;
            offset_value.set("r", value.r)?;
            offset_value.set("a", value.a)?;
            offset.set(name.as_str(), offset_value)?;
        }
        skin_config.set("offset", offset)?;
        let root_for_get_path = root.to_path_buf();
        let skin_files_for_get_path = skin_files.clone();
        let skin_file_dependency_names_for_get_path = skin_file_dependency_names.clone();
        let dependencies_for_get_path = load_dependencies.clone();
        let get_path = lua.create_function(move |_, requested: String| {
            record_skin_config_file_dependency(
                &requested,
                &skin_file_dependency_names_for_get_path,
                dependencies_for_get_path.as_ref(),
            );
            skin_config_get_path(&root_for_get_path, &requested, &skin_files_for_get_path)
                .map(|path| path.to_string_lossy().to_string())
                .map_err(mlua::Error::external)
        })?;
        skin_config.set("get_path", get_path)?;
        globals.set("skin_config", skin_config)?;
    }
    globals.set("os", create_os_stub(lua, main_state_probe.clone())?)?;
    globals.set("io", create_io_stub(lua, root)?)?;
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
    let dependencies_for_dofile = load_dependencies.clone();
    let dofile = lua.create_function(move |lua, path: String| {
        let path =
            resolve_lua_path(&root_for_dofile, &path, false).map_err(mlua::Error::external)?;
        record_lua_loaded_file_dependency(&path, dependencies_for_dofile.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).eval::<Value>()
    })?;
    globals.set("dofile", dofile)?;

    let root_for_loadfile = sandbox_root.clone();
    let dependencies_for_loadfile = load_dependencies.clone();
    let loadfile = lua.create_function(move |lua, path: String| {
        let path =
            resolve_lua_path(&root_for_loadfile, &path, false).map_err(mlua::Error::external)?;
        record_lua_loaded_file_dependency(&path, dependencies_for_loadfile.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).into_function()
    })?;
    globals.set("loadfile", loadfile)?;

    let root = sandbox_root;
    let probe_for_require = main_state_probe.clone();
    let dependencies_for_require = load_dependencies.clone();
    let require = lua.create_function(move |lua, module: String| {
        if module == "main_state" {
            return create_main_state_stub(lua, probe_for_require.clone());
        }
        if module == "timer_util" {
            return create_timer_util_module(lua, probe_for_require.clone());
        }
        if module == "event_util" {
            return create_event_util_module(lua);
        }
        if module == "luajava" {
            return create_luajava_stub(lua);
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
        record_lua_loaded_file_dependency(&path, dependencies_for_require.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        let value = lua.load(&source).set_name(path.to_string_lossy().as_ref()).eval::<Value>()?;
        let value = if matches!(value, Value::Nil) { Value::Boolean(true) } else { value };
        loaded.set(module, value.clone())?;
        Ok(value)
    })?;
    globals.set("require", require)?;

    let timer_fn_map = lua.create_table()?;
    let timer_fn_metatable = lua.create_table()?;
    timer_fn_metatable.set("__mode", "k")?;
    timer_fn_map.set_metatable(Some(timer_fn_metatable));
    globals.set("bmz_timer_fn_map", timer_fn_map)?;

    Ok(main_state_probe)
}

#[derive(Debug, Clone)]
struct MainStateProbe {
    mode: MainStateProbeMode,
    number_calls: Vec<i32>,
    number_values: BTreeMap<i32, i32>,
    option_calls: Vec<i32>,
    option_values: BTreeMap<i32, bool>,
    timer_calls: Vec<i32>,
    timer_values: BTreeMap<i32, i32>,
    event_index_calls: Vec<i32>,
    event_index_values: BTreeMap<i32, i32>,
    gauge_type_calls: usize,
    gauge_type_value: i32,
    float_number_calls: Vec<i32>,
    float_number_values: BTreeMap<i32, f64>,
    text_calls: Vec<i32>,
    os_clock_calls: usize,
    os_clock_value: Option<f64>,
    time_value_us: i32,
    next_dynamic_timer_id: i32,
    dynamic_timers: Vec<(i32, String)>,
    fixed_delay_timers: Vec<(i32, i32, i32)>,
    keylogger_destination_occurrences: BTreeMap<String, usize>,
    gauge_lead_glow_occurrences: BTreeMap<String, usize>,
    load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
}

impl Default for MainStateProbe {
    fn default() -> Self {
        Self {
            mode: MainStateProbeMode::default(),
            number_calls: Vec::new(),
            number_values: BTreeMap::new(),
            option_calls: Vec::new(),
            option_values: BTreeMap::new(),
            timer_calls: Vec::new(),
            timer_values: BTreeMap::new(),
            event_index_calls: Vec::new(),
            event_index_values: BTreeMap::new(),
            gauge_type_calls: 0,
            gauge_type_value: 0,
            float_number_calls: Vec::new(),
            float_number_values: BTreeMap::new(),
            text_calls: Vec::new(),
            os_clock_calls: 0,
            os_clock_value: None,
            time_value_us: 1_000_000,
            next_dynamic_timer_id: SKIN_DYNAMIC_TIMER_BASE,
            dynamic_timers: Vec::new(),
            fixed_delay_timers: Vec::new(),
            keylogger_destination_occurrences: BTreeMap::new(),
            gauge_lead_glow_occurrences: BTreeMap::new(),
            load_dependencies: None,
        }
    }
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
    fn clear_aux_calls(&mut self) {
        self.float_number_calls.clear();
        self.float_number_values.clear();
        self.text_calls.clear();
        self.os_clock_calls = 0;
        self.os_clock_value = None;
        self.event_index_calls.clear();
        self.event_index_values.clear();
    }

    fn begin_number_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::SymbolicNumbers { base_value: default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    fn begin_number_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    fn begin_number_call_recording_with_option_value(
        &mut self,
        default_value: i32,
        option_id: i32,
        option_value: bool,
    ) {
        self.begin_number_call_recording(default_value);
        self.option_values.insert(option_id, option_value);
    }

    fn begin_number_recording_with_value(&mut self, ref_id: i32, value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
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
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    fn begin_number_recording_with_values_and_options(
        &mut self,
        values: BTreeMap<i32, i32>,
        options: BTreeMap<i32, bool>,
    ) {
        self.begin_number_recording_with_values(values);
        self.option_values = options;
    }

    fn begin_number_timer_recording_with_values(
        &mut self,
        values: BTreeMap<i32, i32>,
        mut timer_values: BTreeMap<i32, i32>,
    ) {
        self.begin_number_recording_with_values(values);
        timer_values.entry(i32::MIN).or_insert(i32::MIN);
        self.timer_values = timer_values;
    }

    fn begin_option_call_recording(&mut self, default_value: bool) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
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
        self.event_index_calls.clear();
        self.event_index_values.clear();
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
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.option_values.insert(i32::MIN, true);
        self.timer_values.insert(i32::MIN, i32::MIN);
    }

    fn begin_timer_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.timer_values.insert(i32::MIN, default_value);
    }

    fn begin_timer_recording_with_values(&mut self, mut timer_values: BTreeMap<i32, i32>) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        timer_values.entry(i32::MIN).or_insert(i32::MIN);
        self.timer_values = timer_values;
    }

    fn begin_timer_event_recording_with_values(
        &mut self,
        timer_values: BTreeMap<i32, i32>,
        event_id: i32,
        event_value: i32,
    ) {
        self.begin_timer_recording_with_values(timer_values);
        self.event_index_values.insert(event_id, event_value);
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
        self.event_index_calls.clear();
        self.event_index_values.clear();
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
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = value;
    }

    fn begin_gauge_type_recording_with_value(&mut self, value: i32) {
        self.begin_gauge_type_call_recording(value);
    }

    fn begin_event_index_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    fn begin_event_index_recording_with_value(&mut self, event_id: i32, value: i32) {
        self.begin_event_index_call_recording(0);
        self.event_index_values.insert(event_id, value);
    }

    fn begin_os_clock_recording(&mut self, value: f64) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.float_number_calls.clear();
        self.float_number_values.clear();
        self.text_calls.clear();
        self.os_clock_calls = 0;
        self.os_clock_value = Some(value);
    }

    fn begin_os_clock_options_recording(
        &mut self,
        value: f64,
        option_values: &[(i32, bool)],
        default_option_value: bool,
    ) {
        self.begin_os_clock_recording(value);
        self.option_values.insert(i32::MIN, default_option_value);
        for &(option_id, option_value) in option_values {
            self.option_values.insert(option_id, option_value);
        }
    }

    fn end_recording(&mut self) {
        self.mode = MainStateProbeMode::RuntimeStub;
        self.number_values.clear();
        self.option_values.clear();
        self.timer_values.clear();
        self.event_index_values.clear();
        self.event_index_calls.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.os_clock_value = None;
    }

    fn number(&mut self, ref_id: i32) -> i32 {
        match self.mode {
            MainStateProbeMode::RuntimeStub => {
                let value = self
                    .number_values
                    .get(&ref_id)
                    .copied()
                    .unwrap_or_else(|| lua_runtime_stub_number(ref_id));
                self.record_load_time_number_dependency(ref_id, value);
                value
            }
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

    fn judge(&mut self, index: i32) -> i32 {
        main_state_judge_ref(index).map(|ref_id| self.number(ref_id)).unwrap_or(0)
    }

    fn option(&mut self, option_id: i32) -> bool {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            let value = self
                .option_values
                .get(&option_id)
                .copied()
                .unwrap_or_else(|| lua_runtime_stub_option(option_id));
            self.record_load_time_option_dependency(option_id, value);
            return value;
        }
        self.option_calls.push(option_id);
        self.option_values
            .get(&option_id)
            .copied()
            .or_else(|| self.option_values.get(&i32::MIN).copied())
            .unwrap_or(false)
    }

    fn record_load_time_number_dependency(&self, ref_id: i32, value: i32) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.number_values.insert(ref_id, value);
        }
    }

    fn record_load_time_option_dependency(&self, option_id: i32, value: bool) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.option_values.insert(option_id, value);
        }
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

    fn float_number(&mut self, ref_id: i32) -> f64 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 0.0;
        }
        self.float_number_calls.push(ref_id);
        self.float_number_values.get(&ref_id).copied().unwrap_or(0.0)
    }

    fn volume_number(&mut self, ref_id: i32) -> f64 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 1.0;
        }
        f64::from(self.number(ref_id)) / 100.0
    }

    fn text(&mut self, ref_id: i32) -> String {
        if ref_id == 1010 {
            return "BMZ Player 0.1.0".to_string();
        }
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            if (1001..=1003).contains(&ref_id) {
                return format!(
                    "{LUA_TEXT_REF_SENTINEL_PREFIX}{ref_id}{LUA_TEXT_REF_SENTINEL_SUFFIX}"
                );
            }
            return String::new();
        }
        self.text_calls.push(ref_id);
        format!("Text{ref_id}")
    }

    fn event_index(&mut self, event_id: i32) -> i32 {
        match self.mode {
            MainStateProbeMode::RuntimeStub => 0,
            MainStateProbeMode::SymbolicNumbers { base_value } => {
                self.event_index_calls.push(event_id);
                self.event_index_values.get(&event_id).copied().unwrap_or(base_value + event_id)
            }
            MainStateProbeMode::RecordNumbers { default_value } => {
                self.event_index_calls.push(event_id);
                self.event_index_values.get(&event_id).copied().unwrap_or(default_value)
            }
        }
    }

    fn time(&mut self) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return lua_load_now_micros();
        }
        let value = self.time_value_us;
        self.time_value_us = self.time_value_us.saturating_add(1_000);
        value
    }

    fn begin_draw_probe(&mut self, numbers: BTreeMap<i32, i32>, floats: BTreeMap<i32, f64>) {
        self.begin_number_recording_with_values(numbers);
        self.float_number_values = floats;
    }

    fn os_clock(&mut self) -> f64 {
        if let Some(value) = self.os_clock_value {
            self.os_clock_calls += 1;
            return value;
        }
        if !matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            self.os_clock_calls += 1;
            return 0.0;
        }
        lua_os_clock_seconds()
    }
}

const LUA_TEXT_REF_SENTINEL_PREFIX: &str = "__BMZ_TEXT_REF_";
const LUA_TEXT_REF_SENTINEL_SUFFIX: &str = "__";

fn lua_runtime_stub_number(ref_id: i32) -> i32 {
    let now = unix_seconds_to_utc_datetime(lua_os_now_seconds());
    match ref_id {
        // beatoraja IntegerProperty: currenttime_year/month/day
        21 => now.year,
        22 => now.month as i32,
        23 => now.day as i32,
        _ => 0,
    }
}

fn lua_runtime_stub_option(option_id: i32) -> bool {
    match option_id {
        // OPTION_AUTOPLAYOFF. Some Lua play skins build their score graph only for normal play.
        32 => true,
        _ => false,
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
    let probe_for_judge = probe.clone();
    table.set(
        "judge",
        lua.create_function(move |_, index: i32| {
            Ok(probe_for_judge
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .judge(index))
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
    let probe_for_text = probe.clone();
    table.set(
        "text",
        lua.create_function(move |_, ref_id: i32| {
            Ok(probe_for_text
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .text(ref_id))
        })?,
    )?;
    table.set(
        "offset",
        lua.create_function(move |lua, _offset_id: i32| create_main_state_offset_table(lua))?,
    )?;
    let probe_for_float_number = probe.clone();
    table.set(
        "float_number",
        lua.create_function(move |_, ref_id: i32| {
            Ok(probe_for_float_number
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .float_number(ref_id))
        })?,
    )?;
    let probe_for_event_index = probe.clone();
    table.set(
        "event_index",
        lua.create_function(move |_, event_id: i32| {
            Ok(probe_for_event_index
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .event_index(event_id))
        })?,
    )?;
    table.set(
        "timer",
        lua.create_function(move |_, timer_id: i32| {
            Ok(probe_for_timer
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .timer(timer_id))
        })?,
    )?;
    let probe_for_time = probe.clone();
    table.set(
        "time",
        lua.create_function(move |_, ()| {
            Ok(probe_for_time
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .time())
        })?,
    )?;
    let probe_for_gauge_type = probe.clone();
    table.set(
        "gauge_type",
        lua.create_function(move |_, ()| {
            Ok(probe_for_gauge_type
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .gauge_type())
        })?,
    )?;
    let probe_for_volume_sys = probe.clone();
    table.set(
        "volume_sys",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_sys
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(57))
        })?,
    )?;
    let probe_for_volume_key = probe.clone();
    table.set(
        "volume_key",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_key
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(58))
        })?,
    )?;
    let probe_for_volume_bg = probe;
    table.set(
        "volume_bg",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_bg
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(59))
        })?,
    )?;
    table.set("set_volume_sys", lua.create_function(|_, _: Value| Ok(true))?)?;
    table.set("set_volume_key", lua.create_function(|_, _: Value| Ok(true))?)?;
    table.set("set_volume_bg", lua.create_function(|_, _: Value| Ok(true))?)?;
    table
        .set("audio_play", lua.create_function(|_, (_path, _volume): (Value, Value)| Ok(true))?)?;
    table
        .set("audio_loop", lua.create_function(|_, (_path, _volume): (Value, Value)| Ok(true))?)?;
    table.set("audio_stop", lua.create_function(|_, _path: Value| Ok(true))?)?;
    Ok(Value::Table(table))
}

fn create_main_state_offset_table(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("x", 0)?;
    table.set("y", 0)?;
    table.set("w", 0)?;
    table.set("h", 0)?;
    table.set("r", 0)?;
    table.set("a", 0)?;
    Ok(Value::Table(table))
}

#[derive(Debug)]
struct TimerObserveState {
    timer_value: i32,
}

fn lua_load_now_micros() -> i32 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_micros().min(i32::MAX as u128) as i32
}

fn lua_load_now_ms() -> i32 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_millis().min(i32::MAX as u128) as i32
}

fn create_os_stub(lua: &Lua, probe: Arc<Mutex<MainStateProbe>>) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    let probe_for_clock = probe.clone();
    table.set(
        "clock",
        lua.create_function(move |_, ()| {
            Ok(probe_for_clock
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .os_clock())
        })?,
    )?;
    table.set(
        "date",
        lua.create_function(|lua, args: Variadic<Value>| {
            let format = args
                .first()
                .and_then(|value| match value {
                    Value::String(value) => Some(value.to_string_lossy()),
                    _ => None,
                })
                .unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".to_string());
            let seconds = args
                .get(1)
                .and_then(|value| match value {
                    Value::Integer(value) => Some(*value),
                    Value::Number(value) => Some(*value as i64),
                    _ => None,
                })
                .unwrap_or_else(lua_os_now_seconds);
            let date = unix_seconds_to_utc_datetime(seconds);
            if format == "*t" || format == "!*t" {
                let result = lua.create_table()?;
                result.set("year", date.year)?;
                result.set("month", date.month)?;
                result.set("day", date.day)?;
                result.set("hour", date.hour)?;
                result.set("min", date.minute)?;
                result.set("sec", date.second)?;
                result.set("wday", date.weekday)?;
                result.set("yday", date.yearday)?;
                result.set("isdst", false)?;
                Ok(Value::Table(result))
            } else {
                Ok(Value::String(lua.create_string(format_lua_date(&format, date))?))
            }
        })?,
    )?;
    Ok(Value::Table(table))
}

fn create_io_stub(lua: &Lua, root: &Path) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    let root_for_open = root.to_path_buf();
    table.set(
        "open",
        lua.create_function(move |lua, (path, mode): (String, Option<String>)| {
            let mode = mode.unwrap_or_else(|| "r".to_string());
            if mode.starts_with('r') {
                let Ok(path) = resolve_skin_io_path(&root_for_open, &path) else {
                    return Ok(Value::Nil);
                };
                let Ok(source) = fs::read_to_string(path) else {
                    return Ok(Value::Nil);
                };
                return create_read_file_stub(lua, source);
            }
            if mode.starts_with('w') || mode.starts_with('a') {
                return create_write_file_stub(lua);
            }
            Ok(Value::Nil)
        })?,
    )?;
    let root_for_lines = root.to_path_buf();
    table.set(
        "lines",
        lua.create_function(move |lua, path: String| {
            let Ok(path) = resolve_skin_io_path(&root_for_lines, &path) else {
                return create_lines_iterator(lua, Vec::new());
            };
            let source = fs::read_to_string(path).unwrap_or_default();
            create_lines_iterator(lua, source.lines().map(str::to_string).collect())
        })?,
    )?;
    table.set("close", lua.create_function(|_, _file: Value| Ok(true))?)?;
    Ok(Value::Table(table))
}

fn lua_os_clock_seconds() -> f64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(Instant::now);
    origin.elapsed().as_secs_f64()
}

fn create_read_file_stub(lua: &Lua, source: String) -> mlua::Result<Value> {
    let file = lua.create_table()?;
    let lines = source.lines().map(str::to_string).collect::<Vec<_>>();
    file.set(
        "lines",
        lua.create_function(move |lua, _: Value| create_lines_iterator(lua, lines.clone()))?,
    )?;
    file.set("close", lua.create_function(|_, _: Value| Ok(true))?)?;
    Ok(Value::Table(file))
}

fn create_lines_iterator(lua: &Lua, lines: Vec<String>) -> mlua::Result<Function> {
    let index = Arc::new(Mutex::new(0usize));
    lua.create_function(move |lua, ()| {
        let mut index =
            index.lock().map_err(|_| mlua::Error::external("io lines lock poisoned"))?;
        let Some(line) = lines.get(*index) else {
            return Ok(Value::Nil);
        };
        *index += 1;
        Ok(Value::String(lua.create_string(line)?))
    })
}

fn create_write_file_stub(lua: &Lua) -> mlua::Result<Value> {
    let file = lua.create_table()?;
    file.set(
        "write",
        lua.create_function(|_, (_self, _args): (Value, Variadic<Value>)| Ok(true))?,
    )?;
    file.set("close", lua.create_function(|_, _: Value| Ok(true))?)?;
    Ok(Value::Table(file))
}

fn lua_os_now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy)]
struct LuaDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    weekday: u32,
    yearday: u32,
}

fn unix_seconds_to_utc_datetime(seconds: i64) -> LuaDateTime {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    LuaDateTime {
        year,
        month,
        day,
        hour: seconds_of_day / 3_600,
        minute: (seconds_of_day % 3_600) / 60,
        second: seconds_of_day % 60,
        // Lua's wday is 1-based with Sunday == 1. 1970-01-01 was Thursday.
        weekday: ((days + 4).rem_euclid(7) + 1) as u32,
        yearday: yearday(year, month, day),
    }
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

fn yearday(year: i32, month: u32, day: u32) -> u32 {
    const COMMON_MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut result = day;
    for m in 1..month {
        result += COMMON_MONTH_DAYS[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            result += 1;
        }
    }
    result
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn format_lua_date(format: &str, date: LuaDateTime) -> String {
    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('Y') => output.push_str(&format!("{:04}", date.year)),
            Some('m') => output.push_str(&format!("{:02}", date.month)),
            Some('d') => output.push_str(&format!("{:02}", date.day)),
            Some('H') => output.push_str(&format!("{:02}", date.hour)),
            Some('M') => output.push_str(&format!("{:02}", date.minute)),
            Some('S') => output.push_str(&format!("{:02}", date.second)),
            Some('%') => output.push('%'),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }
    output
}

#[derive(Debug)]
struct EventObserveBoolState {
    is_on: bool,
}

#[derive(Debug)]
struct EventObserveTimerState {
    value: i32,
}

#[derive(Debug)]
struct EventMinIntervalState {
    last_execution_ms: Option<i32>,
}

/// beatoraja の `EventUtility` 相当。CustomEvent 用 callback 生成器を提供する。
fn create_event_util_module(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;

    table.set(
        "event_observe_turn_true",
        lua.create_function(|lua, (observed, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: false }));
            lua.create_function(move |_, ()| {
                let on = observed.call::<bool>(())?;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event observe lock poisoned"))?;
                if state.is_on != on {
                    state.is_on = on;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveTimerState { value: TIMER_OFF_VALUE }));
            lua.create_function(move |_, ()| {
                let value = timer.call::<i32>(())?;
                let mut state =
                    state.lock().map_err(|_| mlua::Error::external("event timer lock poisoned"))?;
                if value != state.value && value != TIMER_OFF_VALUE {
                    state.value = value;
                    action.call::<()>(())?;
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer_on",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: false }));
            lua.create_function(move |_, ()| {
                let on = timer.call::<i32>(())? != TIMER_OFF_VALUE;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event timer-on lock poisoned"))?;
                if state.is_on != on {
                    state.is_on = on;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer_off",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: true }));
            lua.create_function(move |_, ()| {
                let off = timer.call::<i32>(())? == TIMER_OFF_VALUE;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event timer-off lock poisoned"))?;
                if state.is_on != off {
                    state.is_on = off;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_min_interval",
        lua.create_function(|lua, (min_interval_ms, action): (i32, Function)| {
            let state = Arc::new(Mutex::new(EventMinIntervalState { last_execution_ms: None }));
            lua.create_function(move |_, ()| {
                let now = lua_load_now_ms();
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event interval lock poisoned"))?;
                let should_run = state
                    .last_execution_ms
                    .is_none_or(|last| now.saturating_sub(last) >= min_interval_ms);
                if should_run {
                    state.last_execution_ms = Some(now);
                    action.call::<()>(())?;
                }
                Ok(true)
            })
        })?,
    )?;

    Ok(Value::Table(table))
}

fn create_luajava_stub(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set(
        "bindClass",
        lua.create_function(|lua, _class_name: String| create_luajava_object_stub(lua))?,
    )?;
    table.set(
        "newInstance",
        lua.create_function(|lua, (_class_name, _args): (String, Variadic<Value>)| {
            create_luajava_object_stub(lua)
        })?,
    )?;
    table.set(
        "createProxy",
        lua.create_function(|lua, _: Variadic<Value>| create_luajava_object_stub(lua))?,
    )?;
    Ok(Value::Table(table))
}

fn create_luajava_object_stub(lua: &Lua) -> mlua::Result<Value> {
    let object = lua.create_table()?;
    let metatable = lua.create_table()?;
    metatable.set(
        "__index",
        lua.create_function(|lua, (_table, _key): (Value, Value)| create_luajava_object_stub(lua))?,
    )?;
    metatable.set(
        "__call",
        lua.create_function(|lua, (_self, _args): (Value, Variadic<Value>)| {
            create_luajava_object_stub(lua)
        })?,
    )?;
    object.set_metatable(Some(metatable));
    Ok(Value::Table(object))
}

/// beatoraja の `TimerUtility` 相当。Lua スキンが `require("timer_util")` できるようにする。
fn create_timer_util_module(lua: &Lua, probe: Arc<Mutex<MainStateProbe>>) -> mlua::Result<Value> {
    let table = lua.create_table()?;

    table.set(
        "now_timer",
        lua.create_function(|_, timer_value: i32| {
            Ok(if timer_value != TIMER_OFF_VALUE {
                lua_load_now_micros().saturating_sub(timer_value.max(0))
            } else {
                0
            })
        })?,
    )?;
    table.set(
        "is_timer_on",
        lua.create_function(|_, timer_value: i32| Ok(timer_value != TIMER_OFF_VALUE))?,
    )?;
    table.set(
        "is_timer_off",
        lua.create_function(|_, timer_value: i32| Ok(timer_value == TIMER_OFF_VALUE))?,
    )?;

    let probe_for_timer_function = probe.clone();
    table.set(
        "timer_function",
        lua.create_function(move |lua, timer_id: i32| {
            let probe = probe_for_timer_function.clone();
            lua.create_function(move |_, _: Value| {
                Ok(probe
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                    .timer(timer_id))
            })
        })?,
    )?;

    let probe_for_observe = probe.clone();
    table.set(
        "timer_observe_boolean",
        lua.create_function(move |lua, observed: Function| {
            let observe = infer_is_gauge_iidx_global_observe(lua, &observed)
                .or_else(|| infer_boolean_predicate(&observed, &probe_for_observe, None))
                .or_else(|| infer_constant_boolean(&observed))
                .unwrap_or_else(|| "number(0) < 0".to_string());
            let timer_id = {
                let mut probe = probe_for_observe
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?;
                let timer_id = probe.next_dynamic_timer_id;
                probe.next_dynamic_timer_id += 1;
                probe.dynamic_timers.push((timer_id, observe));
                timer_id
            };
            let state = Arc::new(Mutex::new(TimerObserveState { timer_value: TIMER_OFF_VALUE }));
            let observed_for_timer = observed.clone();
            let inner = lua.create_function(move |_, ()| {
                let on = observed_for_timer.call::<bool>(())?;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("timer observe lock poisoned"))?;
                if on && state.timer_value == TIMER_OFF_VALUE {
                    state.timer_value = lua_load_now_ms();
                } else if !on && state.timer_value != TIMER_OFF_VALUE {
                    state.timer_value = TIMER_OFF_VALUE;
                }
                Ok(state.timer_value)
            })?;
            let map: Table = lua.globals().get("bmz_timer_fn_map")?;
            map.set(inner.clone(), timer_id)?;
            Ok(inner)
        })?,
    )?;

    table.set(
        "new_passive_timer",
        lua.create_function(|lua, ()| {
            let state = Arc::new(Mutex::new(TimerObserveState { timer_value: TIMER_OFF_VALUE }));
            let passive = lua.create_table()?;
            let state_for_timer = state.clone();
            passive.set(
                "timer",
                lua.create_function(move |_, ()| {
                    Ok(state_for_timer
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value)
                })?,
            )?;
            let state_for_turn_on = state.clone();
            passive.set(
                "turn_on",
                lua.create_function(move |_, ()| {
                    let mut state = state_for_turn_on
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?;
                    if state.timer_value == TIMER_OFF_VALUE {
                        state.timer_value = lua_load_now_micros();
                    }
                    Ok(())
                })?,
            )?;
            let state_for_turn_on_reset = state.clone();
            passive.set(
                "turn_on_reset",
                lua.create_function(move |_, ()| {
                    state_for_turn_on_reset
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value = lua_load_now_micros();
                    Ok(())
                })?,
            )?;
            passive.set(
                "turn_off",
                lua.create_function(move |_, ()| {
                    state
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value = TIMER_OFF_VALUE;
                    Ok(())
                })?,
            )?;
            Ok(passive)
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

fn default_property_op(property: &JsonValue, items: &[JsonValue]) -> Option<i64> {
    if let Some(default_name) = property.get("def").and_then(JsonValue::as_str)
        && let Some(op) = option_name_to_op(items, default_name)
    {
        return Some(op);
    }
    items.first().and_then(|item| item.get("op")).and_then(json_integer)
}

fn option_name_to_op(items: &[JsonValue], value: &str) -> Option<i64> {
    items.iter().find_map(|item| {
        (item.get("name").and_then(JsonValue::as_str) == Some(value))
            .then(|| item.get("op").and_then(json_integer))
            .flatten()
    })
}

fn json_integer(value: &JsonValue) -> Option<i64> {
    value.as_i64().or_else(|| {
        let value = value.as_f64()?;
        (value.is_finite()
            && value.fract() == 0.0
            && value >= i64::MIN as f64
            && value <= i64::MAX as f64)
            .then_some(value as i64)
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct LuaSkinOffsetValue {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    a: i32,
}

fn skin_config_offsets_from_header(header: &JsonValue) -> BTreeMap<String, LuaSkinOffsetValue> {
    let mut result = BTreeMap::new();
    let Some(offsets) = header.get("offset").and_then(JsonValue::as_array) else {
        return result;
    };

    for offset in offsets {
        let Some(name) = offset.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        result.insert(name.to_string(), LuaSkinOffsetValue::default());
    }

    result
}

/// スキン設定パネルで選んだファイル選択を、filepath 定義の `path` グロブごとに
/// 集める。キーは `path` グロブ (区切りを `/` に正規化)、値は選択ファイルの
/// スキンルート相対パス。選択が無い / 空の定義は含めない。
fn skin_files_from_header(
    root: &Path,
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
        let choice = selected
            .get(name)
            .filter(|choice| !choice.is_empty())
            .cloned()
            .or_else(|| default_skin_file_from_filepath(root, &normalized_path, filepath));
        if let Some(choice) = choice {
            result.insert(normalized_path, choice);
        }
    }
    result
}

fn skin_named_files_from_header(
    root: &Path,
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
        let choice = selected
            .get(name)
            .filter(|choice| !choice.is_empty())
            .cloned()
            .or_else(|| default_skin_file_from_filepath(root, &normalized_path, filepath));
        if let Some(choice) = choice {
            result.insert(name.to_string(), choice);
        }
    }
    result
}

fn skin_file_dependency_names_from_header(header: &JsonValue) -> BTreeMap<String, String> {
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
const RANDOM_FILE_SELECTION: &str = "Random";

/// `0..len` の範囲でロードごとに変わる擬似乱数インデックスを返す。
/// `RandomState` のプロセス内ランダムキーを使い、追加クレートなしで beatoraja
/// 相当の「毎ロードでランダム」を満たす。
fn random_skin_file_index(len: usize) -> usize {
    use std::hash::BuildHasher;

    debug_assert!(len > 0);
    let hash = std::collections::hash_map::RandomState::new().hash_one(len as u64);
    (hash % len as u64) as usize
}

fn default_skin_file_from_filepath(
    root: &Path,
    normalized_path: &str,
    filepath: &JsonValue,
) -> Option<String> {
    let candidates = skin_file_candidates(root, normalized_path);
    if candidates.is_empty() {
        return None;
    }
    let default_name = filepath.get("def").and_then(JsonValue::as_str).unwrap_or_default();
    if !default_name.is_empty() {
        // def="Random" は具体ファイルへ固定せず、ランダム番兵を既定にする。
        if default_name.eq_ignore_ascii_case(RANDOM_FILE_SELECTION) {
            return Some(RANDOM_FILE_SELECTION.to_string());
        }
        if let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, default_name))
        {
            return Some(candidate_file_name(candidate));
        }
    } else if let Some(candidate) =
        candidates.iter().find(|candidate| filename_matches_def(candidate, "default"))
    {
        return Some(candidate_file_name(candidate));
    }
    candidates.into_iter().next().map(|candidate| candidate_file_name(&candidate))
}

fn skin_file_candidates(root: &Path, normalized_path: &str) -> Vec<String> {
    let requested_path = strip_beatoraja_asset_filter(normalized_path);
    let Some((prefix, suffix)) = requested_path.split_once('*') else {
        return vec![requested_path.to_string()];
    };
    if suffix.contains('*') {
        return Vec::new();
    }
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let (directory_prefix, name_prefix) = prefix.split_at(slash);
    let dir = root.join(directory_prefix);
    let mut candidates = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return candidates;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(name_prefix) {
            continue;
        }
        if let Some(nested_suffix) = suffix.strip_prefix('/') {
            let candidate = format!("{directory_prefix}{name}/{nested_suffix}");
            if root.join(&candidate).exists() {
                candidates.push(candidate);
            }
        } else if name.ends_with(suffix) {
            candidates.push(format!("{directory_prefix}{name}"));
        }
    }
    candidates.sort();
    candidates
}

fn filename_matches_def(candidate: &str, default_name: &str) -> bool {
    let file_name = Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.eq_ignore_ascii_case(default_name) {
        return true;
    }
    let stem = Path::new(file_name).file_stem().and_then(|stem| stem.to_str()).unwrap_or(file_name);
    if stem.eq_ignore_ascii_case(default_name) {
        return true;
    }
    filepath_def_acronym(default_name).is_some_and(|acronym| {
        let stem_lower = stem.to_ascii_lowercase();
        let acronym_lower = acronym.to_ascii_lowercase();
        stem_lower == acronym_lower || stem_lower.starts_with(&acronym_lower)
    })
}

fn filepath_def_acronym(default_name: &str) -> Option<String> {
    if !default_name.contains('-') {
        return None;
    }
    let acronym = default_name
        .split('-')
        .filter_map(|part| part.chars().find(|ch| ch.is_ascii_alphanumeric()))
        .collect::<String>();
    (!acronym.is_empty()).then_some(acronym)
}

fn candidate_file_name(candidate: &str) -> String {
    Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or(candidate).to_string()
}

/// ユーザ選択のスキンルート相対パスを解決する。
///
/// 絶対パスやスキンルート外への脱出を含む選択は無効として `None` を返す。
/// 通常の候補解決経路 (`skin_config_get_path` 本体) と挙動を揃え、
/// ファイル / ディレクトリの双方を許可する (Lua スキンは
/// `skin_config.get_path("dir/*") .. "/foo.lua"` の形でディレクトリ選択を
/// 連結に使うパターンがある)。
fn resolve_selected_skin_path(root: &Path, selected: &str) -> Option<PathBuf> {
    let relative = Path::new(selected);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    let candidate = root.join(relative);
    candidate.exists().then_some(candidate)
}

fn skin_config_get_path(
    root: &Path,
    requested: &str,
    skin_files: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let requested_path = strip_beatoraja_asset_filter(requested);
    let relative_path = Path::new(requested_path);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        bail!("skin_config.get_path escapes skin root: {requested}");
    }

    // ユーザがスキン設定パネルで「ランダム」を選んだときは、候補からロードごとに
    // ランダムに選ぶ (beatoraja のファイル選択 "Random" 相当)。
    let want_random =
        skin_files.get(&requested.replace('\\', "/")).is_some_and(|s| s == RANDOM_FILE_SELECTION);

    // ユーザがスキン設定パネルで選んだファイルを最優先で返す。
    // 選択が存在しない / ファイルが消えている場合は従来通り候補解決へ委ねる。
    if !want_random {
        if let Some(selected) = skin_files.get(&requested.replace('\\', "/"))
            && let Some(path) =
                resolve_selected_skin_path_for_pattern(root, requested_path, selected)
        {
            return Ok(path);
        }
        if let Some(path) =
            resolve_selected_skin_path_for_wildcard_child(root, requested_path, skin_files)
        {
            return Ok(path);
        }
    }

    let Some((prefix, suffix)) = requested_path.split_once('*') else {
        return Ok(root.join(requested_path));
    };
    if suffix.contains('*') {
        bail!("skin_config.get_path supports only one wildcard: {requested}");
    }

    let slash = prefix.rfind(['/', '\\']).map(|index| index + 1).unwrap_or(0);
    let (directory_prefix, name_prefix) = prefix.split_at(slash);
    let dir = root.join(directory_prefix);
    let suffix = suffix.replace('\\', "/");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("failed to read skin_config path dir: {}", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(name_prefix) {
            continue;
        }
        let candidate_relative = if let Some(nested_suffix) = suffix.strip_prefix('/') {
            format!("{directory_prefix}{name}/{nested_suffix}")
        } else {
            if !name.ends_with(&suffix) {
                continue;
            }
            format!("{directory_prefix}{name}")
        };
        let candidate = root.join(candidate_relative);
        if candidate.exists() {
            candidates.push(candidate);
        }
    }
    if candidates.is_empty() {
        bail!("skin_config path not found: {requested}");
    }
    let index = if want_random { random_skin_file_index(candidates.len()) } else { 0 };
    Ok(candidates.swap_remove(index))
}

fn resolve_selected_skin_path_for_wildcard_child(
    root: &Path,
    requested: &str,
    skin_files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let (requested_prefix, requested_suffix) = requested.split_once('*')?;
    for (configured, selected) in skin_files {
        let (configured_prefix, configured_suffix) = configured.split_once('*')?;
        if requested_prefix != configured_prefix {
            continue;
        }
        let wildcard = wildcard_from_selection(configured_prefix, configured_suffix, selected)?;
        let candidate = format!("{requested_prefix}{wildcard}{requested_suffix}");
        if let Some(path) = resolve_selected_skin_path(root, &candidate) {
            return Some(path);
        }
    }
    None
}

fn resolve_selected_skin_path_for_pattern(
    root: &Path,
    pattern: &str,
    selected: &str,
) -> Option<PathBuf> {
    if let Some(path) = resolve_selected_skin_path(root, selected) {
        return Some(path);
    }
    let pattern = strip_beatoraja_asset_filter(pattern).replace('\\', "/");
    let star = pattern.find('*')?;
    let prefix = &pattern[..star];
    let slash = prefix.rfind(['/', '\\']).map(|index| index + 1).unwrap_or(0);
    let directory_prefix = &prefix[..slash];
    resolve_selected_skin_path(root, &format!("{directory_prefix}{}", selected.replace('\\', "/")))
}

fn wildcard_from_selection<'a>(
    configured_prefix: &str,
    configured_suffix: &str,
    selected: &'a str,
) -> Option<&'a str> {
    selected
        .strip_prefix(configured_prefix)
        .and_then(|rest| rest.strip_suffix(configured_suffix).or(Some(rest)))
        .or_else(|| {
            let name_prefix = configured_prefix.rsplit(['/', '\\']).next().unwrap_or_default();
            selected
                .strip_prefix(name_prefix)
                .and_then(|rest| rest.strip_suffix(configured_suffix).or(Some(rest)))
        })
}

fn strip_beatoraja_asset_filter(path: &str) -> &str {
    path.split_once('|').map_or(path, |(asset_path, _)| asset_path)
}

/// `Path::canonicalize` returns Windows extended-length (`\\?\`) verbatim paths.
/// Verbatim paths reject `/` as a separator, but beatoraja Lua skins build paths
/// by string concatenation (e.g. `skin_config.get_path("_font/*") .. "/set.lua"`),
/// so a verbatim sandbox root makes every such `dofile`/`require` fail with a
/// path-syntax error. Strip the verbatim prefix so derived paths stay normal and
/// tolerate mixed separators. No-op on non-Windows.
fn canonicalize_skin_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize().map(simplify_verbatim_path)
}

#[cfg(windows)]
fn simplify_verbatim_path(path: PathBuf) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        // Only simplify regular drive paths like `C:\dir`; leave device paths alone.
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    path
}

#[cfg(not(windows))]
fn simplify_verbatim_path(path: PathBuf) -> PathBuf {
    path
}

fn resolve_lua_path(root: &Path, requested: &str, module: bool) -> Result<PathBuf> {
    let relative = if module { requested.replace('.', "/") } else { requested.to_string() };
    let relative_path = Path::new(&relative);
    if relative_path.is_absolute() {
        let canonical = canonicalize_skin_path(relative_path)?;
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
        if let Some(path) = resolve_beatoraja_skin_alias(root, &candidate) {
            return Ok(path);
        }
        let path = root.join(candidate);
        if path.is_file() {
            let canonical = canonicalize_skin_path(&path)?;
            if !canonical.starts_with(root) {
                bail!("lua path escapes skin root: {}", canonical.display());
            }
            return Ok(canonical);
        }
    }

    bail!("lua file not found: {requested}");
}

fn resolve_skin_io_path(root: &Path, requested: &str) -> Result<PathBuf> {
    let relative = requested.replace('\\', "/");
    let relative_path = Path::new(&relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        bail!("io path escapes skin root: {requested}");
    }

    if let Some(path) = resolve_beatoraja_skin_alias(root, &relative) {
        return Ok(path);
    }

    let path = root.join(&relative);
    let canonical = canonicalize_skin_path(&path)?;
    if !canonical.starts_with(root) {
        bail!("io path escapes skin root: {}", canonical.display());
    }
    Ok(canonical)
}

fn resolve_beatoraja_skin_alias(root: &Path, relative: &str) -> Option<PathBuf> {
    let rest = relative.strip_prefix("skin/")?;
    let (skin_name, skin_relative) = rest.split_once('/')?;
    if let Some(canonical) = canonicalize_skin_child(root, skin_relative) {
        return Some(canonical);
    }
    for ancestor in root.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) != Some(skin_name) {
            continue;
        }
        if let Some(canonical) = canonicalize_skin_child(ancestor, skin_relative) {
            return Some(canonical);
        }
    }
    None
}

fn canonicalize_skin_child(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = root.join(relative);
    if !path.is_file() {
        return None;
    }
    let Ok(root) = canonicalize_skin_path(root) else {
        return None;
    };
    let Ok(canonical) = canonicalize_skin_path(&path) else {
        return None;
    };
    canonical.starts_with(&root).then_some(canonical)
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

fn infer_m_select_result_graph_height_expr(path: &str) -> Option<String> {
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

fn lua_path_array_index(path: &str, marker: &str) -> Option<i64> {
    let (_, rest) = path.split_once(marker)?;
    let (index, _) = rest.split_once(']')?;
    index.parse().ok()
}

fn lua_value_to_json(
    lua: &Lua,
    value: Value,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
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
        Value::String(value) => JsonValue::String(value.to_string_lossy()),
        Value::Table(table) => lua_table_to_json(
            lua,
            table,
            path,
            depth + 1,
            warnings,
            main_state_probe,
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

fn peaceful_gauge_lead_glow_id(id: &str) -> Option<(&str, bool)> {
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

fn is_peaceful_gauge_lead_glow_destination(entries: &[(Value, Value)]) -> bool {
    let Some(Value::Table(dst)) = entries.iter().find_map(|(key, value)| {
        matches!(key, Value::String(key) if key.as_bytes() == b"dst").then_some(value)
    }) else {
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

fn lua_table_to_json(
    lua: &Lua,
    table: Table,
    path: &str,
    depth: usize,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    table_budget: &mut TableBudget,
) -> Result<JsonValue> {
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        entries.push(pair?);
    }
    table_budget.consume(entries.len(), path)?;

    if entries.is_empty() {
        return Ok(JsonValue::Array(Vec::new()));
    }

    let mut integer_keys = Vec::new();
    let mut has_non_integer_key = false;
    for (key, value) in &entries {
        if matches!(value, Value::Nil) {
            continue;
        }
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
                lua,
                value,
                &format!("{path}[{}]", index + 1),
                depth,
                warnings,
                main_state_probe,
                table_budget,
            )?);
        }
        return Ok(JsonValue::Array(values));
    }

    if !integer_keys.is_empty() {
        warnings.push(format!("mixed lua table converted to object at {path}"));
    }
    let object_id = lua_object_id(&entries);
    let gauge_lead_glow_destination = object_id
        .as_deref()
        .filter(|_| path.contains(".destination["))
        .and_then(|id| peaceful_gauge_lead_glow_id(id))
        .filter(|_| is_peaceful_gauge_lead_glow_destination(&entries))
        .and_then(|(group, below_border)| {
            let mut probe = main_state_probe.lock().ok()?;
            let occurrence = probe
                .gauge_lead_glow_occurrences
                .entry(object_id.as_deref()?.to_string())
                .or_default();
            let part = *occurrence + 1;
            *occurrence += 1;
            Some((group.to_string(), below_border, part))
        });
    let keylogger_destination = object_id.as_deref().and_then(parse_keylogger_destination_id);
    let keylogger_slot = if path.contains(".destination[") && keylogger_destination.is_some() {
        object_id.as_deref().and_then(|id| {
            let mut probe = main_state_probe.lock().ok()?;
            let occurrence =
                probe.keylogger_destination_occurrences.entry(id.to_string()).or_default();
            let slot = *occurrence % 16 + 1;
            *occurrence += 1;
            Some(slot)
        })
    } else {
        None
    };
    let mut object = JsonMap::new();
    for (key, value) in entries {
        let key = lua_key_to_json_key(key, path, warnings)?;
        if matches!(value, Value::Nil) {
            continue;
        }
        if let Value::Function(function) = &value {
            if key == "value" {
                let is_graph = path.contains(".graph[");
                if matches!(object_id.as_deref(), Some("val-hits-per-sec")) {
                    object.insert(
                        "value_expr".to_string(),
                        JsonValue::String("bmz:keylogger_nps".to_string()),
                    );
                    continue;
                }
                if is_graph
                    && let Some(value_expr) =
                        object_id.as_deref().and_then(keylogger_graph_value_expr_from_id)
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                    continue;
                }
                if !is_graph
                    && path.contains(".imageset[")
                    && let Some(ref_id) = infer_gauge_type_imageset_ref(function, main_state_probe)
                {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                    continue;
                }
                if !is_graph
                    && path.contains(".text[")
                    && let Some(value_expr) = infer_course_table_text_expr(
                        function,
                        object_id.as_deref(),
                        main_state_probe,
                    )
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                    continue;
                }
                if !is_graph
                    && path.contains(".text[")
                    && let Some(ref_id) = infer_main_state_text_ref(function, main_state_probe)
                {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                    continue;
                }
                if !is_graph
                    && path.contains(".slider[")
                    && let Some(value_expr) =
                        infer_slider_value_expr(function, object_id.as_deref(), main_state_probe)
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                    continue;
                }
                if !is_graph
                    && let Some(value_expr) = infer_bmz_builtin_value_expr(
                        function,
                        object_id.as_deref(),
                        main_state_probe,
                    )
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                    continue;
                }
                if !is_graph
                    && let Some(ref_id) = infer_gated_number_ref(function, main_state_probe)
                {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                    continue;
                }
                if !is_graph
                    && let Some(ref_id) = infer_main_state_number_ref(function, main_state_probe)
                {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                    continue;
                }
                if is_graph
                    && let Some(graph_type) =
                        infer_fast_slow_ratio_graph_type(function, main_state_probe)
                {
                    object.insert(
                        "type".to_string(),
                        JsonValue::Number(JsonNumber::from(graph_type)),
                    );
                } else if !is_graph
                    && let Some(expr) = infer_main_state_number_expr(function, main_state_probe)
                {
                    object.insert("expr".to_string(), JsonValue::String(expr));
                } else if is_graph && matches!(object_id.as_deref(), Some("default_chart_gauge")) {
                    object.insert(
                        "value_expr".to_string(),
                        JsonValue::String("bmz:default_chart_gauge".to_string()),
                    );
                } else if !is_graph
                    && matches!(object_id.as_deref(), Some("default_chart_total_count"))
                {
                    object.insert(
                        "value_expr".to_string(),
                        JsonValue::String("bmz:default_chart_total_count".to_string()),
                    );
                } else if let Some(value_expr) = infer_value_float_expr(function, main_state_probe)
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                } else if path.contains(".text[")
                    && let Some(ref_id) = infer_constant_text_ref_at_load(function)
                {
                    object.insert("ref".to_string(), JsonValue::Number(JsonNumber::from(ref_id)));
                } else if path.contains(".text[")
                    && let Some(text) = infer_constant_text_at_load(function)
                {
                    object.insert("constantText".to_string(), JsonValue::String(text));
                } else if let Some(value_expr) = infer_constant_number_at_load(function) {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
                } else {
                    warnings.push(format!("skipping unsupported value function at {path}.{key}"));
                }
                continue;
            }
            if key == "act"
                && let Some(event_id) = infer_constant_integer_at_load(function)
            {
                object.insert(key.clone(), JsonValue::Number(JsonNumber::from(event_id)));
                continue;
            }
            if key == "draw" {
                if let Some((group, below_border, part)) = &gauge_lead_glow_destination {
                    object.insert(
                        key.clone(),
                        JsonValue::String(format!(
                            "gauge_lead_glow({group},{part},{})",
                            if *below_border { "below" } else { "above" }
                        )),
                    );
                    continue;
                }
                if let (Some((graph_kind, lane, Some(kind))), Some(slot)) =
                    (keylogger_destination, keylogger_slot)
                {
                    object.insert(
                        key.clone(),
                        JsonValue::String(format!("keylogger_{graph_kind}({lane},{slot},{kind})")),
                    );
                    continue;
                }
                if let Some(draw) =
                    infer_boolean_predicate(function, main_state_probe, object_id.as_deref())
                {
                    object.insert(key.clone(), JsonValue::String(draw));
                } else {
                    warnings.push(format!("skipping unsupported draw function at {path}.{key}"));
                }
                continue;
            }
            if key == "timer" {
                if let (Some((_, lane, _)), Some(slot)) = (keylogger_destination, keylogger_slot) {
                    object.insert(
                        "timer_expr".to_string(),
                        JsonValue::String(format!("bmz:keylogger_event:{lane}:{slot}")),
                    );
                    continue;
                }
                if path.contains(".customTimers[")
                    && let Some(id) = object_id.as_deref().and_then(|id| id.parse::<i32>().ok())
                    && let Some((source_timer, delay_ms)) =
                        infer_fixed_delay_timer(function, main_state_probe)
                {
                    if let Ok(mut probe) = main_state_probe.lock()
                        && !probe.fixed_delay_timers.iter().any(|(existing, _, _)| *existing == id)
                    {
                        probe.fixed_delay_timers.push((id, source_timer, delay_ms));
                    }
                    continue;
                }
                let map: Table = lua.globals().get("bmz_timer_fn_map")?;
                if let Ok(timer_id) = map.get::<i32>(function.clone()) {
                    object.insert(key.clone(), JsonValue::Number(JsonNumber::from(timer_id)));
                    continue;
                }
                if let Some(timer_id) = infer_timer_function_ref(function, main_state_probe) {
                    object.insert(key.clone(), JsonValue::Number(JsonNumber::from(timer_id)));
                    continue;
                }
            }
        }
        if is_unsupported_json_field_value(&value) {
            if should_silently_skip_loader_field(path, &key, &value) {
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
                table_budget,
            )?,
        );
    }
    Ok(JsonValue::Object(object))
}

fn keylogger_graph_value_expr_from_id(id: &str) -> Option<String> {
    let rest = id.strip_prefix("keylogger-graph-")?;
    let mut parts = rest.split('-');
    let graph_kind = parts.next()?;
    let lane = parts.next()?.parse::<usize>().ok()?;
    let layer = parts.next()?;
    if parts.next().is_some()
        || !matches!(graph_kind, "judge" | "fastslow")
        || lane == 0
        || !matches!(layer, "cool" | "great" | "good" | "bad" | "fast" | "slow")
    {
        return None;
    }
    Some(format!("bmz:keylogger_graph:{graph_kind}:{lane}:{layer}"))
}

fn parse_keylogger_destination_id(id: &str) -> Option<(&'static str, usize, Option<&str>)> {
    if let Some(rest) = id.strip_prefix("keylogger-note-judge-") {
        let (lane, kind) = rest.split_once('-')?;
        return Some(("judge", lane.parse().ok()?, Some(kind)));
    }
    if let Some(rest) = id.strip_prefix("keylogger-note-fastslow-") {
        let (lane, kind) = rest.split_once('-')?;
        return Some(("fastslow", lane.parse().ok()?, Some(kind)));
    }
    let lane = id.strip_prefix("keylogger-note-")?.parse().ok()?;
    Some(("plain", lane, None))
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

/// Rm-skin `getDummyNumber(ref)` — `number(101) < 1` なら 0、でなければ `number(ref)`。
fn infer_gated_number_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    const GATE_REF: i32 = 101;
    let refs = collect_number_refs(function, main_state_probe)?;
    if !refs.contains(&GATE_REF) {
        return None;
    }
    let target = if refs.len() == 1 {
        GATE_REF
    } else if refs.len() == 2 {
        if refs[0] == GATE_REF && refs[1] == GATE_REF {
            GATE_REF
        } else {
            refs.iter().copied().find(|ref_id| *ref_id != GATE_REF)?
        }
    } else {
        return None;
    };
    let gated_off =
        call_number_expr_with_values(function, main_state_probe, BTreeMap::from([(GATE_REF, 0)]))?;
    if gated_off != 0 {
        return None;
    }
    let mut open_values = BTreeMap::from([(GATE_REF, 5), (target, 7)]);
    if target == GATE_REF {
        open_values.insert(GATE_REF, 7);
    }
    let open_on = call_number_expr_with_values(function, main_state_probe, open_values.clone())?;
    if open_on != 7 {
        return None;
    }
    open_values.insert(target, 0);
    let open_zero = call_number_expr_with_values(function, main_state_probe, open_values)?;
    if open_zero != 0 {
        return None;
    }
    Some(target)
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
        ("== 0", samples.iter().map(|value| *value == 0).collect::<Vec<_>>()),
        ("< 0", samples.iter().map(|value| *value < 0).collect::<Vec<_>>()),
        ("> 0", samples.iter().map(|value| *value > 0).collect::<Vec<_>>()),
        ("!= 0", samples.iter().map(|value| *value != 0).collect::<Vec<_>>()),
        (">= 0", samples.iter().map(|value| *value >= 0).collect::<Vec<_>>()),
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

fn infer_main_state_event_index_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_event_index_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.event_index_calls.clone();
        probe.end_recording();
        calls
    };
    let event_id = single_number_call(&calls)?;
    let samples = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let observed = samples
        .iter()
        .map(|sample| call_draw_with_event_index(function, main_state_probe, event_id, *sample))
        .collect::<Option<Vec<_>>>()?;
    let enabled = samples
        .iter()
        .zip(observed)
        .filter_map(|(value, enabled)| enabled.then_some(*value))
        .collect::<Vec<_>>();
    if enabled.is_empty() || enabled.len() == samples.len() {
        return None;
    }
    Some(
        enabled
            .into_iter()
            .map(|value| format!("event_index({event_id}) == {value}"))
            .collect::<Vec<_>>()
            .join(" or "),
    )
}

fn call_draw_with_event_index(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    event_id: i32,
    value: i32,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_event_index_recording_with_value(event_id, value);
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

fn infer_main_state_option_number_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let option_id = single_number_call(&collect_option_calls(function, main_state_probe)?)?;
    let mut number_refs =
        collect_number_refs_with_option_value(function, main_state_probe, option_id, true)?;
    number_refs.extend(collect_number_refs_with_option_value(
        function,
        main_state_probe,
        option_id,
        false,
    )?);
    number_refs.sort_unstable();
    number_refs.dedup();
    let number_ref = single_number_call(&number_refs)?;

    let false_zero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 0, option_id, false)?;
    let false_nonzero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 5, option_id, false)?;
    let true_zero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 0, option_id, true)?;
    let true_nonzero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 5, option_id, true)?;

    match (false_zero, false_nonzero, true_zero, true_nonzero) {
        (false, false, false, true) => {
            Some(format!("option({option_id}) && number({number_ref}) != 0"))
        }
        (false, false, true, false) => {
            Some(format!("option({option_id}) && number({number_ref}) == 0"))
        }
        (false, true, false, false) => {
            Some(format!("!option({option_id}) && number({number_ref}) != 0"))
        }
        (true, false, false, false) => {
            Some(format!("!option({option_id}) && number({number_ref}) == 0"))
        }
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

fn infer_end_of_note_shadow_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let timers = collect_timer_refs(function, main_state_probe)?;
    let timer_id = single_number_call(&timers)?;
    if !matches!(timer_id, 143 | 144) {
        return None;
    }

    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.as_slice() != REMAIN_NOTE_REFS {
        return None;
    }

    let samples = [
        (i32::MIN, BTreeMap::from([(106, 0), (110, 0), (111, 0), (112, 0), (113, 0), (114, 0)])),
        (i32::MIN, BTreeMap::from([(106, 5), (110, 5), (111, 0), (112, 0), (113, 0), (114, 0)])),
        (i32::MIN, BTreeMap::from([(106, 5), (110, 2), (111, 1), (112, 1), (113, 0), (114, 0)])),
        (0, BTreeMap::from([(106, 5), (110, 5), (111, 0), (112, 0), (113, 0), (114, 0)])),
        (100, BTreeMap::from([(106, 0), (110, 0), (111, 0), (112, 0), (113, 0), (114, 0)])),
    ];
    for (timer_value, values) in samples {
        let expected = timer_value == i32::MIN && remain_notes_value(&values) == 0;
        let actual = call_draw_with_numbers_and_timers(
            function,
            main_state_probe,
            values,
            BTreeMap::from([(timer_id, timer_value)]),
        )?;
        if actual != expected {
            return None;
        }
    }

    Some(format!("timer({timer_id}) == timer_off and {} == 0", remain_notes_numerator_expr()))
}

fn infer_os_clock_after_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let mut first_true_ms = None;
    let mut saw_clock = false;
    let mut saw_false = false;
    for elapsed_ms in (0..=10_000).step_by(100) {
        {
            main_state_probe.lock().ok()?.begin_os_clock_recording(elapsed_ms as f64 / 1000.0);
        }
        let result = function.call::<Value>(()).ok();
        let (clock_calls, value) = {
            let mut probe = main_state_probe.lock().ok()?;
            let clock_calls = probe.os_clock_calls;
            probe.end_recording();
            let value = match result? {
                Value::Boolean(value) => value,
                _ => return None,
            };
            (clock_calls, value)
        };
        saw_clock |= clock_calls > 0;
        if value {
            first_true_ms = Some(elapsed_ms);
            break;
        }
        saw_false = true;
    }
    let first_true_ms = first_true_ms?;
    (saw_clock && saw_false).then(|| format!("timer(0) >= {first_true_ms}"))
}

fn infer_os_clock_after_option_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let mut first_option_call_ms = None;
    let mut saw_clock = false;
    let mut saw_false_before_option = false;
    for elapsed_ms in (0..=10_000).step_by(100) {
        {
            main_state_probe.lock().ok()?.begin_os_clock_recording(elapsed_ms as f64 / 1000.0);
        }
        let result = function.call::<Value>(()).ok();
        let (clock_calls, option_calls, value) = {
            let mut probe = main_state_probe.lock().ok()?;
            let clock_calls = probe.os_clock_calls;
            let option_calls = probe.option_calls.clone();
            probe.end_recording();
            let value = match result? {
                Value::Boolean(value) => value,
                _ => return None,
            };
            (clock_calls, option_calls, value)
        };
        saw_clock |= clock_calls > 0;
        if option_calls.is_empty() {
            if !value {
                saw_false_before_option = true;
            }
            continue;
        }
        first_option_call_ms = Some(elapsed_ms);
        break;
    }
    let first_option_ms = first_option_call_ms?;
    if !saw_clock || !saw_false_before_option {
        return None;
    }

    let mut option_ids = Vec::<i32>::new();
    for _ in 0..16 {
        let known_true = option_ids.iter().map(|&option_id| (option_id, true)).collect::<Vec<_>>();
        let (calls, value) = call_draw_with_os_clock_options(
            function,
            main_state_probe,
            first_option_ms,
            &known_true,
            false,
        )?;
        let next_option_id = calls.into_iter().find(|call| !option_ids.contains(call));
        if let Some(option_id) = next_option_id {
            option_ids.push(option_id);
            continue;
        }
        if value && !option_ids.is_empty() {
            let mut condition = format!("timer(0) >= {first_option_ms}");
            for option_id in option_ids {
                condition.push_str(&format!(" and option({option_id})"));
            }
            return Some(condition);
        }
        return None;
    }
    None
}

fn call_draw_with_os_clock_options(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    elapsed_ms: i32,
    option_values: &[(i32, bool)],
    default_option_value: bool,
) -> Option<(Vec<i32>, bool)> {
    {
        main_state_probe.lock().ok()?.begin_os_clock_options_recording(
            elapsed_ms as f64 / 1000.0,
            option_values,
            default_option_value,
        );
    }
    let result = function.call::<Value>(()).ok();
    let (calls, value) = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.option_calls.clone();
        probe.end_recording();
        let value = match result? {
            Value::Boolean(value) => value,
            _ => return None,
        };
        (calls, value)
    };
    Some((calls, value))
}

fn collect_timer_refs(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    {
        main_state_probe.lock().ok()?.begin_timer_call_recording(i32::MIN);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.timer_calls.clone();
        probe.end_recording();
        calls
    };
    let mut timers = calls;
    timers.sort_unstable();
    timers.dedup();
    Some(timers)
}

fn call_timer_function_with_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    timer_values: BTreeMap<i32, i32>,
) -> Option<i32> {
    {
        main_state_probe.lock().ok()?.begin_timer_recording_with_values(timer_values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            i32::try_from(value as i64).ok()
        }
        _ => None,
    }
}

fn call_timer_function_with_values_at_time(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    timer_values: BTreeMap<i32, i32>,
    time_value_us: i32,
) -> Option<i32> {
    {
        let mut probe = main_state_probe.lock().ok()?;
        probe.begin_timer_recording_with_values(timer_values);
        probe.time_value_us = time_value_us;
    }
    let result = function.call::<Value>(()).ok();
    {
        let mut probe = main_state_probe.lock().ok()?;
        probe.time_value_us = 1_000_000;
        probe.end_recording();
    }
    match result? {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            i32::try_from(value as i64).ok()
        }
        _ => None,
    }
}

fn event_index_calls_with_timer_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    timer_values: BTreeMap<i32, i32>,
) -> Option<Vec<i32>> {
    {
        main_state_probe.lock().ok()?.begin_timer_recording_with_values(timer_values);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.event_index_calls.clone();
        probe.end_recording();
        calls
    };
    Some(calls)
}

fn call_draw_with_timer_event(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    timer_values: BTreeMap<i32, i32>,
    event_id: i32,
    event_value: i32,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_timer_event_recording_with_values(
            timer_values,
            event_id,
            event_value,
        );
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn keybeam_hold_timer_for_keyon_timer(timer_id: i32) -> Option<i32> {
    match timer_id {
        100..=109 => Some(timer_id - 30),
        110..=117 => Some(timer_id - 30),
        _ => None,
    }
}

fn is_keybeam_keyoff_timer(timer_id: i32) -> bool {
    matches!(timer_id, 120..=137)
}

fn infer_keybeam_timer_event_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let timers = collect_timer_refs(function, main_state_probe)?;
    for keyon_timer in timers.iter().copied() {
        let Some(hold_timer) = keybeam_hold_timer_for_keyon_timer(keyon_timer) else {
            continue;
        };
        if !timers.contains(&hold_timer) {
            continue;
        }

        let active_timers = BTreeMap::from([(keyon_timer, 1)]);
        let event_calls =
            event_index_calls_with_timer_values(function, main_state_probe, active_timers.clone())?;
        let event_id = single_number_call(&event_calls)?;
        let samples = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let observed = samples
            .iter()
            .map(|sample| {
                call_draw_with_timer_event(
                    function,
                    main_state_probe,
                    active_timers.clone(),
                    event_id,
                    *sample,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let enabled = samples
            .iter()
            .zip(observed)
            .filter_map(|(value, enabled)| enabled.then_some(*value))
            .collect::<Vec<_>>();
        if enabled.is_empty() || enabled.len() == samples.len() {
            continue;
        }

        let prefix =
            format!("timer({keyon_timer}) != timer_off and timer({hold_timer}) == timer_off and ");
        return Some(
            enabled
                .into_iter()
                .map(|value| format!("{prefix}event_index({event_id}) == {value}"))
                .collect::<Vec<_>>()
                .join(" or "),
        );
    }
    None
}

fn infer_timer_function_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    let timers = collect_timer_refs(function, main_state_probe)?;
    for timer_id in timers.into_iter().filter(|timer_id| is_keybeam_keyoff_timer(*timer_id)) {
        let sample = main_state_probe.lock().ok()?.time_value_us.saturating_sub(1);
        if call_timer_function_with_values(
            function,
            main_state_probe,
            BTreeMap::from([(timer_id, sample)]),
        ) == Some(sample)
        {
            return Some(timer_id);
        }
    }
    None
}

/// `source timer timestamp + fixed delay` を返し、delay到達前はtimer-offとなる
/// custom timerだけを限定的にIR化する。
fn infer_fixed_delay_timer(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<(i32, i32)> {
    let timers = collect_timer_refs(function, main_state_probe)?;
    let source_timer = *timers.as_slice().first()?;
    if timers.len() != 1 {
        return None;
    }
    let source_time_us = 100_000;
    let returned_start = call_timer_function_with_values_at_time(
        function,
        main_state_probe,
        BTreeMap::from([(source_timer, source_time_us)]),
        i32::MAX / 2,
    )?;
    let delay_us = returned_start.checked_sub(source_time_us)?;
    if delay_us <= 0 || delay_us % 1_000 != 0 {
        return None;
    }
    let delay_ms = delay_us / 1_000;
    if delay_ms > 60_000 {
        return None;
    }
    let before = returned_start.checked_sub(1)?;
    if call_timer_function_with_values_at_time(
        function,
        main_state_probe,
        BTreeMap::from([(source_timer, source_time_us)]),
        before,
    ) != Some(TIMER_OFF_VALUE)
        || call_timer_function_with_values_at_time(
            function,
            main_state_probe,
            BTreeMap::from([(source_timer, source_time_us)]),
            returned_start,
        ) != Some(returned_start)
        || call_timer_function_with_values_at_time(
            function,
            main_state_probe,
            BTreeMap::from([(source_timer, source_time_us)]),
            returned_start.saturating_add(123_000),
        ) != Some(returned_start)
        || call_timer_function_with_values_at_time(
            function,
            main_state_probe,
            BTreeMap::new(),
            returned_start.saturating_add(123_000),
        ) != Some(TIMER_OFF_VALUE)
    {
        return None;
    }
    Some((source_timer, delay_ms))
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
    // beatoraja の gauge id 0..=8 を網羅。6/7/8 (CLASS / EXCLASS / EXHARDCLASS) を
    // 含めることで段位ゲージ用の skin 条件 (例: `gauge_type() >= 6`) を取りこぼさない。
    let samples = [0, 1, 2, 3, 4, 5, 6, 7, 8];
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

fn infer_constant_boolean(function: &Function) -> Option<String> {
    match function.call::<bool>(()).ok() {
        Some(true) => Some("number(0) >= 0".to_string()),
        Some(false) => Some("number(0) < 0".to_string()),
        _ => None,
    }
}

/// Starseeker 等が `return is_gauge_iidx` / `return not is_gauge_iidx` と書くが
/// グローバルを定義しないスキン向け。ロード時に真偽を切り替えて EX-HARD/HAZARD 相当へ写す。
fn infer_is_gauge_iidx_global_observe(lua: &Lua, function: &Function) -> Option<String> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("is_gauge_iidx").ok();
    let selected_gauge_display = globals
        .get::<Table>("skin_config")
        .ok()
        .and_then(|skin_config| skin_config.get::<Table>("option").ok())
        .and_then(|option| option.get::<i64>("グルーヴゲージ表示").ok());

    fn observe_truth(function: &Function) -> Option<bool> {
        match function.call::<Value>(()).ok()? {
            Value::Boolean(value) => Some(value),
            Value::Nil => Some(false),
            _ => None,
        }
    }

    globals.set("is_gauge_iidx", false).ok()?;
    let when_false = observe_truth(function)?;
    globals.set("is_gauge_iidx", true).ok()?;
    let when_true = observe_truth(function)?;

    if let Some(value) = previous {
        globals.set("is_gauge_iidx", value).ok()?;
    } else {
        globals.raw_remove("is_gauge_iidx").ok()?;
    }

    match (when_false, when_true) {
        (false, true) if selected_gauge_display == Some(930) => Some("number(0) < 0".to_string()),
        (true, false) if selected_gauge_display == Some(930) => Some("number(0) >= 0".to_string()),
        (false, true) => Some("gauge_type() == 4 or gauge_type() == 5".to_string()),
        (true, false) => Some("gauge_type() != 4 and gauge_type() != 5".to_string()),
        _ => None,
    }
}

fn infer_boolean_predicate(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    object_id: Option<&str>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe).unwrap_or_default();
    infer_result_average_timing_sign_draw_condition(function, main_state_probe)
        .or_else(|| {
            if refs.len() >= 2 {
                infer_or_of_number_gt_zero(function, main_state_probe)
                    .or_else(|| infer_or_of_number_eq_zero(function, main_state_probe))
                    .or_else(|| infer_or_of_number_lt_zero(function, main_state_probe))
                    .or_else(|| infer_two_number_compare_and(function, main_state_probe))
            } else {
                None
            }
        })
        .or_else(|| infer_float_number_and_number_and_draw(function, main_state_probe))
        .or_else(|| infer_main_state_option_number_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_event_index_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_gauge_type_draw_condition(function, main_state_probe))
        .or_else(|| infer_keybeam_timer_event_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_timer_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_end_of_note_shadow_draw_condition(function, main_state_probe))
        .or_else(|| infer_os_clock_after_draw_condition(function, main_state_probe))
        .or_else(|| infer_os_clock_after_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_judge_fast_slow_draw_condition(function, main_state_probe, object_id))
        .or_else(|| infer_or_of_number_gt_zero(function, main_state_probe))
        .or_else(|| infer_or_of_number_eq_zero(function, main_state_probe))
        .or_else(|| infer_or_of_number_lt_zero(function, main_state_probe))
        .or_else(|| infer_two_number_compare_and(function, main_state_probe))
        .or_else(|| infer_number_eq_zero_with_constant_tail(function, main_state_probe))
        .or_else(|| infer_constant_draw_at_load(function))
}

/// `skin_config.option` のみ等、ロード時に結果が決まる draw function を畳み込む。
fn infer_constant_draw_at_load(function: &Function) -> Option<String> {
    match function.call::<bool>(()).ok() {
        Some(true) => Some("number(0) >= 0".to_string()),
        Some(false) => Some("number(0) < 0".to_string()),
        _ => None,
    }
}

fn infer_constant_text_at_load(function: &Function) -> Option<String> {
    match function.call::<Value>(()).ok()? {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) if value.is_finite() => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn infer_constant_text_ref_at_load(function: &Function) -> Option<i32> {
    let text = infer_constant_text_at_load(function)?;
    let ref_id = text
        .strip_prefix(LUA_TEXT_REF_SENTINEL_PREFIX)?
        .strip_suffix(LUA_TEXT_REF_SENTINEL_SUFFIX)?
        .parse::<i32>()
        .ok()?;
    (1001..=1003).contains(&ref_id).then_some(ref_id)
}

fn repair_keybeam_destination_draws(root: &mut JsonMap<String, JsonValue>) {
    let Some(destinations) = root.get_mut("destination").and_then(JsonValue::as_array_mut) else {
        return;
    };
    for index in 0..destinations.len().saturating_sub(1) {
        let Some((hold_draw, fade_draw)) =
            keybeam_draw_replacements(&destinations[index], &destinations[index + 1])
        else {
            continue;
        };
        if let JsonValue::Object(destination) = &mut destinations[index] {
            destination.insert("draw".to_string(), JsonValue::String(hold_draw));
        }
        if let JsonValue::Object(destination) = &mut destinations[index + 1] {
            destination.insert("draw".to_string(), JsonValue::String(fade_draw));
        }
    }
}

fn keybeam_draw_replacements(hold: &JsonValue, fade: &JsonValue) -> Option<(String, String)> {
    let hold = hold.as_object()?;
    let fade = fade.as_object()?;
    let hold_id = json_string_field(hold, "id")?;
    if !hold_id.starts_with("key-beam-") || hold_id != json_string_field(fade, "id")? {
        return None;
    }
    if json_i32_field(hold, "timer").is_some() || json_i32_field(hold, "loop") == Some(-1) {
        return None;
    }
    if !needs_keybeam_hold_draw_repair(json_string_field(hold, "draw")) {
        return None;
    }

    let fade_timer = json_i32_field(fade, "timer")?;
    if json_i32_field(fade, "loop") != Some(-1) || !is_keybeam_keyoff_timer(fade_timer) {
        return None;
    }
    let keyon_timer = keybeam_keyon_timer_for_keyoff_timer(fade_timer)?;
    let hold_timer = keybeam_hold_timer_for_keyon_timer(keyon_timer)?;
    let fade_draw = json_string_field(fade, "draw")?;
    let hold_draw = keybeam_hold_draw_from_fade_draw(fade_draw, keyon_timer, hold_timer)?;
    let fade_draw = fade_draw
        .split(" or ")
        .map(str::trim)
        .map(|branch| format!("keybeam_fade({fade_timer}) != 0 and {branch}"))
        .collect::<Vec<_>>()
        .join(" or ");
    Some((hold_draw, fade_draw))
}

fn json_string_field<'a>(object: &'a JsonMap<String, JsonValue>, key: &str) -> Option<&'a str> {
    object.get(key)?.as_str()
}

fn json_i32_field(object: &JsonMap<String, JsonValue>, key: &str) -> Option<i32> {
    i32::try_from(object.get(key)?.as_i64()?).ok()
}

fn needs_keybeam_hold_draw_repair(draw: Option<&str>) -> bool {
    match draw.map(str::trim) {
        None | Some("") | Some("number(0) < 0") => true,
        Some(draw) => !draw.contains("timer("),
    }
}

fn keybeam_keyon_timer_for_keyoff_timer(timer_id: i32) -> Option<i32> {
    match timer_id {
        120..=137 => Some(timer_id - 20),
        _ => None,
    }
}

fn keybeam_hold_draw_from_fade_draw(
    fade_draw: &str,
    keyon_timer: i32,
    _hold_timer: i32,
) -> Option<String> {
    let prefix = format!("keybeam_hold({keyon_timer}) != 0 and ");
    let branches = fade_draw
        .split(" or ")
        .map(str::trim)
        .filter(|branch| branch.contains("event_index("))
        .map(|branch| format!("{prefix}{branch}"))
        .collect::<Vec<_>>();
    (!branches.is_empty()).then(|| branches.join(" or "))
}

fn infer_constant_number_at_load(function: &Function) -> Option<String> {
    match function.call::<Value>(()).ok()? {
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) if value.is_finite() => Some(value.to_string()),
        _ => None,
    }
}

fn infer_constant_integer_at_load(function: &Function) -> Option<i64> {
    match function.call::<Value>(()).ok()? {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => Some(value as i64),
        _ => None,
    }
}

fn collect_number_refs(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    let mut calls = Vec::new();
    // Lua の `or` / `and` 短絡評価で片方の number() だけ呼ばれることがあるため、
    // 複数の probe 値で実行して ref を集める。
    for default_value in [5, 0, -1] {
        {
            main_state_probe.lock().ok()?.begin_number_call_recording(default_value);
        }
        let _ = function.call::<Value>(()).ok();
        {
            let mut probe = main_state_probe.lock().ok()?;
            calls.extend(probe.number_calls.iter().copied());
            probe.end_recording();
        }
    }
    calls.sort_unstable();
    calls.dedup();
    Some(calls)
}

fn collect_number_refs_with_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
) -> Option<Vec<i32>> {
    collect_number_refs_with_option_value(function, main_state_probe, option_id, true)
}

fn collect_number_refs_with_option_value(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
    option_value: bool,
) -> Option<Vec<i32>> {
    let mut calls = Vec::new();
    for default_value in [5, 0, -1] {
        {
            main_state_probe.lock().ok()?.begin_number_call_recording_with_option_value(
                default_value,
                option_id,
                option_value,
            );
        }
        let _ = function.call::<Value>(()).ok();
        {
            let mut probe = main_state_probe.lock().ok()?;
            calls.extend(probe.number_calls.iter().copied());
            probe.end_recording();
        }
    }
    calls.sort_unstable();
    calls.dedup();
    Some(calls)
}

fn call_draw_with_numbers(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values(values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn call_draw_with_numbers_and_timers(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
    timers: BTreeMap<i32, i32>,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_timer_recording_with_values(values, timers);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        Value::Nil => Some(false),
        _ => None,
    }
}

fn call_draw_with_number_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    number_ref: i32,
    number_value: i32,
    option_id: i32,
    option_value: bool,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values_and_options(
            BTreeMap::from([(number_ref, number_value)]),
            BTreeMap::from([(option_id, option_value)]),
        );
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn call_number_float_with_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<f64> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values(values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) if value.is_finite() => Some(value),
        _ => None,
    }
}

fn call_number_float_with_values_and_options(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
    options: BTreeMap<i32, bool>,
) -> Option<f64> {
    {
        main_state_probe
            .lock()
            .ok()?
            .begin_number_recording_with_values_and_options(values, options);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) if value.is_finite() => Some(value),
        _ => None,
    }
}

fn verify_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    expected: impl Fn(&BTreeMap<i32, i32>) -> bool,
) -> bool {
    let samples = [-1, 0, 1, 2, 3, 5];
    for &left in &samples {
        for &right in &samples {
            let mut values = BTreeMap::new();
            if refs.len() == 1 {
                values.insert(refs[0], left);
            } else if refs.len() >= 2 {
                values.insert(refs[0], left);
                values.insert(refs[1], right);
                for extra in refs.iter().skip(2) {
                    values.insert(*extra, 0);
                }
            }
            let Some(got) = call_draw_with_numbers(function, main_state_probe, values.clone())
            else {
                return false;
            };
            if got != expected(&values) {
                return false;
            }
        }
    }
    true
}

fn infer_or_of_number_gt_zero(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.is_empty() {
        return None;
    }
    let all_zero = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    if call_draw_with_numbers(function, main_state_probe, all_zero) != Some(false) {
        return None;
    }
    let mut terms = Vec::new();
    for ref_id in &refs {
        let mut only_positive = refs.iter().copied().map(|id| (id, 0)).collect::<BTreeMap<_, _>>();
        only_positive.insert(*ref_id, 5);
        if call_draw_with_numbers(function, main_state_probe, only_positive) == Some(true) {
            terms.push(format!("number({ref_id}) > 0"));
        }
    }
    if terms.is_empty() {
        return None;
    }
    let condition = terms.join(" or ");
    verify_draw_condition(function, main_state_probe, &refs, |values| {
        refs.iter().any(|ref_id| values.get(ref_id).copied().unwrap_or(0) > 0)
    })
    .then_some(condition)
}

fn infer_or_of_number_lt_zero(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.is_empty() {
        return None;
    }
    if refs.len() == 1 {
        let ref_id = refs[0];
        let condition = format!("number({ref_id}) < 0");
        return verify_draw_condition(function, main_state_probe, &refs, |values| {
            values.get(&ref_id).copied().unwrap_or(0) < 0
        })
        .then_some(condition);
    }
    let all_zero = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    if call_draw_with_numbers(function, main_state_probe, all_zero) != Some(false) {
        return None;
    }
    let mut terms = Vec::new();
    for ref_id in &refs {
        let mut only_negative = refs.iter().copied().map(|id| (id, 0)).collect::<BTreeMap<_, _>>();
        only_negative.insert(*ref_id, -1);
        if call_draw_with_numbers(function, main_state_probe, only_negative) == Some(true) {
            terms.push(format!("number({ref_id}) < 0"));
        }
    }
    if terms.is_empty() {
        return None;
    }
    let condition = terms.join(" or ");
    verify_draw_condition(function, main_state_probe, &refs, |values| {
        refs.iter().any(|ref_id| values.get(ref_id).copied().unwrap_or(0) < 0)
    })
    .then_some(condition)
}

fn infer_result_average_timing_sign_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.as_slice() != [374, 375] {
        return None;
    }

    let samples = [(0, 0), (0, 34), (0, -34), (1, 0), (-1, 0), (12, 34), (-12, -34)];
    let observed = samples
        .iter()
        .map(|(integer, afterdot)| {
            call_draw_with_numbers(
                function,
                main_state_probe,
                BTreeMap::from([(374, *integer), (375, *afterdot)]),
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let expected_negative = samples
        .iter()
        .map(|(integer, afterdot)| *integer as f64 + *afterdot as f64 * 0.01 < 0.0)
        .collect::<Vec<_>>();
    if observed == expected_negative {
        return Some("number(374) < 0 or number(375) < 0".to_string());
    }

    let expected_positive = samples
        .iter()
        .map(|(integer, afterdot)| *integer as f64 + *afterdot as f64 * 0.01 > 0.0)
        .collect::<Vec<_>>();
    (observed == expected_positive).then(|| "number(374) > 0 or number(375) > 0".to_string())
}

fn infer_or_of_number_eq_zero(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() < 2 {
        return None;
    }
    let all_positive = refs.iter().copied().map(|ref_id| (ref_id, 5)).collect::<BTreeMap<_, _>>();
    if call_draw_with_numbers(function, main_state_probe, all_positive) != Some(false) {
        return None;
    }
    let mut terms = Vec::new();
    for ref_id in &refs {
        let mut only_zero = refs.iter().copied().map(|id| (id, 5)).collect::<BTreeMap<_, _>>();
        only_zero.insert(*ref_id, 0);
        if call_draw_with_numbers(function, main_state_probe, only_zero) == Some(true) {
            terms.push(format!("number({ref_id}) == 0"));
        }
    }
    if terms.is_empty() {
        return None;
    }
    let condition = terms.join(" or ");
    verify_draw_condition(function, main_state_probe, &refs, |values| {
        refs.iter().any(|ref_id| values.get(ref_id).copied().unwrap_or(0) == 0)
    })
    .then_some(condition)
}

fn infer_two_number_compare_and(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 2 {
        return None;
    }
    let (left, right) = (refs[0], refs[1]);
    for threshold in 0..=5 {
        for &(flip_left, flip_right) in &[(left, right), (right, left)] {
            let condition = format!(
                "number({flip_left}) < number({flip_right}) and number({flip_right}) >= {threshold}"
            );
            if verify_draw_condition(function, main_state_probe, &refs, |values| {
                let a = values.get(&flip_left).copied().unwrap_or(0);
                let b = values.get(&flip_right).copied().unwrap_or(0);
                a < b && b >= threshold
            }) {
                return Some(condition);
            }
            let gt_condition = format!(
                "number({flip_left}) > number({flip_right}) and number({flip_right}) >= {threshold}"
            );
            if verify_draw_condition(function, main_state_probe, &refs, |values| {
                let a = values.get(&flip_left).copied().unwrap_or(0);
                let b = values.get(&flip_right).copied().unwrap_or(0);
                a > b && b >= threshold
            }) {
                return Some(gt_condition);
            }
        }
    }
    None
}

fn infer_number_eq_zero_with_constant_tail(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 1 {
        return None;
    }
    let ref_id = refs[0];
    let zero = call_draw_with_numbers(function, main_state_probe, BTreeMap::from([(ref_id, 0)]))?;
    let nonzero =
        call_draw_with_numbers(function, main_state_probe, BTreeMap::from([(ref_id, 5)]))?;
    if zero && !nonzero {
        return Some(format!("number({ref_id}) == 0"));
    }
    if !zero && nonzero {
        return Some(format!("number({ref_id}) != 0"));
    }
    None
}

fn infer_gauge_type_imageset_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    {
        main_state_probe.lock().ok()?.begin_gauge_type_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let (gauge_calls, number_calls) = {
        let mut probe = main_state_probe.lock().ok()?;
        let gauge_calls = probe.gauge_type_calls;
        let number_calls = probe.number_calls.clone();
        probe.end_recording();
        (gauge_calls, number_calls)
    };
    (gauge_calls > 0 && number_calls.is_empty()).then_some(SKIN_REF_PLAY_GAUGE_TYPE)
}

fn infer_course_table_text_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    if object_id == Some("table") {
        return Some(SKIN_EXPR_COURSE_TABLE_TEXT.to_string());
    }

    let option_calls = collect_option_calls(function, main_state_probe)?;
    if !option_calls.contains(&290) {
        return None;
    }

    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let text_calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.text_calls.clone();
        probe.end_recording();
        calls
    };
    if text_calls.iter().any(|ref_id| (1001..=1003).contains(ref_id)) {
        Some(SKIN_EXPR_COURSE_TABLE_TEXT.to_string())
    } else {
        None
    }
}

fn infer_main_state_text_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let text_calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.text_calls.clone();
        probe.end_recording();
        calls
    };
    single_number_call(&text_calls)
}

fn call_draw_with_float_and_number(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    float_ref: i32,
    float_value: f64,
    number_ref: i32,
    number_value: i32,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_draw_probe(
            BTreeMap::from([(number_ref, number_value)]),
            BTreeMap::from([(float_ref, float_value)]),
        );
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

fn infer_float_number_and_number_and_draw(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let float_refs = collect_float_number_refs(function, main_state_probe)?;
    let number_refs = collect_number_refs(function, main_state_probe)?;
    if float_refs.len() != 1 || number_refs.len() != 1 {
        return None;
    }
    let float_ref = float_refs[0];
    let number_ref = number_refs[0];
    let zero_zero =
        call_draw_with_float_and_number(function, main_state_probe, float_ref, 0.0, number_ref, 0);
    let zero_pos =
        call_draw_with_float_and_number(function, main_state_probe, float_ref, 0.0, number_ref, 5);
    let pos_pos =
        call_draw_with_float_and_number(function, main_state_probe, float_ref, 1.0, number_ref, 5);
    if zero_pos == Some(true) && zero_zero == Some(false) && pos_pos == Some(false) {
        return Some(format!("float_number({float_ref}) == 0 && number({number_ref}) != 0"));
    }
    if pos_pos == Some(true) && zero_pos == Some(false) && zero_zero == Some(false) {
        return Some(format!("float_number({float_ref}) != 0 && number({number_ref}) != 0"));
    }
    if zero_zero == Some(true) && zero_pos == Some(false) && pos_pos == Some(false) {
        return Some(format!("number({number_ref}) == 0"));
    }
    None
}

fn collect_float_number_refs(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    let mut calls = Vec::new();
    for float_value in [0.0_f64, 1.0] {
        {
            main_state_probe
                .lock()
                .ok()?
                .begin_draw_probe(BTreeMap::new(), BTreeMap::from([(113, float_value)]));
        }
        let _ = function.call::<Value>(()).ok();
        {
            let mut probe = main_state_probe.lock().ok()?;
            calls.extend(probe.float_number_calls.iter().copied());
            probe.end_recording();
        }
    }
    calls.sort_unstable();
    calls.dedup();
    (!calls.is_empty()).then_some(calls)
}

fn format_number_sum_expr(refs: &[i32]) -> String {
    refs.iter().map(|ref_id| format!("number({ref_id})")).collect::<Vec<_>>().join("+")
}

fn infer_slider_value_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    match object_id {
        Some("adjustedcover") | Some("adjusted-cover") | Some("adjusted_cover") => {
            Some(SKIN_EXPR_ADJUSTED_COVER.to_string())
        }
        _ => infer_hsfix_dependent_float(function, main_state_probe)
            .map(|_| SKIN_EXPR_ADJUSTED_COVER.to_string()),
    }
}

fn infer_bmz_builtin_value_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    match object_id {
        Some("adjusted-rate-num") | Some("adjusted_rate_num") => {
            Some(SKIN_EXPR_ADJUSTED_RATE.to_string())
        }
        Some("adjusted-rate-adot-num") | Some("adjusted_rate_adot_num") => {
            Some(SKIN_EXPR_ADJUSTED_RATE_ADOT.to_string())
        }
        Some("threshold-num") | Some("threshold_num") | Some("fs-threshold") => {
            Some(SKIN_EXPR_FS_THRESHOLD.to_string())
        }
        Some("val-gauge-percent-integer") => Some(SKIN_EXPR_GAUGE_PERCENT_INTEGER.to_string()),
        Some("val-gauge-percent-fraction") => Some(SKIN_EXPR_GAUGE_PERCENT_FRACTION.to_string()),
        Some("val-gauge-amount-integer") => Some(SKIN_EXPR_GAUGE_AMOUNT_INTEGER.to_string()),
        Some("val-gauge-amount-fraction") => Some(SKIN_EXPR_GAUGE_AMOUNT_FRACTION.to_string()),
        _ => {
            let refs = collect_number_refs(function, main_state_probe)?;
            if refs.iter().any(|ref_id| matches!(ref_id, 160 | 90 | 91 | 314 | 14)) {
                infer_hsfix_dependent_float(function, main_state_probe).map(|_| {
                    if object_id.is_some_and(|id| id.contains("adot") || id.contains("dot")) {
                        SKIN_EXPR_ADJUSTED_RATE_ADOT.to_string()
                    } else {
                        SKIN_EXPR_ADJUSTED_RATE.to_string()
                    }
                })
            } else if collect_option_calls(function, main_state_probe)
                .is_some_and(|options| options.iter().any(|option| (180..=183).contains(option)))
            {
                Some(SKIN_EXPR_FS_THRESHOLD.to_string())
            } else {
                None
            }
        }
    }
}

fn infer_hsfix_dependent_float(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<f64> {
    let number_refs = collect_number_refs(function, main_state_probe)?;
    let float_refs = collect_float_number_refs(function, main_state_probe)?;
    if number_refs.iter().any(|ref_id| matches!(ref_id, 160 | 90 | 91))
        || float_refs.iter().any(|ref_id| matches!(ref_id, 14 | 314))
    {
        Some(0.0)
    } else {
        None
    }
}

fn collect_option_calls(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.option_calls.clone();
        probe.end_recording();
        calls
    };
    (!calls.is_empty()).then_some(calls)
}

fn infer_value_float_expr(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    infer_remain_rate_scaled(function, main_state_probe)
        .or_else(|| infer_number_scalar_multiply(function, main_state_probe))
        .or_else(|| infer_option_weighted_number_sum(function, main_state_probe))
        .or_else(|| infer_weighted_number_ratio_scaled(function, main_state_probe))
        .or_else(|| infer_division_of_number_sums(function, main_state_probe))
}

const REMAIN_NOTE_REFS: [i32; 6] = [106, 110, 111, 112, 113, 114];

fn remain_notes_numerator_expr() -> String {
    "number(106)-number(110)-number(111)-number(112)-number(113)-number(114)".to_string()
}

fn remain_notes_value(values: &BTreeMap<i32, i32>) -> i32 {
    REMAIN_NOTE_REFS
        .iter()
        .map(|ref_id| {
            let value = values.get(ref_id).copied().unwrap_or(0);
            if *ref_id == 106 { value } else { -value }
        })
        .sum()
}

fn infer_remain_rate_scaled(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 6 || !refs.iter().all(|ref_id| REMAIN_NOTE_REFS.contains(ref_id)) {
        return None;
    }
    let mut probe_values = BTreeMap::from([(106, 10)]);
    for ref_id in REMAIN_NOTE_REFS {
        probe_values.entry(ref_id).or_insert(0);
    }
    let scale_sample =
        call_number_float_with_values(function, main_state_probe, probe_values.clone())?;
    let scale = scale_sample.round();
    if (scale - 100.0).abs() > 0.5 && (scale - 10000.0).abs() > 0.5 {
        return None;
    }
    let numerator = remain_notes_numerator_expr();
    let expr = format!("({numerator})/number(106)*{}", scale as i64);
    let expected = |values: &BTreeMap<i32, i32>| {
        let remain: f64 = REMAIN_NOTE_REFS
            .iter()
            .map(|ref_id| {
                let value = values.get(ref_id).copied().unwrap_or(0) as f64;
                if *ref_id == 106 { value } else { -value }
            })
            .sum();
        let total = values.get(&106).copied().unwrap_or(0) as f64;
        if total.abs() < f64::EPSILON { 0.0 } else { remain / total * scale }
    };
    for test_values in [
        probe_values.clone(),
        BTreeMap::from([(106, 20), (110, 5)]),
        BTreeMap::from([(106, 30), (110, 10), (111, 5)]),
    ] {
        let actual =
            call_number_float_with_values(function, main_state_probe, test_values.clone())?;
        if !approx_float_eq(actual, expected(&test_values)) {
            return None;
        }
    }
    Some(expr)
}

fn infer_number_scalar_multiply(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 1 {
        return None;
    }
    let ref_id = refs[0];
    let baseline = call_number_float_with_values(function, main_state_probe, BTreeMap::new())?;
    let at_one =
        call_number_float_with_values(function, main_state_probe, BTreeMap::from([(ref_id, 1)]))?;
    let coefficient = at_one - baseline;
    if coefficient.abs() < f64::EPSILON {
        return None;
    }
    let at_three =
        call_number_float_with_values(function, main_state_probe, BTreeMap::from([(ref_id, 3)]))?;
    if !approx_float_eq(at_three - baseline, coefficient * 3.0) {
        return None;
    }
    Some(format!("{coefficient}*number({ref_id})"))
}

fn infer_option_weighted_number_sum(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let options = collect_option_calls(function, main_state_probe)?;
    if options.is_empty() || options.len() > 12 {
        return None;
    }

    let mut refs = Vec::new();
    for option_id in &options {
        refs.extend(collect_number_refs_with_option(function, main_state_probe, *option_id)?);
    }
    refs.sort_unstable();
    refs.dedup();
    if refs.is_empty() || refs.len() > 16 {
        return None;
    }

    let mut terms = Vec::new();
    for option_id in &options {
        let option_values = BTreeMap::from([(*option_id, true)]);
        let zero_values = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect();
        let baseline = call_number_float_with_values_and_options(
            function,
            main_state_probe,
            zero_values,
            option_values.clone(),
        )?;
        for ref_id in &refs {
            let mut values = refs.iter().copied().map(|id| (id, 0)).collect::<BTreeMap<_, _>>();
            values.insert(*ref_id, 1);
            let at_one = call_number_float_with_values_and_options(
                function,
                main_state_probe,
                values,
                option_values.clone(),
            )?;
            let coefficient = at_one - baseline;
            if coefficient.abs() > f64::EPSILON {
                terms.push(format!("{coefficient}*option({option_id})*number({ref_id})"));
            }
        }
    }
    if terms.is_empty() {
        return None;
    }

    for option_id in &options {
        let option_values = BTreeMap::from([(*option_id, true)]);
        for sample in [1, 3, 7] {
            let values = refs.iter().copied().map(|ref_id| (ref_id, sample)).collect();
            let actual = call_number_float_with_values_and_options(
                function,
                main_state_probe,
                values,
                option_values.clone(),
            )?;
            let expected = evaluate_option_weighted_number_terms(
                &terms,
                *option_id,
                &refs.iter().copied().map(|ref_id| (ref_id, sample)).collect(),
            )?;
            if !approx_float_eq(actual, expected) {
                return None;
            }
        }
    }

    Some(terms.join("+"))
}

fn evaluate_option_weighted_number_terms(
    terms: &[String],
    active_option: i32,
    values: &BTreeMap<i32, i32>,
) -> Option<f64> {
    let mut total = 0.0;
    for term in terms {
        let mut factors = term.split('*');
        let coefficient = factors.next()?.parse::<f64>().ok()?;
        let option = factors.next()?.trim();
        let number = factors.next()?.trim();
        if factors.next().is_some() {
            return None;
        }
        let option_id = option.strip_prefix("option(")?.strip_suffix(')')?.parse::<i32>().ok()?;
        let ref_id = number.strip_prefix("number(")?.strip_suffix(')')?.parse::<i32>().ok()?;
        if option_id == active_option {
            total += coefficient * f64::from(values.get(&ref_id).copied().unwrap_or(0));
        }
    }
    Some(total)
}

fn infer_weighted_number_ratio_scaled(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() < 2 || refs.len() > 16 {
        return None;
    }
    refs.iter().find_map(|denominator_ref| {
        infer_weighted_number_ratio_scaled_with_denominator(
            function,
            main_state_probe,
            &refs,
            *denominator_ref,
        )
    })
}

fn infer_weighted_number_ratio_scaled_with_denominator(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    denominator_ref: i32,
) -> Option<String> {
    const PROBE_DENOMINATOR: i32 = 1000;
    let mut base_values =
        refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    base_values.insert(denominator_ref, PROBE_DENOMINATOR);
    let baseline = call_number_float_with_values(function, main_state_probe, base_values.clone())?;
    if !approx_float_eq(baseline, 0.0) {
        return None;
    }

    let mut terms = Vec::new();
    for ref_id in refs.iter().copied().filter(|ref_id| *ref_id != denominator_ref) {
        let mut values = base_values.clone();
        values.insert(ref_id, 1);
        let at_one = call_number_float_with_values(function, main_state_probe, values)?;
        if at_one - baseline < 1.0 {
            continue;
        }
        let coefficient = ((at_one - baseline) * f64::from(PROBE_DENOMINATOR)).round() as i64;
        if coefficient <= 0 {
            continue;
        }
        terms.push((ref_id, coefficient));
    }
    if terms.is_empty() {
        return None;
    }

    let test_cases = [
        refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 1))
            .chain(std::iter::once((denominator_ref, PROBE_DENOMINATOR)))
            .collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 3))
            .chain(std::iter::once((denominator_ref, PROBE_DENOMINATOR)))
            .collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 1))
            .chain(std::iter::once((denominator_ref, 74)))
            .collect::<BTreeMap<_, _>>(),
    ];
    for values in test_cases {
        let expected = weighted_ratio_floor(&terms, denominator_ref, &values) as f64;
        let actual = match call_number_float_with_values(function, main_state_probe, values) {
            Some(value) if value.is_finite() => value,
            _ if expected.abs() < f64::EPSILON => 0.0,
            _ => return None,
        };
        if !approx_float_eq(actual, expected) {
            return None;
        }
    }

    let numerator = terms
        .iter()
        .map(|(ref_id, coefficient)| {
            if *coefficient == 1 {
                format!("number({ref_id})")
            } else {
                format!("{coefficient}*number({ref_id})")
            }
        })
        .collect::<Vec<_>>()
        .join("+");
    Some(format!("floor(({numerator})/number({denominator_ref}))"))
}

fn weighted_ratio_floor(
    terms: &[(i32, i64)],
    denominator_ref: i32,
    values: &BTreeMap<i32, i32>,
) -> i64 {
    let denominator = values.get(&denominator_ref).copied().unwrap_or(0);
    if denominator <= 0 {
        return 0;
    }
    let numerator = terms
        .iter()
        .map(|(ref_id, coefficient)| {
            coefficient.saturating_mul(i64::from(values.get(ref_id).copied().unwrap_or(0)))
        })
        .sum::<i64>();
    numerator / i64::from(denominator)
}

fn fast_slow_ref_set() -> BTreeMap<i32, ()> {
    FAST_SLOW_FAST_REFS.into_iter().chain(FAST_SLOW_SLOW_REFS).map(|ref_id| (ref_id, ())).collect()
}

fn infer_fast_slow_ratio_graph_type(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    let refs = collect_number_refs(function, main_state_probe)?;
    let expected = fast_slow_ref_set();
    if refs.len() != expected.len() || !refs.iter().all(|ref_id| expected.contains_key(ref_id)) {
        return None;
    }
    let fast_set: BTreeMap<i32, ()> =
        FAST_SLOW_FAST_REFS.into_iter().map(|ref_id| (ref_id, ())).collect();
    let slow_set: BTreeMap<i32, ()> =
        FAST_SLOW_SLOW_REFS.into_iter().map(|ref_id| (ref_id, ())).collect();
    if verify_fast_slow_ratio(function, main_state_probe, &refs, &fast_set) {
        return Some(148);
    }
    if verify_fast_slow_ratio(function, main_state_probe, &refs, &slow_set) {
        return Some(149);
    }
    None
}

fn approx_float_eq(actual: f64, expected: f64) -> bool {
    if expected.abs() < f64::EPSILON && (!actual.is_finite() || actual.abs() < f64::EPSILON) {
        return true;
    }
    (actual - expected).abs() <= 0.02
}

fn verify_fast_slow_ratio(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    numerator_refs: &BTreeMap<i32, ()>,
) -> bool {
    let ratio = |values: &BTreeMap<i32, i32>| {
        let num: f64 = numerator_refs
            .keys()
            .map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64)
            .sum();
        let den: f64 =
            refs.iter().map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64).sum();
        if den.abs() < f64::EPSILON { 0.0 } else { num / den }
    };
    let all_zero: BTreeMap<i32, i32> = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect();
    let all_one: BTreeMap<i32, i32> = refs.iter().copied().map(|ref_id| (ref_id, 1)).collect();
    let mut numerator_only = all_zero.clone();
    for ref_id in numerator_refs.keys() {
        numerator_only.insert(*ref_id, 5);
    }
    let mut complement_only =
        refs.iter().copied().map(|ref_id| (ref_id, 5)).collect::<BTreeMap<_, _>>();
    for ref_id in numerator_refs.keys() {
        complement_only.insert(*ref_id, 0);
    }
    let ratio_all_one = ratio(&all_one);
    let ratio_numerator_only = ratio(&numerator_only);
    let ratio_complement_only = ratio(&complement_only);
    for (values, expected) in [
        (all_zero, 0.0),
        (all_one, ratio_all_one),
        (numerator_only, ratio_numerator_only),
        (complement_only, ratio_complement_only),
    ] {
        let actual = match call_number_float_with_values(function, main_state_probe, values) {
            Some(value) if value.is_finite() => value,
            _ if expected.abs() < f64::EPSILON => 0.0,
            _ => return false,
        };
        if !approx_float_eq(actual, expected) {
            return false;
        }
    }
    true
}

fn infer_division_of_number_sums(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() < 2 || refs.len() > 24 {
        return None;
    }
    let zero_values = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    let baseline = call_number_float_with_values(function, main_state_probe, zero_values.clone())?;
    let mut numerator_refs = Vec::new();
    for ref_id in &refs {
        let mut values = zero_values.clone();
        values.insert(*ref_id, 5);
        let value = call_number_float_with_values(function, main_state_probe, values)?;
        if value > baseline + f64::EPSILON {
            numerator_refs.push(*ref_id);
        }
    }
    if numerator_refs.is_empty() {
        return None;
    }
    let numerator = format_number_sum_expr(&numerator_refs);
    let denominator = format_number_sum_expr(&refs);
    let expr = format!("({numerator})/({denominator})");
    let expected_ratio = |values: &BTreeMap<i32, i32>| {
        let num: f64 = numerator_refs
            .iter()
            .map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64)
            .sum();
        let den: f64 =
            refs.iter().map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64).sum();
        if den.abs() < f64::EPSILON { 0.0 } else { num / den }
    };
    let mut numerator_only = zero_values.clone();
    for ref_id in &numerator_refs {
        numerator_only.insert(*ref_id, 5);
    }
    let mut denominator_only =
        refs.iter().copied().map(|ref_id| (ref_id, 5)).collect::<BTreeMap<_, _>>();
    for ref_id in &numerator_refs {
        denominator_only.insert(*ref_id, 0);
    }
    let test_cases = [
        zero_values,
        refs.iter().copied().map(|id| (id, 1)).collect(),
        refs.iter().copied().map(|id| (id, 3)).collect(),
        numerator_only,
        denominator_only,
    ];
    for values in test_cases {
        let expected = expected_ratio(&values);
        let actual = call_number_float_with_values(function, main_state_probe, values)?;
        if !approx_float_eq(actual, expected) {
            return None;
        }
    }
    Some(expr)
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

/// beatoraja Lua skin loader が document/header に残すコールバック。
/// BMZ は `.luaskin` 実行結果だけを使い、関数参照自体は JSON 化しない。
const SILENTLY_SKIPPED_LOADER_FIELDS: &[&str] = &["process", "main", "processHeader", "act"];

fn should_silently_skip_loader_field(path: &str, key: &str, value: &Value) -> bool {
    matches!(value, Value::Function(_))
        && (SILENTLY_SKIPPED_LOADER_FIELDS.contains(&key)
            || (key == "timer" && path.contains(".customTimers[")))
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

    #[test]
    fn maps_peacefulplay_keylogger_graph_ids_to_builtin_expressions() {
        assert_eq!(
            keylogger_graph_value_expr_from_id("keylogger-graph-judge-3-good").as_deref(),
            Some("bmz:keylogger_graph:judge:3:good")
        );
        assert_eq!(
            keylogger_graph_value_expr_from_id("keylogger-graph-fastslow-9-fast").as_deref(),
            Some("bmz:keylogger_graph:fastslow:9:fast")
        );
        assert!(keylogger_graph_value_expr_from_id("graph-now").is_none());
    }

    #[test]
    fn infers_fixed_delay_timer_function() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        lua.globals()
            .set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap())
            .unwrap();
        let function = lua
            .load(
                r#"return function()
                    local off = main_state.timer_off_value
                    local source = main_state.timer(143)
                    if source == off then return off end
                    local start = source + 1000000
                    if main_state.time() < start then return off end
                    return start
                end"#,
            )
            .eval::<Function>()
            .unwrap();
        assert_eq!(infer_fixed_delay_timer(&function, &probe), Some((143, 1000)));
    }

    #[test]
    fn infers_event_index_or_draw_condition() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let function = lua
            .load(
                r#"
                return function()
                    return main_state.event_index(42) == 2 or main_state.event_index(42) == 3
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(
            infer_main_state_event_index_draw_condition(&function, &probe),
            Some("event_index(42) == 2 or event_index(42) == 3".to_string())
        );
    }

    #[test]
    fn infers_keybeam_hold_draw_condition() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let function = lua
            .load(
                r#"
                local off = main_state.timer_off_value
                local last_update_time = off
                local last_key_on_timer = {}
                local last_key_off_timer = {}
                local active = {}
                local fade_start_time = {}
                local suppress_until_key_off = {}
                local lanes = {
                    { display_lane = 1, key_on_timer = 101, key_off_timer = 121, hold_timer = 71 },
                    { display_lane = 2, key_on_timer = 102, key_off_timer = 122, hold_timer = 72 },
                }
                local function update()
                    local now = main_state.time()
                    if now == last_update_time then
                        return
                    end
                    last_update_time = now
                    for _, lane_info in ipairs(lanes) do
                        local lane = lane_info.display_lane
                        local key_on_time = main_state.timer(lane_info.key_on_timer)
                        local key_off_time = main_state.timer(lane_info.key_off_timer)
                        local hold_time = main_state.timer(lane_info.hold_timer)
                        local key_on_changed = key_on_time ~= off and key_on_time ~= last_key_on_timer[lane]
                        local key_off_changed = key_off_time ~= off and key_off_time ~= last_key_off_timer[lane]
                        if key_on_changed then
                            active[lane] = true
                            fade_start_time[lane] = nil
                            suppress_until_key_off[lane] = false
                        end
                        if hold_time ~= off and (active[lane] or key_off_changed) then
                            suppress_until_key_off[lane] = true
                            fade_start_time[lane] = nil
                        end
                        if key_off_changed then
                            active[lane] = true
                            fade_start_time[lane] = key_off_time
                        end
                        last_key_on_timer[lane] = key_on_time
                        last_key_off_timer[lane] = key_off_time
                    end
                end
                return function()
                    update()
                    if not active[1] then
                        return false
                    end
                    if suppress_until_key_off[1] then
                        return false
                    end
                    if fade_start_time[1] ~= nil and main_state.time() >= fade_start_time[1] then
                        return false
                    end
                    return main_state.event_index(501) == 2 or main_state.event_index(501) == 3
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(
            infer_boolean_predicate(&function, &probe, None),
            Some(
                "timer(101) != timer_off and timer(71) == timer_off and event_index(501) == 2 or timer(101) != timer_off and timer(71) == timer_off and event_index(501) == 3"
                    .to_string()
            )
        );
    }

    #[test]
    fn infers_end_of_note_shadow_draw_condition() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let function = lua
            .load(
                r#"
                local TIMER_OFF = main_state.timer_off_value
                local function getRemainNotes()
                    return main_state.number(106)
                        - main_state.number(110)
                        - main_state.number(111)
                        - main_state.number(112)
                        - main_state.number(113)
                        - main_state.number(114)
                end

                return function()
                    if main_state.timer(143) == TIMER_OFF and getRemainNotes() == 0 then
                        return true
                    end
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(
            infer_boolean_predicate(&function, &probe, None),
            Some(
                "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0"
                    .to_string()
            )
        );
    }

    #[test]
    fn repairs_keybeam_hold_destination_draws_from_fade_pairs() {
        let mut root = JsonMap::from_iter([(
            "destination".to_string(),
            JsonValue::Array(vec![
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-pgreat".to_string())),
                    ("draw".to_string(), JsonValue::String("number(0) < 0".to_string())),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-pgreat".to_string())),
                    ("timer".to_string(), JsonValue::Number(JsonNumber::from(122))),
                    ("loop".to_string(), JsonValue::Number(JsonNumber::from(-1))),
                    ("draw".to_string(), JsonValue::String("event_index(502) == 1".to_string())),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-great".to_string())),
                    (
                        "draw".to_string(),
                        JsonValue::String(
                            "event_index(503) == 2 or event_index(503) == 3".to_string(),
                        ),
                    ),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-great".to_string())),
                    ("timer".to_string(), JsonValue::Number(JsonNumber::from(123))),
                    ("loop".to_string(), JsonValue::Number(JsonNumber::from(-1))),
                    (
                        "draw".to_string(),
                        JsonValue::String(
                            "event_index(503) == 2 or event_index(503) == 3".to_string(),
                        ),
                    ),
                ])),
            ]),
        )]);

        postprocess_lua_skin_json(&mut root);

        let destinations = root.get("destination").and_then(JsonValue::as_array).unwrap();
        let draw = |index: usize| {
            destinations[index]
                .as_object()
                .and_then(|destination| destination.get("draw"))
                .and_then(JsonValue::as_str)
                .unwrap()
        };
        assert_eq!(draw(0), "keybeam_hold(102) != 0 and event_index(502) == 1");
        assert_eq!(
            draw(2),
            "keybeam_hold(103) != 0 and event_index(503) == 2 or keybeam_hold(103) != 0 and event_index(503) == 3"
        );
        assert_eq!(draw(1), "keybeam_fade(122) != 0 and event_index(502) == 1");
    }

    #[test]
    fn infers_keybeam_keyoff_timer_function() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let function = lua
            .load(
                r#"
                local off = main_state.timer_off_value
                local fade_us = 50000
                local last_update_time = off
                local last_key_on_timer = {}
                local last_key_off_timer = {}
                local active = {}
                local fade_start_time = {}
                local lanes = {
                    { display_lane = 1, key_on_timer = 101, key_off_timer = 121, hold_timer = 71 },
                    { display_lane = 2, key_on_timer = 102, key_off_timer = 122, hold_timer = 72 },
                }
                local function update()
                    local now = main_state.time()
                    if now == last_update_time then
                        return
                    end
                    last_update_time = now
                    for _, lane_info in ipairs(lanes) do
                        local lane = lane_info.display_lane
                        local key_on_time = main_state.timer(lane_info.key_on_timer)
                        local key_off_time = main_state.timer(lane_info.key_off_timer)
                        local key_off_changed = key_off_time ~= off and key_off_time ~= last_key_off_timer[lane]
                        if key_on_time ~= off and key_on_time ~= last_key_on_timer[lane] then
                            active[lane] = true
                            fade_start_time[lane] = nil
                        end
                        if key_off_changed then
                            active[lane] = true
                            fade_start_time[lane] = key_off_time
                        end
                        if fade_start_time[lane] and now >= fade_start_time[lane] + fade_us then
                            active[lane] = false
                        end
                        last_key_on_timer[lane] = key_on_time
                        last_key_off_timer[lane] = key_off_time
                    end
                end
                return function()
                    update()
                    local fade_start = fade_start_time[1]
                    if active[1] and fade_start and main_state.time() >= fade_start then
                        return fade_start
                    end
                    return off
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(infer_timer_function_ref(&function, &probe), Some(121));
    }

    #[test]
    fn infers_main_state_judge_as_beatoraja_number_ref() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let value = lua
            .load(
                r#"
                return function()
                    return main_state.judge(1) or 0
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();
        let draw = lua
            .load(
                r#"
                return function()
                    return (main_state.judge(2) or 0) > 0
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(infer_main_state_number_ref(&value, &probe), Some(111));
        assert_eq!(
            infer_boolean_predicate(&draw, &probe, None),
            Some("number(112) > 0".to_string())
        );
    }

    #[test]
    fn infers_weighted_pscore_value_expr_from_judge_counts() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
        lua.globals().set("main_state", main_state).unwrap();
        let function = lua
            .load(
                r#"
                local function clamp(value, min_value, max_value)
                    if value < min_value then
                        return min_value
                    end
                    if value > max_value then
                        return max_value
                    end
                    return value
                end

                return function()
                    local total_notes = main_state.number(74)
                    if not total_notes or total_notes <= 0 then
                        return 0
                    end

                    local cool = main_state.judge(0)
                    local great = main_state.judge(1)
                    local good = main_state.judge(2)
                    local raw = 100000 * ((cool * 1.0) + (great * 0.7) + (good * 0.4)) / total_notes
                    return clamp(math.floor(raw), 0, 100000)
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

        assert_eq!(
            infer_value_float_expr(&function, &probe),
            Some(
                "floor((100000*number(110)+70000*number(111)+40000*number(112))/number(74))"
                    .to_string()
            )
        );
    }

    #[test]
    fn infers_peaceful_play_gauge_value_builtins() {
        let lua = Lua::new();
        let probe = Arc::new(Mutex::new(MainStateProbe::default()));
        let function = lua.load("return function() return 0 end").eval::<Function>().unwrap();

        for (id, expected) in [
            ("val-gauge-percent-integer", SKIN_EXPR_GAUGE_PERCENT_INTEGER),
            ("val-gauge-percent-fraction", SKIN_EXPR_GAUGE_PERCENT_FRACTION),
            ("val-gauge-amount-integer", SKIN_EXPR_GAUGE_AMOUNT_INTEGER),
            ("val-gauge-amount-fraction", SKIN_EXPR_GAUGE_AMOUNT_FRACTION),
        ] {
            assert_eq!(
                infer_bmz_builtin_value_expr(&function, Some(id), &probe),
                Some(expected.to_string())
            );
        }
    }

    fn unique_skin_test_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bmz-lua-{tag}-{nanos}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn beatoraja_skin_alias_accepts_renamed_skin_root() {
        let root = unique_skin_test_dir("renamed-root").join("mz-select");
        fs::create_dir_all(root.join("customize/advanced")).unwrap();
        fs::write(root.join("customize/advanced/enable.txt"), "parts.lua\n").unwrap();

        let resolved =
            resolve_skin_io_path(&root, "skin/m_select/customize/advanced/enable.txt").unwrap();

        assert_eq!(
            resolved,
            canonicalize_skin_path(&root.join("customize/advanced/enable.txt")).unwrap()
        );
    }

    #[test]
    fn default_skin_file_uses_random_sentinel_for_random_def() {
        let root = unique_skin_test_dir("random-def");
        fs::create_dir_all(root.join("bg")).unwrap();
        fs::write(root.join("bg/one.mp4"), []).unwrap();
        fs::write(root.join("bg/two.mp4"), []).unwrap();
        let filepath: JsonValue =
            serde_json::from_str(r#"{ "name": "BG", "path": "bg/*.mp4", "def": "Random" }"#)
                .unwrap();

        assert_eq!(
            default_skin_file_from_filepath(&root, "bg/*.mp4", &filepath).as_deref(),
            Some(RANDOM_FILE_SELECTION)
        );
    }

    #[test]
    fn default_skin_file_returns_beatoraja_filename_selection() {
        let root = unique_skin_test_dir("filename-default");
        fs::create_dir_all(root.join("bg")).unwrap();
        fs::write(root.join("bg/one.mp4"), []).unwrap();
        fs::write(root.join("bg/two.mp4"), []).unwrap();
        let filepath: JsonValue =
            serde_json::from_str(r#"{ "name": "BG", "path": "bg/*.mp4", "def": "two" }"#).unwrap();

        assert_eq!(
            default_skin_file_from_filepath(&root, "bg/*.mp4", &filepath).as_deref(),
            Some("two.mp4")
        );
    }

    #[test]
    fn default_skin_file_prefers_default_stem_when_def_missing() {
        let root = unique_skin_test_dir("default-stem");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/pastel.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let filepath: JsonValue =
            serde_json::from_str(r#"{ "name": "Note", "path": "notes/*.png" }"#).unwrap();

        assert_eq!(
            default_skin_file_from_filepath(&root, "notes/*.png", &filepath).as_deref(),
            Some("default.png")
        );
    }

    #[test]
    fn property_default_matches_item_name_not_numeric_op_string() {
        let property: JsonValue = serde_json::from_str(
            r#"
            {
                "name": "Graph",
                "def": "923",
                "item": [
                    { "name": "AC", "op": 922 },
                    { "name": "TYPE-M", "op": 923 }
                ]
            }
            "#,
        )
        .unwrap();
        let items = property.get("item").and_then(JsonValue::as_array).unwrap();

        assert_eq!(default_property_op(&property, items), Some(922));
    }

    #[test]
    fn selected_numeric_option_must_exist_in_items() {
        let items: Vec<JsonValue> = serde_json::from_str(
            r#"
            [
                { "name": "AC", "op": 922 },
                { "name": "TYPE-M", "op": 923 }
            ]
            "#,
        )
        .unwrap();

        assert_eq!(option_value_to_op(&items, "923"), Some(923));
        assert_eq!(option_value_to_op(&items, "999"), None);
    }

    #[test]
    fn property_options_accept_integral_lua_numbers() {
        let property: JsonValue = serde_json::from_str(
            r#"
            {
                "name": "Key Beam Length",
                "def": "100%",
                "item": [
                    { "name": "100%", "op": 11400.0 },
                    { "name": "90%", "op": 11401.0 }
                ]
            }
            "#,
        )
        .unwrap();
        let header = serde_json::json!({ "property": [property] });
        let mut warnings = Vec::new();

        let options = skin_config_options_from_header(
            &header,
            &BTreeMap::from([("Key Beam Length".to_string(), "90%".to_string())]),
            &mut warnings,
        );

        assert_eq!(options.get("Key Beam Length"), Some(&11401));
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn property_options_reject_fractional_lua_numbers() {
        let items = vec![serde_json::json!({ "name": "invalid", "op": 11400.5 })];

        assert_eq!(option_value_to_op(&items, "invalid"), None);
    }

    #[test]
    fn get_path_accepts_beatoraja_filename_selection() {
        let root = unique_skin_test_dir("filename-getpath");
        fs::create_dir_all(root.join("bg")).unwrap();
        fs::write(root.join("bg/one.mp4"), []).unwrap();
        let skin_files = BTreeMap::from([("bg/*.mp4".to_string(), "one.mp4".to_string())]);

        let resolved = skin_config_get_path(&root, "bg/*.mp4", &skin_files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("one.mp4"));
    }

    #[test]
    fn get_path_randomizes_when_selection_is_random_sentinel() {
        let root = unique_skin_test_dir("random-getpath");
        fs::create_dir_all(root.join("bg")).unwrap();
        fs::write(root.join("bg/one.mp4"), []).unwrap();
        fs::write(root.join("bg/two.mp4"), []).unwrap();
        let skin_files =
            BTreeMap::from([("bg/*.mp4".to_string(), RANDOM_FILE_SELECTION.to_string())]);

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let resolved = skin_config_get_path(&root, "bg/*.mp4", &skin_files).unwrap();
            let name =
                resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            assert!(name == "one.mp4" || name == "two.mp4", "unexpected match {name}");
            seen.insert(name);
        }
        assert_eq!(seen.len(), 2, "Random selection should pick randomly among matches");
    }
}
