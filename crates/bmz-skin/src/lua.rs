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

use bmz_render::skin::{
    SKIN_DYNAMIC_TIMER_BASE, SKIN_EXPR_ADJUSTED_COVER, SKIN_EXPR_ADJUSTED_RATE,
    SKIN_EXPR_ADJUSTED_RATE_ADOT, SKIN_EXPR_COURSE_TABLE_TEXT,
    SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT, SKIN_EXPR_FS_THRESHOLD, SKIN_REF_PLAY_GAUGE_TYPE,
};

use crate::{LoadedLuaSkinValue, SkinLoadWarning};

const LUA_INSTRUCTION_LIMIT: i64 = 2_000_000;
const LUA_HOOK_INTERVAL: u32 = 1_000;
const LUA_MAX_TABLE_DEPTH: usize = 64;
const LUA_MAX_TABLE_ENTRIES: usize = 200_000;
const TIMER_OFF_VALUE: i32 = i32::MIN;

/// beatoraja fast/slow 判定カウント ref (graph 比率推論用)
const FAST_SLOW_FAST_REFS: [i32; 6] = [410, 412, 414, 416, 418, 421];
const FAST_SLOW_SLOW_REFS: [i32; 6] = [411, 413, 415, 417, 419, 422];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertReport {
    pub warnings: Vec<String>,
}

pub fn load_lua_skin_value(
    input: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    let (value, warnings) = execute_lua_skin(input, options, files)?;
    Ok(LoadedLuaSkinValue {
        value,
        warnings: warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
    })
}

pub fn load_lua_skin_header_value(input: &Path) -> Result<LoadedLuaSkinValue> {
    let (value, warnings) = execute_lua_skin_header(input)?;
    Ok(LoadedLuaSkinValue {
        value,
        warnings: warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
    })
}

pub fn convert_lua_skin_to_json(
    input: &Path,
    output: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<ConvertReport> {
    let (json, warnings) = execute_lua_skin(input, options, files)?;
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
    let probe =
        install_sandbox(&lua, &root, &BTreeMap::new(), None, &BTreeMap::new(), &BTreeMap::new())?;
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
) -> Result<(JsonValue, Vec<String>)> {
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
    let header_probe =
        install_sandbox(&header_lua, &root, options, None, &BTreeMap::new(), &BTreeMap::new())?;
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
    let main_state_probe =
        install_sandbox(&lua, &root, options, Some(&skin_options), &skin_files, &skin_offsets)?;
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

    if let JsonValue::Object(ref mut root) = json {
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
    }

    Ok((json, warnings))
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

fn install_sandbox(
    lua: &Lua,
    root: &Path,
    options: &BTreeMap<String, String>,
    skin_config_options: Option<&BTreeMap<String, i64>>,
    skin_files: &BTreeMap<String, String>,
    skin_offsets: &BTreeMap<String, LuaSkinOffsetValue>,
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
        let get_path = lua.create_function(move |_, requested: String| {
            skin_config_get_path(&root_for_get_path, &requested, &skin_files_for_get_path)
                .map(|path| path.to_string_lossy().to_string())
                .map_err(mlua::Error::external)
        })?;
        skin_config.set("get_path", get_path)?;
        globals.set("skin_config", skin_config)?;
    }
    globals.set("os", create_os_stub(lua)?)?;
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
    gauge_type_calls: usize,
    gauge_type_value: i32,
    float_number_calls: Vec<i32>,
    float_number_values: BTreeMap<i32, f64>,
    text_calls: Vec<i32>,
    next_dynamic_timer_id: i32,
    dynamic_timers: Vec<(i32, String)>,
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
            gauge_type_calls: 0,
            gauge_type_value: 0,
            float_number_calls: Vec::new(),
            float_number_values: BTreeMap::new(),
            text_calls: Vec::new(),
            next_dynamic_timer_id: SKIN_DYNAMIC_TIMER_BASE,
            dynamic_timers: Vec::new(),
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
    }

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
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    fn begin_number_call_recording_with_option(&mut self, default_value: i32, option_id: i32) {
        self.begin_number_call_recording(default_value);
        self.option_values.insert(option_id, true);
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

    fn begin_number_recording_with_values_and_options(
        &mut self,
        values: BTreeMap<i32, i32>,
        options: BTreeMap<i32, bool>,
    ) {
        self.begin_number_recording_with_values(values);
        self.option_values = options;
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
            MainStateProbeMode::RuntimeStub => lua_runtime_stub_number(ref_id),
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
            return String::new();
        }
        self.text_calls.push(ref_id);
        format!("Text{ref_id}")
    }

    fn event_index(&mut self, _event_id: i32) -> i32 {
        0
    }

    fn begin_draw_probe(&mut self, numbers: BTreeMap<i32, i32>, floats: BTreeMap<i32, f64>) {
        self.begin_number_recording_with_values(numbers);
        self.float_number_values = floats;
    }
}

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
        lua.create_function(move |_, _event_id: i32| {
            Ok(probe_for_event_index
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .event_index(0))
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

fn create_os_stub(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set(
        "clock",
        lua.create_function(|_, ()| {
            static ORIGIN: OnceLock<Instant> = OnceLock::new();
            let origin = ORIGIN.get_or_init(Instant::now);
            Ok(origin.elapsed().as_secs_f64())
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

fn default_skin_file_from_filepath(
    root: &Path,
    normalized_path: &str,
    filepath: &JsonValue,
) -> Option<String> {
    let candidates = skin_file_candidates(root, normalized_path);
    if candidates.is_empty() {
        return None;
    }
    if let Some(default_name) = filepath.get("def").and_then(JsonValue::as_str)
        && !default_name.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, default_name))
    {
        return Some(candidate.clone());
    }
    candidates.into_iter().next()
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
    Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(default_name))
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

    // ユーザがスキン設定パネルで選んだファイルを最優先で返す。
    // 選択が存在しない / ファイルが消えている場合は従来通り候補解決へ委ねる。
    if let Some(selected) = skin_files.get(&requested.replace('\\', "/"))
        && let Some(path) = resolve_selected_skin_path(root, selected)
    {
        return Ok(path);
    }
    if let Some(path) =
        resolve_selected_skin_path_for_wildcard_child(root, requested_path, skin_files)
    {
        return Ok(path);
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
    candidates.sort();
    candidates.into_iter().next().ok_or_else(|| anyhow!("skin_config path not found: {requested}"))
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
        let wildcard = selected
            .strip_prefix(configured_prefix)
            .and_then(|rest| rest.strip_suffix(configured_suffix))?;
        let candidate = format!("{requested_prefix}{wildcard}{requested_suffix}");
        if let Some(path) = resolve_selected_skin_path(root, &candidate) {
            return Some(path);
        }
    }
    None
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
    for ancestor in root.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) != Some(skin_name) {
            continue;
        }
        let path = ancestor.join(skin_relative);
        if !path.is_file() {
            continue;
        }
        let Ok(canonical) = canonicalize_skin_path(&path) else {
            continue;
        };
        if canonical.starts_with(ancestor) {
            return Some(canonical);
        }
    }
    None
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
    let mut object = JsonMap::new();
    for (key, value) in entries {
        let key = lua_key_to_json_key(key, path, warnings)?;
        if matches!(value, Value::Nil) {
            continue;
        }
        if let Value::Function(function) = &value {
            if key == "value" {
                let is_graph = path.contains(".graph[");
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
                } else if let Some(value_expr) = infer_value_float_expr(function, main_state_probe)
                {
                    object.insert("value_expr".to_string(), JsonValue::String(value_expr));
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
                let map: Table = lua.globals().get("bmz_timer_fn_map")?;
                if let Ok(timer_id) = map.get::<i32>(function.clone()) {
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
        .or_else(|| infer_main_state_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_gauge_type_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_timer_option_draw_condition(function, main_state_probe))
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
    let mut calls = Vec::new();
    for default_value in [5, 0, -1] {
        {
            main_state_probe
                .lock()
                .ok()?
                .begin_number_call_recording_with_option(default_value, option_id);
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
        .or_else(|| infer_division_of_number_sums(function, main_state_probe))
}

const REMAIN_NOTE_REFS: [i32; 6] = [106, 110, 111, 112, 113, 114];

fn remain_notes_numerator_expr() -> String {
    "number(106)-number(110)-number(111)-number(112)-number(113)-number(114)".to_string()
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
