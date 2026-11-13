use super::*;

// Work units count native matching steps/bytes, not Lua VM instructions.
// Keep the instruction ceilings unchanged; native loops are much cheaper.
const PATTERN_WORK_PER_INSTRUCTION: i64 = 4;

unsafe extern "C-unwind" {
    fn bmz_open_patterns(state: *mut mlua::lua_State) -> c_int;
}

#[cfg(test)]
#[path = "pattern_budget_tests.rs"]
mod pattern_budget_tests;

#[derive(Clone)]
pub(super) struct LuaInstructionBudget {
    total_remaining: Arc<AtomicI64>,
    callback_remaining: Arc<AtomicI64>,
    pattern_total_remaining: Arc<AtomicI64>,
    pattern_callback_remaining: Arc<AtomicI64>,
}

impl LuaInstructionBudget {
    fn check(&self) -> mlua::Result<()> {
        if self.total_remaining.load(Ordering::Relaxed) <= 0
            || self.callback_remaining.load(Ordering::Relaxed) <= 0
        {
            Err(mlua::Error::runtime("lua skin instruction limit exceeded"))
        } else if self.pattern_total_remaining.load(Ordering::Relaxed) <= 0
            || self.pattern_callback_remaining.load(Ordering::Relaxed) <= 0
        {
            Err(mlua::Error::runtime("lua skin pattern work limit exceeded"))
        } else {
            Ok(())
        }
    }

    fn hook(&self) -> impl Fn(&Lua, mlua::Debug) -> mlua::Result<VmState> + Send + 'static {
        let budget = self.clone();
        move |_, _| {
            budget.charge_hook_interval()?;
            Ok(VmState::Continue)
        }
    }

    fn charge_hook_interval(&self) -> mlua::Result<()> {
        let interval = i64::from(LUA_HOOK_INTERVAL);
        self.total_remaining.fetch_sub(interval, Ordering::Relaxed);
        self.callback_remaining.fetch_sub(interval, Ordering::Relaxed);
        self.check()
    }

    pub(super) fn begin_inference(&self) {
        self.total_remaining.store(LUA_INFERENCE_INSTRUCTION_LIMIT, Ordering::Relaxed);
        self.pattern_total_remaining.store(
            LUA_INFERENCE_INSTRUCTION_LIMIT * PATTERN_WORK_PER_INSTRUCTION,
            Ordering::Relaxed,
        );
        self.begin_callback();
    }

    pub(super) fn begin_callback(&self) {
        self.callback_remaining.store(LUA_INSTRUCTION_LIMIT, Ordering::Relaxed);
        self.pattern_callback_remaining
            .store(LUA_INSTRUCTION_LIMIT * PATTERN_WORK_PER_INSTRUCTION, Ordering::Relaxed);
    }

    pub(super) fn begin_runtime_callback(&self, new_frame: bool) {
        // A callback has its own ceiling, while all callbacks evaluated for the
        // same frame share an aggregate budget so a skin cannot multiply the
        // per-call allowance by creating hundreds of dynamic properties.
        if new_frame {
            self.total_remaining.store(LUA_RUNTIME_FRAME_INSTRUCTION_LIMIT, Ordering::Relaxed);
            self.pattern_total_remaining.store(
                LUA_RUNTIME_FRAME_INSTRUCTION_LIMIT * PATTERN_WORK_PER_INSTRUCTION,
                Ordering::Relaxed,
            );
        }
        self.begin_callback();
    }
}

pub(super) fn install_instruction_limit(lua: &Lua) -> mlua::Result<LuaInstructionBudget> {
    install_instruction_limit_with_ceilings(
        lua,
        LUA_LOAD_INSTRUCTION_LIMIT,
        LUA_LOAD_INSTRUCTION_LIMIT,
    )
}

pub(super) fn install_instruction_limit_with_ceilings(
    lua: &Lua,
    total_limit: i64,
    callback_limit: i64,
) -> mlua::Result<LuaInstructionBudget> {
    let budget = LuaInstructionBudget {
        total_remaining: Arc::new(AtomicI64::new(total_limit)),
        callback_remaining: Arc::new(AtomicI64::new(callback_limit)),
        pattern_total_remaining: Arc::new(AtomicI64::new(
            total_limit.saturating_mul(PATTERN_WORK_PER_INSTRUCTION),
        )),
        pattern_callback_remaining: Arc::new(AtomicI64::new(
            callback_limit.saturating_mul(PATTERN_WORK_PER_INSTRUCTION),
        )),
    };
    lua.set_hook(HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL), budget.hook());

    // Meter the native matcher itself. A Lua implementation magnifies ordinary
    // CSV processing into hundreds of millions of VM instructions. Prepaid work
    // blocks also charge calls that fail before returning to Rust normally.
    let guard = budget.clone();
    let charge = lua.create_function(move |_, units: i64| {
        debug_assert!(units > 0);
        guard.pattern_total_remaining.fetch_sub(units, Ordering::Relaxed);
        guard.pattern_callback_remaining.fetch_sub(units, Ordering::Relaxed);
        guard.check()
    })?;
    // SAFETY: compiled against the exact mlua-sys Lua headers. The module uses
    // the public C API, owns no Rust objects and is called through mlua's
    // protected boundary. Its checker is a normal managed mlua callback.
    let open = unsafe { lua.create_c_function(bmz_open_patterns)? };
    let patterns: Table = open.call(charge)?;
    let strings: Table = lua.globals().get("string")?;
    for name in ["find", "match", "gmatch", "gsub"] {
        strings.set(name, patterns.get::<Function>(name)?)?;
    }

    // Lua runs __gc finalizers with hooks disabled, including at VM teardown.
    // Reject every non-nil __gc value before Lua marks a table for finalization;
    // even an initial false value could later be replaced with a function.
    let setmetatable: Function = lua.globals().get("setmetatable")?;
    let setmetatable = lua.create_registry_value(setmetatable)?;
    lua.globals().set(
        "setmetatable",
        lua.create_function(move |lua, args: mlua::MultiValue| {
            if let Some(Value::Table(metatable)) = args.get(1)
                && !matches!(metatable.raw_get::<Value>("__gc")?, Value::Nil)
            {
                return Err(mlua::Error::runtime("lua skin finalizers are disabled"));
            }
            lua.registry_value::<Function>(&setmetatable)?.call::<mlua::MultiValue>(args)
        })?,
    )?;

    // A hook error is otherwise an ordinary Lua error: a loop can catch it
    // forever. Keep the original protected calls private, and propagate an
    // exhausted budget through every protected-call boundary, including nested
    // pcall/xpcall and an xpcall error handler.
    // Lua disables hooks while executing a native xpcall message handler.
    // Run the handler only after pcall has unwound, with the same budget and
    // normal hook delivery. Lua wrappers also preserve yielding through
    // protected calls. debug/traceback is not exposed by the sandbox.
    let guard = budget.clone();
    let check = lua.create_function(move |_, ()| guard.check())?;
    let (pcall, xpcall): (Function, Function) = lua.load(r#"
        local check, original, pack, unpack, kind, raise = ...
        local function protected(...)
            check()
            local result = pack(original(...))
            check()
            return unpack(result, 1, result.n)
        end
        local function handled(f, handler, ...)
            check()
            if kind(handler) ~= 'function' then raise('xpcall error handler must be a function', 2) end
            local result = pack(protected(f, ...))
            if result[1] then return unpack(result, 1, result.n) end
            local ok, value = protected(handler, result[2])
            if not ok then value = 'error in error handling' end
            return false, value
        end
        return protected, handled
    "#).set_mode(ChunkMode::Text).call((
        check,
        lua.globals().get::<Function>("pcall")?,
        lua.globals().get::<Table>("table")?.get::<Function>("pack")?,
        lua.globals().get::<Table>("table")?.get::<Function>("unpack")?,
        lua.globals().get::<Function>("type")?,
        lua.globals().get::<Function>("error")?,
    ))?;
    lua.globals().set("pcall", pcall)?;
    lua.globals().set("xpcall", xpcall)?;

    // mlua 0.10's hook belongs to one thread. Lua-created coroutines inherit
    // the native hook, but mlua removes it when the thread does not match.
    // Switch the hook for each resume/close and restore the caller even on an
    // error or yield. All threads share the same counters.
    if let Ok(coroutine) = lua.globals().get::<Table>("coroutine") {
        for name in ["resume", "close"] {
            let original: Function = coroutine.get(name)?;
            let original = lua.create_registry_value(original)?;
            let guard = budget.clone();
            coroutine.set(
                name,
                lua.create_function(move |lua, args: mlua::MultiValue| {
                    guard.check()?;
                    let Some(Value::Thread(thread)) = args.front() else {
                        return Err(mlua::Error::runtime("coroutine thread expected"));
                    };
                    // Switching hooks resets Lua's interval counter. Charge a
                    // minimum interval so short resume/yield loops cannot keep
                    // resetting it without ever consuming the shared budget.
                    guard.charge_hook_interval()?;
                    thread.set_hook(
                        HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
                        guard.hook(),
                    );
                    let original: Function = lua.registry_value(&original)?;
                    let result = original.call::<mlua::MultiValue>(args);
                    lua.set_hook(
                        HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
                        guard.hook(),
                    );
                    guard.check()?;
                    result
                })?,
            )?;
        }
        // The stock wrap resumes internally, bypassing coroutine.resume.
        let wrap: Function = lua
            .load(
                r#"
            local create, resume, pack, unpack, raise = ...
            return function(f)
                local co = create(f)
                return function(...)
                    local result = pack(resume(co, ...))
                    if not result[1] then raise(result[2], 2) end
                    return unpack(result, 2, result.n)
                end
            end
        "#,
            )
            .set_mode(ChunkMode::Text)
            .call((
                coroutine.get::<Function>("create")?,
                coroutine.get::<Function>("resume")?,
                lua.globals().get::<Table>("table")?.get::<Function>("pack")?,
                lua.globals().get::<Table>("table")?.get::<Function>("unpack")?,
                lua.globals().get::<Function>("error")?,
            ))?;
        coroutine.set("wrap", wrap)?;
    }
    Ok(budget)
}

#[derive(Debug)]
pub(super) struct TableBudget {
    remaining_entries: usize,
    remaining_bytes: usize,
}

impl Default for TableBudget {
    fn default() -> Self {
        Self { remaining_entries: LUA_MAX_TABLE_ENTRIES, remaining_bytes: 64 * 1024 * 1024 }
    }
}

impl TableBudget {
    pub(super) fn consume_string_bytes(&mut self, bytes: &[u8], path: &str) -> Result<()> {
        // Lossy UTF-8 can expand each input byte into a three-byte replacement.
        let count = if std::str::from_utf8(bytes).is_ok() {
            bytes.len()
        } else {
            bytes.len().saturating_mul(3)
        };
        if count > self.remaining_bytes {
            bail!("lua converted string byte limit exceeded at {path}");
        }
        self.remaining_bytes -= count;
        Ok(())
    }

    pub(super) fn consume(&mut self, count: usize, path: &str) -> Result<()> {
        if count > self.remaining_entries {
            bail!("lua table entry limit exceeded at {path}");
        }
        self.remaining_entries -= count;
        Ok(())
    }
}

pub(super) fn create_skin_config_option_table(
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

/// beatoraja `skin_config.enabled_options`: the selected op of every header
/// property, 1-based in header order.
///
/// Like `skin_config.option`, the values are exposed through a read-only proxy
/// so that only the ops a skin actually reads become document dependencies.
/// Integer indexing, `#`, `ipairs`, `pairs`, `table.concat` and `table.unpack`
/// use the metamethods; raw access sees an empty table.
pub(super) fn install_skin_config_enabled_options(
    lua: &Lua,
    enabled_options: &[i64],
    load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> Result<()> {
    let Ok(skin_config) = lua.globals().get::<Table>("skin_config") else {
        return Ok(());
    };
    let table = lua.create_table()?;
    let values = Arc::<[i64]>::from(enabled_options);
    let values_for_index = values.clone();
    let dependencies_for_index = load_dependencies.clone();
    let index = lua.create_function(move |_, (_table, key): (Table, Value)| {
        let index = match key {
            Value::Integer(index) => index,
            Value::Number(index) if index.fract() == 0.0 => index as i64,
            _ => return Ok(Value::Nil),
        };
        let Some(value) = usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| values_for_index.get(index))
        else {
            return Ok(Value::Nil);
        };
        if let Ok(option_id) = i32::try_from(*value) {
            record_load_dependency_option(dependencies_for_index.as_ref(), option_id, true);
        }
        Ok(Value::Integer(*value))
    })?;
    let length = values.len();
    let len = lua.create_function(move |_, _: Table| Ok(length))?;
    let values_for_pairs = values;
    let pairs = lua.create_function(move |lua, _: Table| {
        let pairs_table = lua.create_table_with_capacity(values_for_pairs.len(), 0)?;
        for value in values_for_pairs.iter() {
            pairs_table.raw_push(*value)?;
            if let Ok(option_id) = i32::try_from(*value) {
                record_load_dependency_option(load_dependencies.as_ref(), option_id, true);
            }
        }
        let next = lua.globals().get::<Function>("next")?;
        Ok((next, pairs_table, Value::Nil))
    })?;
    let metatable = lua.create_table()?;
    metatable.set("__index", index)?;
    metatable.set("__len", len)?;
    metatable.set("__pairs", pairs)?;
    table.set_metatable(Some(metatable));
    skin_config.set("enabled_options", table)?;
    Ok(())
}

pub(super) fn record_load_dependency_option(
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

pub(super) fn record_skin_config_file_dependency(
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

pub(super) fn skin_config_file_dependency_name(
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

pub(super) fn record_lua_loaded_file_dependency(
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

pub(super) fn record_static_skin_config_option_dependencies(
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
