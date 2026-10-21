pub fn load_lua_skin_value(
    input: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    let path_context = SkinPathContext::for_entry(input)?;
    load_lua_skin_value_with_path_context(
        &path_context,
        options,
        files,
        runtime_state,
        virtual_io_files,
    )
}

pub fn load_lua_skin_value_with_path_context(
    path_context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    virtual_io_files: &BTreeMap<String, String>,
) -> Result<LoadedLuaSkinValue> {
    let ExecutedLuaSkin { value, warnings, files, dependencies, lua_runtime, runtime_callbacks } =
        execute_lua_skin(path_context, options, files, runtime_state, virtual_io_files)?;
    let runtime_callback_paths =
        runtime_callbacks.iter().map(|callback| callback.path.clone()).collect::<Vec<_>>();
    let runtime_draw_paths = runtime_callbacks
        .iter()
        .filter(|callback| callback.kind == LuaRuntimeCallbackKind::Draw)
        .map(|callback| callback.path.clone())
        .collect();
    Ok(LoadedLuaSkinValue {
        value,
        lua_runtime,
        runtime_callback_paths,
        runtime_draw_paths,
        warnings: warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
        files,
        dependencies,
        internal_enabled_options: Vec::new(),
    })
}

pub fn load_lua_skin_header_value(input: &Path) -> Result<LoadedLuaSkinValue> {
    let path_context = SkinPathContext::for_entry(input)?;
    load_lua_skin_header_value_with_path_context(&path_context)
}

pub fn load_lua_skin_header_value_with_path_context(
    path_context: &SkinPathContext,
) -> Result<LoadedLuaSkinValue> {
    let (value, warnings) = execute_lua_skin_header(path_context)?;
    Ok(LoadedLuaSkinValue {
        value,
        lua_runtime: None,
        runtime_callback_paths: Vec::new(),
        runtime_draw_paths: Vec::new(),
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
    let path_context = SkinPathContext::for_entry(input)?;
    convert_lua_skin_to_json_with_path_context(&path_context, output, options, files)
}

pub fn convert_lua_skin_to_json_with_path_context(
    path_context: &SkinPathContext,
    output: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<ConvertReport> {
    let ExecutedLuaSkin { value: mut json, mut warnings, runtime_callbacks, .. } =
        execute_lua_skin(
            path_context,
            options,
            files,
            &LuaLoadRuntimeState::default(),
            &BTreeMap::new(),
        )?;
    // A JSON skin cannot run Lua, so clicked `act` functions are dropped as
    // they were before the runtime VM ran them.
    let (acts, runtime_callbacks): (Vec<_>, Vec<_>) = runtime_callbacks
        .into_iter()
        .partition(|callback| callback.kind == LuaRuntimeCallbackKind::Act);
    if !acts.is_empty() {
        remove_lua_act_ids(&mut json);
        warnings.extend(acts.iter().map(|act| {
            format!("dropping Lua act function at {}; JSON skins cannot run Lua", act.path)
        }));
    }
    if !runtime_callbacks.is_empty() {
        bail!(
            "lua-to-json cannot serialize runtime callbacks: {}",
            runtime_callbacks
                .iter()
                .map(|callback| callback.path.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output dir: {}", parent.display()))?;
    }
    fs::write(output, serde_json::to_string_pretty(&json)? + "\n")
        .with_context(|| format!("failed to write json skin: {}", output.display()))?;

    Ok(ConvertReport { warnings })
}

fn remove_lua_act_ids(value: &mut JsonValue) {
    match value {
        JsonValue::Object(object) => {
            if object
                .get("act")
                .and_then(JsonValue::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .is_some_and(|id| bmz_skin_document::lua_act_callback_id(id).is_some())
            {
                object.remove("act");
            }
            object.values_mut().for_each(remove_lua_act_ids);
        }
        JsonValue::Array(items) => items.iter_mut().for_each(remove_lua_act_ids),
        _ => {}
    }
}

pub(super) fn execute_lua_skin_header(
    path_context: &SkinPathContext,
) -> Result<(JsonValue, Vec<String>)> {
    let input = path_context.entry_file();

    let mut warnings = Vec::new();
    let mut table_budget = TableBudget::default();
    let source = fs::read_to_string(input)
        .with_context(|| format!("failed to read lua skin: {}", input.display()))?;

    let lua = Lua::new();
    let instruction_budget = install_instruction_limit(&lua);
    let probe = install_sandbox(
        &lua,
        path_context,
        &BTreeMap::new(),
        None,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        &BTreeMap::new(),
        None,
    )?;
    let header = lua
        .load(&source)
        .set_name(input.to_string_lossy().as_ref())
        .eval::<Value>()
        .with_context(|| format!("failed to execute lua skin header: {}", input.display()))?;
    instruction_budget.begin_inference();
    let header_json = lua_value_to_json(
        &lua,
        header,
        "$",
        0,
        &mut warnings,
        &probe,
        &instruction_budget,
        &mut table_budget,
    )?;

    Ok((header_json, warnings))
}
use super::*;
