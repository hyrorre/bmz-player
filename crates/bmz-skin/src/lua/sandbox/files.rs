//! Files that Lua skins write through `io`, `main_state.file_*` and the
//! `java.io.File` facade, kept in memory. Nothing is written to disk.
//!
//! A load runs the skin several times: the document VM's `main`, callback
//! inference in that VM, and the runtime VM's clean `main`. Only the document
//! `main` and the runtime callbacks may change the session's files, so the
//! other passes see their own writes but never keep them.
//!
//! Contents are bytes, as in LuaJ: a skin may write a string cut in the middle
//! of a UTF-8 character and compare it with the same string after reading.

use super::*;

/// Upper bound of all files a store keeps, in bytes.
const LUA_FILE_STORE_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Default)]
struct LuaFileLayer {
    /// `None` is a deletion, which also hides a file on disk.
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    directories: BTreeSet<PathBuf>,
}

impl LuaFileLayer {
    fn bytes(&self) -> usize {
        self.files.values().flatten().map(Vec::len).sum()
    }

    fn bytes_after_write(&self, key: &Path, len: usize) -> usize {
        let previous = self.files.get(key).and_then(Option::as_ref).map_or(0, Vec::len);
        self.bytes() - previous + len
    }

    fn merge_into(&mut self, target: &mut LuaFileLayer) {
        target.files.append(&mut self.files);
        target.directories.append(&mut self.directories);
    }
}

/// Files Lua skins write, kept in memory for as long as the owner keeps the
/// store; clones share the same files. The app keeps one per session, so a
/// skin reads back on its next load what it wrote before (beatoraja writes
/// those files into the skin folder).
#[derive(Debug, Clone, Default)]
pub struct LuaSkinFileStore(Arc<Mutex<LuaFileLayer>>);

impl PartialEq for LuaSkinFileStore {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LuaSkinFileStore {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LuaFilePhase {
    /// The skin's `main`: writes are staged for the session.
    Load,
    /// Callback inference: writes are probes and are discarded.
    Inference,
    /// Runtime callbacks: writes reach the session store directly.
    Live,
}

/// The files one Lua VM sees: its own writes over the session store.
#[derive(Debug)]
pub(super) struct LuaFileOverlay {
    store: Option<LuaSkinFileStore>,
    phase: LuaFilePhase,
    staged: LuaFileLayer,
    scratch: LuaFileLayer,
}

impl Default for LuaFileOverlay {
    fn default() -> Self {
        Self::new(None)
    }
}

impl LuaFileOverlay {
    pub(super) fn new(store: Option<LuaSkinFileStore>) -> Self {
        Self {
            store,
            phase: LuaFilePhase::Load,
            staged: LuaFileLayer::default(),
            scratch: LuaFileLayer::default(),
        }
    }

    /// Writes reach other loads, so a cached document would skip them.
    pub(super) fn keeps_files(&self) -> bool {
        self.store.is_some()
    }

    /// `Some(None)` is a deleted file; `None` means the disk decides.
    fn read(&self, key: &Path) -> Option<Option<Vec<u8>>> {
        if self.phase == LuaFilePhase::Inference
            && let Some(file) = self.scratch.files.get(key)
        {
            return Some(file.clone());
        }
        if let Some(file) = self.staged.files.get(key) {
            return Some(file.clone());
        }
        let store = self.store.as_ref()?.0.lock().ok()?;
        store.files.get(key).cloned()
    }

    fn is_directory(&self, key: &Path) -> bool {
        let layer_has = |layer: &LuaFileLayer| {
            layer.directories.contains(key)
                || layer.files.iter().any(|(path, file)| {
                    file.is_some() && path.as_path() != key && path.starts_with(key)
                })
        };
        (self.phase == LuaFilePhase::Inference && layer_has(&self.scratch))
            || layer_has(&self.staged)
            || self
                .store
                .as_ref()
                .and_then(|store| store.0.lock().ok())
                .is_some_and(|store| layer_has(&store))
    }

    fn write(&mut self, key: PathBuf, file: Option<Vec<u8>>) -> Result<(), String> {
        let len = file.as_ref().map_or(0, Vec::len);
        if len > LUA_IO_MAX_READ_BYTES {
            return Err(format!("skin file exceeds {LUA_IO_MAX_READ_BYTES} bytes"));
        }
        let limit_error = || format!("skin files exceed {LUA_FILE_STORE_MAX_BYTES} bytes");
        let layer = match self.phase {
            LuaFilePhase::Inference => &mut self.scratch,
            LuaFilePhase::Load => &mut self.staged,
            LuaFilePhase::Live => match &self.store {
                Some(store) => {
                    let mut store = store.0.lock().map_err(|_| "skin file store poisoned")?;
                    if store.bytes_after_write(&key, len) > LUA_FILE_STORE_MAX_BYTES {
                        return Err(limit_error());
                    }
                    store.files.insert(key, file);
                    return Ok(());
                }
                // Without a session the VM's own layer stands in for it.
                None => &mut self.staged,
            },
        };
        if layer.bytes_after_write(&key, len) > LUA_FILE_STORE_MAX_BYTES {
            return Err(limit_error());
        }
        layer.files.insert(key, file);
        Ok(())
    }

    fn create_directory(&mut self, key: PathBuf) {
        match (self.phase, &self.store) {
            (LuaFilePhase::Inference, _) => {
                self.scratch.directories.insert(key);
            }
            (LuaFilePhase::Live, Some(store)) => {
                if let Ok(mut store) = store.0.lock() {
                    store.directories.insert(key);
                }
            }
            _ => {
                self.staged.directories.insert(key);
            }
        }
    }

    /// Files and directories in `directory` known to the overlay, with
    /// `false` for deleted files.
    fn entries(&self, directory: &Path) -> BTreeMap<PathBuf, bool> {
        let mut entries = BTreeMap::new();
        let mut add = |layer: &LuaFileLayer| {
            for (path, file) in &layer.files {
                if path.parent() == Some(directory) {
                    entries.insert(path.clone(), file.is_some());
                } else if file.is_some()
                    && let Ok(rest) = path.strip_prefix(directory)
                    && let Some(child) = rest.components().next()
                {
                    entries.insert(directory.join(child), true);
                }
            }
            for path in &layer.directories {
                if path.parent() == Some(directory) {
                    entries.insert(path.clone(), true);
                }
            }
        };
        // Lowest precedence first, so a newer layer overwrites.
        if let Some(store) = self.store.as_ref().and_then(|store| store.0.lock().ok()) {
            add(&store);
        }
        add(&self.staged);
        if self.phase == LuaFilePhase::Inference {
            add(&self.scratch);
        }
        entries
    }

    /// The document VM's `main` finished; later writes are inference probes.
    pub(super) fn begin_inference(&mut self) {
        self.phase = LuaFilePhase::Inference;
    }

    /// The document load succeeded: its `main` writes become the session's.
    pub(super) fn commit(&mut self) {
        self.scratch = LuaFileLayer::default();
        let Some(store) = &self.store else {
            return;
        };
        let Ok(mut store) = store.0.lock() else {
            return;
        };
        let mut merged_bytes = store.bytes();
        for (key, file) in &self.staged.files {
            merged_bytes = merged_bytes
                .saturating_sub(store.files.get(key).and_then(Option::as_ref).map_or(0, Vec::len))
                + file.as_ref().map_or(0, Vec::len);
        }
        if merged_bytes > LUA_FILE_STORE_MAX_BYTES {
            tracing::warn!(
                limit = LUA_FILE_STORE_MAX_BYTES,
                "discarding Lua skin file writes over the session limit"
            );
            self.staged = LuaFileLayer::default();
            return;
        }
        self.staged.merge_into(&mut store);
    }

    /// The runtime VM's clean `main` finished; its writes repeat the document
    /// load and are dropped, and callbacks now write to the session.
    pub(super) fn go_live(&mut self) {
        if self.store.is_some() {
            self.staged = LuaFileLayer::default();
        }
        self.scratch = LuaFileLayer::default();
        self.phase = LuaFilePhase::Live;
    }
}

pub(super) type SharedLuaFileOverlay = Arc<Mutex<LuaFileOverlay>>;

/// The sandboxed name of a Lua file path: canonical when it exists,
/// otherwise its lexical path inside the configured skin roots.
///
/// Relative paths keep the `io` rules (no `..`); absolute paths, such as
/// `skin_config.get_path` results, must lie inside the skin roots.
pub(super) fn lua_file_key(path_context: &SkinPathContext, requested: &str) -> Option<PathBuf> {
    let requested = if Path::new(requested).is_absolute() {
        requested.to_string()
    } else {
        normalize_skin_io_relative_path(requested).ok()?
    };
    path_context.resolve_path_or_missing(&requested).ok()
}

fn note_kept_file_use(
    overlay: &LuaFileOverlay,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
) {
    if overlay.keeps_files() {
        mark_load_dependency_opaque(dependencies);
    }
}

fn lock_overlay(
    overlay: &SharedLuaFileOverlay,
) -> mlua::Result<std::sync::MutexGuard<'_, LuaFileOverlay>> {
    overlay.lock().map_err(|_| mlua::Error::external("Lua skin file overlay lock poisoned"))
}

/// Reads a skin file: app-supplied virtual files, then files the skin wrote,
/// then the disk inside the skin roots.
pub(super) fn read_lua_skin_file(
    path_context: &SkinPathContext,
    virtual_io_files: &BTreeMap<String, String>,
    overlay: &SharedLuaFileOverlay,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
    requested: &str,
) -> mlua::Result<Option<Vec<u8>>> {
    let relative = normalize_skin_io_relative_path(requested).ok();
    if let Some(relative) = &relative {
        let virtual_source = virtual_io_files.get(relative);
        record_virtual_io_dependency(relative, virtual_source.map(String::as_str), dependencies);
        if let Some(source) = virtual_source {
            return Ok(Some(source.clone().into_bytes()));
        }
    }
    if let Some(key) = lua_file_key(path_context, requested) {
        let overlay = lock_overlay(overlay)?;
        if let Some(file) = overlay.read(&key) {
            note_kept_file_use(&overlay, dependencies);
            return Ok(file);
        }
    }
    // Relative paths keep the entry-directory rules; absolute paths, such as
    // `skin_config.get_path` results, must stay inside the skin roots.
    let path = match &relative {
        Some(relative) => resolve_skin_io_path(path_context.entry_dir(), relative),
        None if Path::new(requested).is_absolute() => path_context.resolve_file(requested),
        None => return Ok(None),
    };
    let Some(source) = path.ok().and_then(|path| {
        let source = read_skin_io_source(&path).ok()?;
        record_lua_loaded_file_dependency(&path, dependencies);
        Some(source)
    }) else {
        mark_load_dependency_opaque(dependencies);
        return Ok(None);
    };
    Ok(Some(source))
}

/// The current content of a written or existing file; empty when missing.
fn lua_skin_file_content(overlay: &LuaFileOverlay, key: &Path) -> Vec<u8> {
    match overlay.read(key) {
        Some(file) => file.unwrap_or_default(),
        None if key.is_file() => read_skin_io_source(key).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Lines without their `\n` or `\r\n`, and no empty line after the last `\n`.
fn lua_skin_file_lines(content: &[u8]) -> impl Iterator<Item = &[u8]> {
    content
        .strip_suffix(b"\n")
        .unwrap_or(content)
        .split(|byte| *byte == b'\n')
        .filter(move |_| !content.is_empty())
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
}

fn lua_skin_file_exists(overlay: &LuaFileOverlay, key: &Path) -> bool {
    match overlay.read(key) {
        Some(file) => file.is_some(),
        None => overlay.is_directory(key) || key.exists(),
    }
}

fn write_lua_skin_file(
    overlay: &SharedLuaFileOverlay,
    dependencies: Option<&Arc<Mutex<SkinLoadDependencies>>>,
    key: &Path,
    text: &[u8],
    append: bool,
) -> mlua::Result<Result<(), String>> {
    let mut overlay = lock_overlay(overlay)?;
    note_kept_file_use(&overlay, dependencies);
    let content = if append {
        let mut content = lua_skin_file_content(&overlay, key);
        content.extend_from_slice(text);
        content
    } else {
        text.to_vec()
    };
    Ok(overlay.write(key.to_path_buf(), Some(content)))
}

/// Paths in a directory: the disk listing plus files the skin wrote, without
/// deleted ones. `None` when no such directory exists.
fn lua_skin_directory_entries(overlay: &LuaFileOverlay, directory: &Path) -> Option<Vec<PathBuf>> {
    let overlay_entries = overlay.entries(directory);
    let on_disk = directory.is_dir();
    if !on_disk && overlay_entries.is_empty() && !overlay.is_directory(directory) {
        return None;
    }
    let mut entries = BTreeSet::new();
    if on_disk && let Ok(read_dir) = fs::read_dir(directory) {
        entries.extend(read_dir.flatten().map(|entry| directory.join(entry.file_name())));
    }
    for (path, present) in overlay_entries {
        if present {
            entries.insert(path);
        } else {
            entries.remove(&path);
        }
    }
    Some(entries.into_iter().collect())
}

fn lua_text_argument(lua: &Lua, value: Value, function: &str) -> mlua::Result<Vec<u8>> {
    match lua.coerce_string(value)? {
        Some(text) => Ok(text.as_bytes().to_vec()),
        None => {
            Err(mlua::Error::runtime(format!("bad argument to '{function}' (string expected)")))
        }
    }
}

/// `io.open(path, "w" | "a")`. Writes go to the overlay as they happen, so the
/// skin can read them back before closing.
pub(super) fn create_lua_file_writer(
    lua: &Lua,
    key: Option<PathBuf>,
    overlay: SharedLuaFileOverlay,
    dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
    truncate: bool,
) -> mlua::Result<Value> {
    if truncate
        && let Some(key) = &key
        && let Err(error) = write_lua_skin_file(&overlay, dependencies.as_ref(), key, b"", false)?
    {
        tracing::warn!(path = %key.display(), %error, "Lua skin file write rejected");
        return Ok(Value::Nil);
    }
    let file = lua.create_table()?;
    file.set(
        "write",
        lua.create_function(move |lua, (file, args): (Table, Variadic<Value>)| {
            let mut text = Vec::new();
            for value in args {
                text.extend_from_slice(&lua_text_argument(lua, value, "write")?);
            }
            if let Some(key) = &key
                && !text.is_empty()
                && let Err(error) =
                    write_lua_skin_file(&overlay, dependencies.as_ref(), key, &text, true)?
            {
                tracing::warn!(path = %key.display(), %error, "Lua skin file write rejected");
                let error = Value::String(lua.create_string(&error)?);
                return Ok(mlua::MultiValue::from_vec(vec![Value::Nil, error]));
            }
            // `io.open(path, "a"):write(text):close()` chains on the file.
            Ok(mlua::MultiValue::from_vec(vec![Value::Table(file)]))
        })?,
    )?;
    file.set("flush", lua.create_function(|_, file: Table| Ok(file))?)?;
    file.set("close", lua.create_function(|_, _: Value| Ok(true))?)?;
    Ok(Value::Table(file))
}

/// beatoraja's `main_state.file_*` (`SkinFileLuaApiExporter`), over the
/// overlay instead of the skin folder.
pub(super) fn install_main_state_file_api(
    lua: &Lua,
    main_state: &Table,
    path_context: &SkinPathContext,
    overlay: SharedLuaFileOverlay,
    dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> mlua::Result<()> {
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    main_state.set(
        "file_exists",
        lua.create_function(move |_, path: String| {
            let Some(key) = lua_file_key(&context, &path) else { return Ok(false) };
            let overlay = lock_overlay(&files)?;
            if overlay.read(&key).is_some() || overlay.is_directory(&key) {
                note_kept_file_use(&overlay, deps.as_ref());
            } else if !key.exists() {
                // A later write could create it, which the cache cannot see.
                mark_load_dependency_opaque(deps.as_ref());
            }
            Ok(lua_skin_file_exists(&overlay, &key))
        })?,
    )?;
    let context = path_context.clone();
    let files = overlay.clone();
    main_state.set(
        "file_mkdir",
        lua.create_function(move |_, path: String| {
            let Some(key) = lua_file_key(&context, &path) else { return Ok(false) };
            lock_overlay(&files)?.create_directory(key);
            Ok(true)
        })?,
    )?;
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    main_state.set(
        "file_list",
        lua.create_function(move |lua, (path, pattern): (String, Option<String>)| {
            // The dependency tracker records files, not directory listings.
            mark_load_dependency_opaque(deps.as_ref());
            let entries = match lua_file_key(&context, &path) {
                Some(key) => lua_skin_directory_entries(&*lock_overlay(&files)?, &key),
                None => None,
            }
            .unwrap_or_default();
            let find: Function = lua.globals().get::<Table>("string")?.get("find")?;
            let mut listed = String::new();
            let mut count = 0;
            for entry in entries {
                let normalized = entry.to_string_lossy().replace('\\', "/");
                // beatoraja turns `%x` into a Java regex escape and keeps the
                // first match; Lua patterns cover the forms skins use.
                let matched = match pattern.as_deref().filter(|pattern| !pattern.is_empty()) {
                    None => Some(normalized),
                    Some(pattern) => {
                        match find.call::<(Option<usize>, Option<usize>)>((
                            normalized.as_str(),
                            pattern,
                        ))? {
                            (Some(start), Some(end)) if start >= 1 && start <= end + 1 => {
                                let bytes = normalized.as_bytes().get(start - 1..end);
                                Some(
                                    String::from_utf8_lossy(bytes.unwrap_or_default()).into_owned(),
                                )
                            }
                            _ => None,
                        }
                    }
                };
                if let Some(matched) = matched {
                    listed.push_str(&matched);
                    listed.push('\n');
                    count += 1;
                }
            }
            Ok((listed, count))
        })?,
    )?;
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    main_state.set(
        "file_read_lines",
        lua.create_function(move |lua, path: String| {
            let lines = lua.create_table()?;
            let Some(key) = lua_file_key(&context, &path) else { return Ok(lines) };
            let overlay = lock_overlay(&files)?;
            if overlay.read(&key).is_some() {
                note_kept_file_use(&overlay, deps.as_ref());
            }
            let content = lua_skin_file_content(&overlay, &key);
            for (index, line) in lua_skin_file_lines(&content).enumerate() {
                lines.set(index + 1, lua.create_string(line)?)?;
            }
            Ok(lines)
        })?,
    )?;
    for (name, append, clear) in
        [("file_write", false, false), ("file_append", true, false), ("file_clear", false, true)]
    {
        let context = path_context.clone();
        let files = overlay.clone();
        let deps = dependencies.clone();
        main_state.set(
            name,
            lua.create_function(move |lua, (path, text): (String, Value)| {
                let Some(key) = lua_file_key(&context, &path) else { return Ok(false) };
                let text = if clear { Vec::new() } else { lua_text_argument(lua, text, name)? };
                Ok(write_lua_skin_file(&files, deps.as_ref(), &key, &text, append)?.is_ok())
            })?,
        )?;
    }
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    main_state.set(
        "file_count_lines",
        lua.create_function(move |_, path: String| {
            let Some(key) = lua_file_key(&context, &path) else { return Ok(0) };
            let overlay = lock_overlay(&files)?;
            if overlay.read(&key).is_some() {
                note_kept_file_use(&overlay, deps.as_ref());
            }
            Ok(lua_skin_file_lines(&lua_skin_file_content(&overlay, &key)).count())
        })?,
    )?;
    let context = path_context.clone();
    let files = overlay;
    main_state.set(
        "file_delete",
        lua.create_function(move |_, path: String| {
            let Some(key) = lua_file_key(&context, &path) else { return Ok(false) };
            let mut overlay = lock_overlay(&files)?;
            let exists = match overlay.read(&key) {
                Some(file) => file.is_some(),
                None => key.is_file(),
            };
            if !exists {
                return Ok(false);
            }
            note_kept_file_use(&overlay, dependencies.as_ref());
            Ok(overlay.write(key, None).is_ok())
        })?,
    )?;
    Ok(())
}

const JAVA_FILE_CLASS: &str = "java.io.File";
const JAVA_CLASS_FIELD: &str = "bmz_java_class";

/// A `java.io.File` for skins that list or create folders through luajava
/// (`luajava.new(luajava.bindClass("java.io.File"), path)`), over the overlay.
fn create_java_file(
    lua: &Lua,
    path_context: &SkinPathContext,
    overlay: &SharedLuaFileOverlay,
    dependencies: &Option<Arc<Mutex<SkinLoadDependencies>>>,
    path: String,
) -> mlua::Result<Value> {
    let file = lua.create_table()?;
    let key = lua_file_key(path_context, &path);
    let name = Path::new(&path.replace('\\', "/"))
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    for getter in ["getPath", "getAbsolutePath", "toString"] {
        let path = path.clone();
        file.set(getter, lua.create_function(move |_, _: Value| Ok(path.clone()))?)?;
    }
    file.set("getName", lua.create_function(move |_, _: Value| Ok(name.clone()))?)?;
    let exists = |check: fn(&LuaFileOverlay, &Path) -> bool| {
        let key = key.clone();
        let overlay = overlay.clone();
        lua.create_function(move |_, _: Value| match &key {
            Some(key) => Ok(check(&*lock_overlay(&overlay)?, key)),
            None => Ok(false),
        })
    };
    file.set("exists", exists(lua_skin_file_exists)?)?;
    file.set(
        "isDirectory",
        exists(|overlay, key| {
            overlay.read(key).is_none() && (overlay.is_directory(key) || key.is_dir())
        })?,
    )?;
    file.set(
        "isFile",
        exists(|overlay, key| match overlay.read(key) {
            Some(file) => file.is_some(),
            None => key.is_file(),
        })?,
    )?;
    for creator in ["mkdir", "mkdirs"] {
        let key = key.clone();
        let overlay = overlay.clone();
        file.set(
            creator,
            lua.create_function(move |_, _: Value| {
                let Some(key) = &key else { return Ok(false) };
                let mut overlay = lock_overlay(&overlay)?;
                if lua_skin_file_exists(&overlay, key) {
                    return Ok(false);
                }
                overlay.create_directory(key.clone());
                Ok(true)
            })?,
        )?;
    }
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    let parent = path.clone();
    file.set(
        "listFiles",
        lua.create_function(move |lua, _: Value| {
            mark_load_dependency_opaque(deps.as_ref());
            let Some(key) = &key else { return Ok(Value::Nil) };
            let Some(entries) = lua_skin_directory_entries(&*lock_overlay(&files)?, key) else {
                return Ok(Value::Nil);
            };
            let children = lua.create_table()?;
            let separator =
                if parent.ends_with(['/', '\\']) { "" } else { std::path::MAIN_SEPARATOR_STR };
            for (index, entry) in entries.iter().enumerate() {
                let Some(name) = entry.file_name() else { continue };
                let child = format!("{parent}{separator}{}", name.to_string_lossy());
                children.set(index + 1, create_java_file(lua, &context, &files, &deps, child)?)?;
            }
            Ok(Value::Table(children))
        })?,
    )?;
    let metatable = lua.create_table()?;
    metatable.set("__tostring", lua.create_function(move |_, _: Value| Ok(path.clone()))?)?;
    file.set_metatable(Some(metatable));
    Ok(Value::Table(file))
}

fn java_file_path(lua: &Lua, args: &[Value]) -> mlua::Result<Option<String>> {
    let text = |value: &Value| {
        lua.coerce_string(value.clone()).map(|text| text.map(|text| text.to_string_lossy()))
    };
    Ok(match args {
        [path] => text(path)?,
        // `new File(parent, child)` with a String or File parent.
        [parent, child] => {
            let parent = match parent {
                Value::Table(file) => file.get::<Function>("getPath")?.call::<String>(())?,
                other => text(other)?.unwrap_or_default(),
            };
            text(child)?.map(|child| format!("{parent}{}{child}", std::path::MAIN_SEPARATOR))
        }
        _ => None,
    })
}

/// Routes `java.io.File` through [`create_java_file`] and leaves every other
/// luajava class on its existing inert stub.
pub(super) fn install_luajava_file_facade(
    lua: &Lua,
    luajava: &Table,
    path_context: &SkinPathContext,
    overlay: SharedLuaFileOverlay,
    dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
) -> mlua::Result<()> {
    let bind_class: Function = luajava.get("bindClass")?;
    luajava.set(
        "bindClass",
        lua.create_function(move |lua, class_name: String| {
            if class_name == JAVA_FILE_CLASS {
                let class = lua.create_table()?;
                class.set(JAVA_CLASS_FIELD, JAVA_FILE_CLASS)?;
                return Ok(Value::Table(class));
            }
            bind_class.call::<Value>(class_name)
        })?,
    )?;
    let new: Function = luajava.get("new")?;
    let context = path_context.clone();
    let files = overlay.clone();
    let deps = dependencies.clone();
    luajava.set(
        "new",
        lua.create_function(move |lua, args: Variadic<Value>| {
            if let Some(Value::Table(class)) = args.first()
                && class.get::<Option<String>>(JAVA_CLASS_FIELD)?.as_deref()
                    == Some(JAVA_FILE_CLASS)
                && let Some(path) = java_file_path(lua, &args[1..])?
            {
                return create_java_file(lua, &context, &files, &deps, path);
            }
            new.call::<Value>(args)
        })?,
    )?;
    let new_instance: Function = luajava.get("newInstance")?;
    let context = path_context.clone();
    luajava.set(
        "newInstance",
        lua.create_function(move |lua, (class_name, args): (String, Variadic<Value>)| {
            if class_name == JAVA_FILE_CLASS
                && let Some(path) = java_file_path(lua, &args)?
            {
                return create_java_file(lua, &context, &overlay, &dependencies, path);
            }
            new_instance.call::<Value>((class_name, args))
        })?,
    )?;
    Ok(())
}
