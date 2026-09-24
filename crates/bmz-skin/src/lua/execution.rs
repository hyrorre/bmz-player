pub(super) struct ExecutedLuaSkin {
    pub(super) value: JsonValue,
    pub(super) warnings: Vec<String>,
    pub(super) files: BTreeMap<String, String>,
    pub(super) dependencies: SkinLoadDependencies,
    pub(super) lua_runtime: Option<LuaSkinRuntime>,
    pub(super) runtime_callbacks: Vec<LuaRuntimeCallbackSpec>,
}

const LUA_PRACTICE_OPTION_ID: i32 = 1080;
const LUA_PRACTICE_STATE_FIELD: &str = "isPractice";

/// Synchronizes scene-stable beatoraja state with boolean fields exported by
/// loaded Lua modules before their callbacks are compiled or registered.
///
/// Some skins keep Practice state in a shared module table instead of reading
/// `main_state.option(1080)` in every draw callback.  BMZ exposes its own Lua
/// API during load, so compatibility branches may intentionally leave that
/// module field untouched.  Updating an existing boolean field keeps the
/// convention usable without depending on a skin name, object ID, or inferred
/// predicate shape.  Missing/non-boolean fields are never created or replaced.
fn apply_lua_scene_boolean_state(
    lua: &Lua,
    runtime_state: &LuaLoadRuntimeState,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
) -> Result<()> {
    let Some(practice) = runtime_state.option_values.get(&LUA_PRACTICE_OPTION_ID).copied() else {
        return Ok(());
    };
    let package: Table = lua.globals().get("package")?;
    let loaded: Table = package.get("loaded")?;
    let mut applied = false;
    for entry in loaded.pairs::<Value, Value>() {
        let (_, value) = entry?;
        let Value::Table(module) = value else {
            continue;
        };
        if matches!(module.raw_get::<Value>(LUA_PRACTICE_STATE_FIELD)?, Value::Boolean(_)) {
            module.raw_set(LUA_PRACTICE_STATE_FIELD, practice)?;
            applied = true;
        }
    }
    if applied {
        record_load_dependency_option(dependencies, LUA_PRACTICE_OPTION_ID, practice);
    }
    Ok(())
}

pub(super) fn execute_lua_skin(
    path_context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<ExecutedLuaSkin> {
    let input = path_context.entry_file().to_path_buf();

    let mut warnings = Vec::new();
    let mut table_budget = TableBudget::default();
    let source = fs::read_to_string(&input)
        .with_context(|| format!("failed to read lua skin: {}", input.display()))?;

    let header_lua = Lua::new();
    let header_instruction_budget = install_instruction_limit(&header_lua);
    // The header pass intentionally uses neutral main_state values, but it must
    // see the same read-only virtual filesystem as the document pass. Some
    // skins read compatibility configuration while their required modules are
    // initialized, before deciding whether to return a header or a document.
    let header_probe = install_sandbox(
        &header_lua,
        path_context,
        options,
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        virtual_io_files,
        None,
    )?;
    let header = header_lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    header_instruction_budget.begin_inference();
    let header_json = lua_value_to_json(
        &header_lua,
        header,
        "$",
        0,
        &mut warnings,
        &header_probe,
        &header_instruction_budget,
        &mut table_budget,
    )?;
    let skin_options = skin_config_options_from_header(&header_json, options, &mut warnings);
    let skin_files = skin_files_from_header(path_context, &header_json, files);
    let skin_named_files = skin_named_files_from_header(path_context, &header_json, files);
    let skin_offsets = skin_config_offsets_from_header(&header_json, runtime_state);
    let mut resolved_runtime_state = runtime_state.clone();
    resolved_runtime_state.offset_id_values.clear();
    for (name, id) in skin_offset_definitions_from_header(&header_json) {
        resolved_runtime_state
            .offset_id_values
            .insert(id, lua_skin_offset_value(runtime_state, &name, Some(id)));
    }
    // ヘッダ pass では skin_config / 全 option が未注入のため draw/value 推論が失敗しうる。
    // 本 pass の警告だけ残す。
    warnings.retain(|warning| {
        !warning.starts_with("skipping unsupported draw function at ")
            && !warning.starts_with("skipping unsupported value function at ")
            && !warning.starts_with("skipping unsupported custom timer function ")
            && !warning.starts_with("mixed lua table converted to object at ")
    });

    let lua = Lua::new();
    let instruction_budget = install_instruction_limit(&lua);
    let dependencies = Arc::new(Mutex::new(SkinLoadDependencies::default()));
    let main_state_probe = install_sandbox(
        &lua,
        path_context,
        options,
        Some(&skin_options),
        &skin_files,
        &skin_file_dependency_names_from_header(&header_json),
        &skin_offsets,
        &resolved_runtime_state,
        virtual_io_files,
        Some(dependencies.clone()),
    )?;
    let value = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin: {}", input.display()))?;
    apply_lua_scene_boolean_state(&lua, &resolved_runtime_state, Some(&dependencies))?;
    let scene_audio_actions = {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        let actions = probe.take_audio_actions();
        probe.capture_audio_actions = false;
        probe.inferring = true;
        actions
    };
    instruction_budget.begin_inference();
    let mut json = lua_value_to_json(
        &lua,
        value,
        "$",
        0,
        &mut warnings,
        &main_state_probe,
        &instruction_budget,
        &mut table_budget,
    )?;
    let result_panel_default = lua
        .globals()
        .get::<Value>("Expand_op")
        .ok()
        .and_then(lua_result_panel_value)
        .or_else(|| main_state_probe.lock().ok().and_then(|probe| probe.result_panel_default));
    record_static_skin_config_option_dependencies(&source, &skin_options, &dependencies);

    if let JsonValue::Object(ref mut root) = json {
        postprocess_lua_skin_json(root, &mut warnings);

        if let Some(panel) = result_panel_default {
            root.insert(
                "resultPanelDefault".to_string(),
                JsonValue::Number(JsonNumber::from(panel)),
            );
        }

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
        let runtime_flags = main_state_probe
            .lock()
            .ok()
            .map(|probe| probe.runtime_flags.clone())
            .unwrap_or_default();
        if !runtime_flags.is_empty() {
            let entries = runtime_flags.into_iter().map(|flag| {
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::Number(JsonNumber::from(flag.id))),
                    ("initial".to_string(), JsonValue::Bool(flag.initial)),
                ]))
            });
            root.insert("runtimeFlag".to_string(), JsonValue::Array(entries.collect()));
        }
        let runtime_events = main_state_probe
            .lock()
            .ok()
            .map(|probe| probe.runtime_events.clone())
            .unwrap_or_default();
        if !runtime_events.is_empty() {
            let entries = runtime_events.into_iter().map(|(id, toggle_flags)| {
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::Number(JsonNumber::from(id))),
                    (
                        "toggleFlags".to_string(),
                        JsonValue::Array(
                            toggle_flags
                                .into_iter()
                                .map(|flag_id| JsonValue::Number(JsonNumber::from(flag_id)))
                                .collect(),
                        ),
                    ),
                ]))
            });
            root.insert("runtimeEvent".to_string(), JsonValue::Array(entries.collect()));
        }
        if !scene_audio_actions.is_empty() {
            root.insert(
                "sceneAudio".to_string(),
                JsonValue::Array(
                    scene_audio_actions.into_iter().map(lua_audio_action_to_json).collect(),
                ),
            );
        }
        normalize_lua_skin_audio_paths(path_context, root, &mut warnings);
    }

    let unsupported_dynamic_timers = main_state_probe
        .lock()
        .ok()
        .map(|probe| probe.unsupported_dynamic_timers.clone())
        .unwrap_or_default();
    warnings.extend(unsupported_dynamic_timers.into_iter().map(|id| {
        format!(
            "timer_util.timer_observe_boolean callback for generated timer {id} could not be inferred; timer remains inactive"
        )
    }));
    let load_time_constant_dynamic_timers = main_state_probe
        .lock()
        .ok()
        .map(|probe| probe.load_time_constant_dynamic_timers.clone())
        .unwrap_or_default();
    warnings.extend(load_time_constant_dynamic_timers.into_iter().map(|id| {
        format!(
            "timer_util.timer_observe_boolean callback for generated timer {id} was fixed to its load-time value; runtime Lua state changes are unsupported"
        )
    }));

    let runtime_callbacks = main_state_probe
        .lock()
        .map_err(|_| anyhow!("main_state probe lock poisoned"))?
        .runtime_callbacks
        .clone();
    let lua_runtime = if runtime_callbacks.is_empty() {
        None
    } else {
        match build_lua_skin_runtime(LuaSkinRuntimeRequest {
            input: &input,
            path_context,
            source: &source,
            options,
            skin_config_options: &skin_options,
            skin_files: &skin_files,
            skin_file_dependency_names: &skin_file_dependency_names_from_header(&header_json),
            skin_offsets: &skin_offsets,
            runtime_state,
            virtual_io_files,
            runtime_callbacks: &runtime_callbacks,
        }) {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                warnings.push(format!(
                    "ERROR: failed to build Lua callback runtime for {}: {error:#}; callbacks use safe fallback values",
                    input.display()
                ));
                None
            }
        }
    };
    let mut dependencies =
        dependencies.lock().map_err(|_| anyhow!("lua dependency tracker lock poisoned"))?.clone();
    // A cached document cannot safely clone its sidecar VM. Keeping runtime
    // callback documents out of the document cache preserves registry/VM IDs.
    dependencies.opaque |= !runtime_callbacks.is_empty();
    Ok(ExecutedLuaSkin {
        value: json,
        warnings,
        files: skin_named_files,
        dependencies,
        lua_runtime,
        runtime_callbacks,
    })
}

pub(super) struct LuaSkinRuntimeRequest<'a> {
    pub(super) input: &'a Path,
    pub(super) path_context: &'a SkinPathContext,
    pub(super) source: &'a str,
    pub(super) options: &'a BTreeMap<String, String>,
    pub(super) skin_config_options: &'a BTreeMap<String, i64>,
    pub(super) skin_files: &'a BTreeMap<String, String>,
    pub(super) skin_file_dependency_names: &'a BTreeMap<String, String>,
    pub(super) skin_offsets: &'a BTreeMap<String, LuaSkinOffsetValue>,
    pub(super) runtime_state: &'a LuaLoadRuntimeState,
    pub(super) virtual_io_files: &'a BTreeMap<String, String>,
    pub(super) runtime_callbacks: &'a [LuaRuntimeCallbackSpec],
}

pub(super) fn build_lua_skin_runtime(request: LuaSkinRuntimeRequest<'_>) -> Result<LuaSkinRuntime> {
    let LuaSkinRuntimeRequest {
        input,
        path_context,
        source,
        options,
        skin_config_options,
        skin_files,
        skin_file_dependency_names,
        skin_offsets,
        runtime_state,
        virtual_io_files,
        runtime_callbacks,
    } = request;
    let lua = Lua::new();
    let instruction_budget = install_instruction_limit(&lua);
    // This is a clean runtime VM. Installing the sandbox and evaluating the
    // skin creates closures, but no draw callback is invoked here.
    install_sandbox(
        &lua,
        path_context,
        options,
        Some(skin_config_options),
        skin_files,
        skin_file_dependency_names,
        skin_offsets,
        runtime_state,
        virtual_io_files,
        None,
    )?;
    let main_state_dispatch = install_runtime_main_state_dispatch(&lua)?;
    let value = lua
        .load(source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute runtime Lua skin: {}", input.display()))?;
    apply_lua_scene_boolean_state(&lua, runtime_state, None)?;

    let mut callbacks = Vec::with_capacity(runtime_callbacks.len());
    for (callback_id, spec) in runtime_callbacks.iter().enumerate() {
        let path = &spec.path;
        let callback =
            lua_value_at_field_path(value.clone(), path).ok().and_then(|value| match value {
                Value::Function(function) => lua.create_registry_value(function).ok(),
                _ => None,
            });
        if callback.is_some() {
            tracing::debug!(
                skin = %input.display(),
                callback_id,
                field_path = path,
                classification = "RUNTIME",
                kind = ?spec.kind,
                "registered Lua callback"
            );
        } else {
            tracing::warn!(
                skin = %input.display(),
                callback_id,
                field_path = path,
                classification = "ERROR",
                kind = ?spec.kind,
                "failed to register Lua callback; callback uses its safe fallback value"
            );
        }
        callbacks.push(LuaRuntimeCallback { path: path.clone(), kind: spec.kind, key: callback });
    }
    Ok(LuaSkinRuntime {
        lua,
        main_state_dispatch,
        callbacks,
        instruction_budget,
        skin_path: input.to_path_buf(),
        failed_callbacks: BTreeSet::new(),
        failure_log_count: 0,
        pending_frame_start: true,
    })
}

pub(super) fn lua_value_at_field_path(mut value: Value, path: &str) -> Result<Value> {
    let bytes = path.as_bytes();
    if bytes.first() != Some(&b'$') {
        bail!("invalid Lua callback field path: {path}");
    }
    let mut cursor = 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'.' => {
                cursor += 1;
                let start = cursor;
                while cursor < bytes.len() && !matches!(bytes[cursor], b'.' | b'[') {
                    cursor += 1;
                }
                if start == cursor {
                    bail!("empty Lua callback field in path: {path}");
                }
                let key = &path[start..cursor];
                let Value::Table(table) = value else {
                    bail!("Lua callback path parent is not a table at {key}: {path}");
                };
                value = table.get::<Value>(key)?;
            }
            b'[' => {
                cursor += 1;
                let start = cursor;
                while cursor < bytes.len() && bytes[cursor] != b']' {
                    cursor += 1;
                }
                if cursor >= bytes.len() {
                    bail!("unterminated Lua callback array index: {path}");
                }
                let index = path[start..cursor]
                    .parse::<i64>()
                    .with_context(|| format!("invalid Lua callback array index: {path}"))?;
                cursor += 1;
                let Value::Table(table) = value else {
                    bail!("Lua callback array parent is not a table at [{index}]: {path}");
                };
                value = table.get::<Value>(index)?;
            }
            _ => bail!("invalid Lua callback field path: {path}"),
        }
    }
    Ok(value)
}
use super::*;
