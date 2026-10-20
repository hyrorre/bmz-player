use super::*;

#[derive(Clone)]
pub(super) struct LuaInstructionBudget {
    total_remaining: Arc<AtomicI64>,
    callback_remaining: Arc<AtomicI64>,
}

impl LuaInstructionBudget {
    pub(super) fn begin_inference(&self) {
        self.total_remaining.store(LUA_INFERENCE_INSTRUCTION_LIMIT, Ordering::Relaxed);
        self.begin_callback();
    }

    pub(super) fn begin_callback(&self) {
        self.callback_remaining.store(LUA_INSTRUCTION_LIMIT, Ordering::Relaxed);
    }

    pub(super) fn begin_runtime_callback(&self, new_frame: bool) {
        // A callback has its own ceiling, while all callbacks evaluated for the
        // same frame share an aggregate budget so a skin cannot multiply the
        // per-call allowance by creating hundreds of dynamic properties.
        if new_frame {
            self.total_remaining.store(LUA_RUNTIME_FRAME_INSTRUCTION_LIMIT, Ordering::Relaxed);
        }
        self.begin_callback();
    }
}

pub(super) fn install_instruction_limit(lua: &Lua) -> LuaInstructionBudget {
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
) -> LuaInstructionBudget {
    let budget = LuaInstructionBudget {
        total_remaining: Arc::new(AtomicI64::new(total_limit)),
        callback_remaining: Arc::new(AtomicI64::new(callback_limit)),
    };
    let total_remaining = budget.total_remaining.clone();
    let callback_remaining = budget.callback_remaining.clone();
    lua.set_hook(HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL), move |_, _| {
        let interval = i64::from(LUA_HOOK_INTERVAL);
        if total_remaining.fetch_sub(interval, Ordering::Relaxed) <= interval
            || callback_remaining.fetch_sub(interval, Ordering::Relaxed) <= interval
        {
            Err(mlua::Error::runtime("lua skin instruction limit exceeded"))
        } else {
            Ok(VmState::Continue)
        }
    });
    budget
}

#[derive(Debug)]
pub(super) struct TableBudget {
    remaining_entries: usize,
}

impl Default for TableBudget {
    fn default() -> Self {
        Self { remaining_entries: LUA_MAX_TABLE_ENTRIES }
    }
}

impl TableBudget {
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
