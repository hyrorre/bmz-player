use super::*;

pub(super) fn install_sandbox(
    lua: &Lua,
    path_context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    skin_config_options: Option<&BTreeMap<String, i64>>,
    skin_files: &BTreeMap<String, String>,
    skin_file_dependency_names: &BTreeMap<String, String>,
    skin_offsets: &BTreeMap<String, LuaSkinOffsetValue>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
    load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> Result<Arc<Mutex<MainStateProbe>>> {
    lua.set_memory_limit(LUA_MEMORY_LIMIT_BYTES).context("failed to set Lua skin memory limit")?;
    let main_state_probe = Arc::new(Mutex::new(MainStateProbe::default()));
    {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.runtime_mode = runtime_state.runtime_mode;
        probe.screen_size = runtime_state.screen_size;
    }
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
    if !runtime_state.text_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.text_values = runtime_state.text_values.clone();
    }
    if !runtime_state.option_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.option_values = runtime_state.option_values.clone();
    }
    if !runtime_state.event_index_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.event_index_values = runtime_state.event_index_values.clone();
    }
    if !runtime_state.offset_id_values.is_empty() {
        let mut probe =
            main_state_probe.lock().map_err(|_| anyhow!("main_state probe lock poisoned"))?;
        probe.offset_values = runtime_state.offset_id_values.clone();
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
        if let Some(load_dependencies) = &load_dependencies
            && let Ok(mut dependencies) = load_dependencies.lock()
        {
            dependencies.offset_values.extend(skin_offsets.clone());
        }
        skin_config.set("offset", offset)?;
        let file_path = lua.create_table()?;
        for (pattern, name) in skin_file_dependency_names {
            if let Some(selected) = skin_files.get(pattern) {
                file_path.set(name.as_str(), selected.as_str())?;
            }
        }
        skin_config.set("file_path", file_path)?;
        let context_for_get_path = path_context.clone();
        let skin_files_for_get_path = skin_files.clone();
        let skin_file_dependency_names_for_get_path = skin_file_dependency_names.clone();
        let dependencies_for_get_path = load_dependencies.clone();
        let mut pinned_random_paths = runtime_state
            .pinned_random_file_paths
            .iter()
            .map(|(pattern, paths)| {
                (pattern.clone(), std::collections::VecDeque::from(paths.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let get_path = lua.create_function_mut(move |_, requested: String| {
            record_skin_config_file_dependency(
                &requested,
                &skin_file_dependency_names_for_get_path,
                dependencies_for_get_path.as_ref(),
            );
            let random =
                skin_config_path_uses_random_selection(&requested, &skin_files_for_get_path);
            if random {
                // Random は同じ設定値でもロードごとに結果が変わるため、具体パスを
                // 埋め込んだ document を次回ロードへ再利用できない。
                mark_load_dependency_opaque(dependencies_for_get_path.as_ref());
            }
            let pattern = requested.replace('\\', "/");
            let pinned = random
                .then(|| pinned_random_paths.get_mut(&pattern).and_then(|paths| paths.pop_front()))
                .flatten();
            let resolved = if let Some(pinned) = pinned {
                // The app's previous selection is still subject to the same sandbox checks.
                context_for_get_path.resolve_path(&pinned)
            } else {
                skin_config_get_path(&context_for_get_path, &requested, &skin_files_for_get_path)
            };
            let path = match resolved {
                Ok(path) => path,
                Err(error) => {
                    mark_load_dependency_opaque(dependencies_for_get_path.as_ref());
                    return Err(mlua::Error::external(error));
                }
            };
            if random
                && let Some(dependencies) = &dependencies_for_get_path
                && let Ok(mut dependencies) = dependencies.lock()
            {
                dependencies
                    .random_file_paths
                    .entry(pattern)
                    .or_default()
                    .push(path.to_string_lossy().into_owned());
            }
            record_lua_loaded_file_dependency(&path, dependencies_for_get_path.as_ref());
            Ok(path.to_string_lossy().to_string())
        })?;
        skin_config.set("get_path", get_path)?;
        globals.set("skin_config", skin_config)?;
    }
    if let Some(dependencies_for_random) = load_dependencies.clone() {
        let math: Table = globals.get("math")?;
        let original_random: Function = math.get("random")?;
        let original_random_key = lua.create_registry_value(original_random)?;
        let random = lua.create_function(move |lua, args: Variadic<Value>| {
            // Lua がロード時に選んだ値を document へ埋め込む場合、runtime state や
            // ファイル更新だけではcache keyを再現できない。
            mark_load_dependency_opaque(Some(&dependencies_for_random));
            let original_random: Function = lua.registry_value(&original_random_key)?;
            original_random.call::<Value>(args)
        })?;
        math.set("random", random)?;
    }
    globals.set("os", create_os_stub(lua, main_state_probe.clone())?)?;
    globals.set(
        "io",
        create_io_stub(lua, path_context.entry_dir(), virtual_io_files, load_dependencies.clone())?,
    )?;
    globals.set("debug", Value::Nil)?;
    // LuaJ's standard globals expose luajava directly. Some beatoraja skins use
    // that global without calling require("luajava"), so keep both access paths
    // pointed at the same sandboxed compatibility table.
    globals.set("luajava", create_luajava_stub(lua)?)?;
    let package: Table = globals.get("package")?;
    package.set("path", path_context.initial_package_path())?;
    package.set("cpath", "")?;
    package.set("loadlib", Value::Nil)?;

    let print_skin_path = path_context.entry_file().to_path_buf();
    let print = lua.create_function(move |_, args: Variadic<Value>| {
        let parts =
            args.into_iter().map(|value| lua_value_to_log_string(&value)).collect::<Vec<_>>();
        tracing::info!(
            target: "bmz_skin::lua::print",
            skin = %print_skin_path.display(),
            "{}",
            parts.join("\t")
        );
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

    let context_for_dofile = path_context.clone();
    let dependencies_for_dofile = load_dependencies.clone();
    let dofile = lua.create_function(move |lua, path: String| {
        let path = match context_for_dofile.resolve_file(&path) {
            Ok(path) => path,
            Err(error) => {
                mark_load_dependency_opaque(dependencies_for_dofile.as_ref());
                return Err(mlua::Error::external(error));
            }
        };
        record_lua_loaded_file_dependency(&path, dependencies_for_dofile.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).eval::<Value>()
    })?;
    globals.set("dofile", dofile)?;

    let context_for_loadfile = path_context.clone();
    let dependencies_for_loadfile = load_dependencies.clone();
    let loadfile = lua.create_function(move |lua, path: String| {
        let path = match context_for_loadfile.resolve_file(&path) {
            Ok(path) => path,
            Err(error) => {
                mark_load_dependency_opaque(dependencies_for_loadfile.as_ref());
                return Err(mlua::Error::external(error));
            }
        };
        record_lua_loaded_file_dependency(&path, dependencies_for_loadfile.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        lua.load(&source).set_name(path.to_string_lossy().as_ref()).into_function()
    })?;
    globals.set("loadfile", loadfile)?;

    let main_state = create_main_state_stub(lua, main_state_probe.clone())?;
    lua.globals().set("bmz_main_state", main_state)?;

    let context_for_require = path_context.clone();
    let probe_for_require = main_state_probe.clone();
    let dependencies_for_require = load_dependencies.clone();
    let require = lua.create_function(move |lua, module: String| {
        let globals = lua.globals();
        let package: Table = globals.get("package")?;
        let loaded: Table = package.get("loaded")?;
        if let Ok(cached) = loaded.get::<Value>(module.as_str())
            && !matches!(cached, Value::Nil)
        {
            return Ok(cached);
        }

        let builtin = match module.as_str() {
            "main_state" => Some(globals.get("bmz_main_state")?),
            "timer_util" => Some(create_timer_util_module(lua, probe_for_require.clone())?),
            "event_util" => Some(create_event_util_module(lua)?),
            "luajava" => Some(globals.get("luajava")?),
            _ => None,
        };
        if let Some(value) = builtin {
            loaded.set(module, value.clone())?;
            return Ok(value);
        }

        let package_path = package.get::<String>("path")?;
        let mut attempted = Vec::new();
        let mut resolved = None;
        for template in package_path.split(';').filter(|template| !template.is_empty()) {
            let module_path = module.replace(['.', '\\'], "/");
            attempted.push(template.replace('\\', "/").replace('?', &module_path));
            if let Ok(Some(path)) = context_for_require.resolve_package_candidate(template, &module)
            {
                resolved = Some(path);
                break;
            }
        }
        let Some(path) = resolved else {
            mark_load_dependency_opaque(dependencies_for_require.as_ref());
            return Err(mlua::Error::runtime(format!(
                "module `{module}` not found in sandboxed package.path; tried: {}",
                attempted.join(", ")
            )));
        };
        record_lua_loaded_file_dependency(&path, dependencies_for_require.as_ref());
        let source = fs::read_to_string(&path).map_err(mlua::Error::external)?;
        let loader = lua.load(&source).set_name(path.to_string_lossy().as_ref()).into_function()?;
        let value = loader.call::<Value>((module.clone(), path.to_string_lossy().to_string()))?;
        if !matches!(value, Value::Nil) {
            loaded.set(module.as_str(), value)?;
        }
        let cached = loaded.get::<Value>(module.as_str())?;
        if !matches!(cached, Value::Nil) {
            return Ok(cached);
        }
        loaded.set(module, true)?;
        Ok(Value::Boolean(true))
    })?;
    globals.set("require", require)?;

    let timer_fn_map = lua.create_table()?;
    let timer_fn_metatable = lua.create_table()?;
    timer_fn_metatable.set("__mode", "k")?;
    timer_fn_map.set_metatable(Some(timer_fn_metatable));
    globals.set("bmz_timer_fn_map", timer_fn_map)?;

    Ok(main_state_probe)
}
