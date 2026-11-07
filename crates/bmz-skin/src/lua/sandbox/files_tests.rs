use super::*;

fn java_file_vm(budget: usize) -> (Lua, crate::tests::TestDir) {
    let dir = crate::tests::TestDir::created("java-file-facade");
    let entry = dir.join("skin.lua");
    fs::write(&entry, "").unwrap();
    let context = SkinPathContext::new(&entry, [dir.to_path_buf()]).unwrap();
    let lua = Lua::new();
    let Value::Table(luajava) = create_luajava_stub(&lua).unwrap() else { unreachable!() };
    install_luajava_file_facade(
        &lua,
        &luajava,
        &context,
        SharedLuaFileOverlay::default(),
        None,
        budget,
    )
    .unwrap();
    lua.globals().set("luajava", luajava).unwrap();
    (lua, dir)
}

#[test]
fn java_file_rejects_oversized_paths_before_conversion() {
    let (lua, _) = java_file_vm(JAVA_FILE_NATIVE_MAX_BYTES);
    lua.load(r#"
        local File = luajava.bindClass("java.io.File")
        local function rejects(...)
            local ok, err = pcall(...)
            assert(not ok and tostring(err):find("Java file path limit exceeded", 1, true))
        end
        rejects(luajava.new, File, string.rep("x", 4097))
        rejects(luajava.newInstance, "java.io.File", string.rep(string.char(255), 1366))
        local parent = string.rep("p", 2048)
        local child = string.rep("c", 2048)
        rejects(luajava.new, File, parent, child)
        rejects(luajava.new, File, luajava.new(File, parent), child)
        rejects(luajava.new, File, { getPath = function() return string.rep("x", 4097) end }, "child")
    "#).exec().unwrap();
}

#[test]
fn java_file_missing_paths_and_directory_listing_use_overlay() {
    let (lua, dir) = java_file_vm(JAVA_FILE_NATIVE_MAX_BYTES);
    lua.load(
        r#"
        local File = luajava.bindClass("java.io.File")
        local parent = luajava.newInstance("java.io.File", "missing")
        assert(parent:getName() == "missing" and not parent:exists())
        assert(parent:mkdirs() and parent:isDirectory())
        local child = luajava.new(File, parent, "child")
        assert(child:mkdir() and not child:mkdir())
        local listed = parent:listFiles()
        assert(#listed == 1 and listed[1]:getName() == "child")
        assert(listed[1]:getPath() == child:getPath() and listed[1]:isDirectory())
    "#,
    )
    .exec()
    .unwrap();
    assert!(!dir.join("missing").exists());
}

#[test]
fn java_file_budget_counts_live_methods_and_listed_children() {
    let (lua, dir) = java_file_vm(2048);
    fs::create_dir(dir.join("parent")).unwrap();
    for n in 0..12 {
        fs::write(dir.join("parent").join(format!("child-{n}")), "").unwrap();
    }
    lua.load(
        r#"
        local File = luajava.bindClass("java.io.File")
        local held = {}
        for i = 1, 4 do held[i] = luajava.new(File, "parent") end
        local ok, err = pcall(luajava.newInstance, "java.io.File", string.rep("x", 800))
        assert(not ok and tostring(err):find("native memory limit exceeded", 1, true))

        -- A method can outlive its table and still retains the File.
        local list = held[1].listFiles
        held = nil
        collectgarbage("collect")
        collectgarbage("collect")
        ok, err = pcall(list)
        assert(not ok and tostring(err):find("native memory limit exceeded", 1, true))
        list = nil
        collectgarbage("collect")
        collectgarbage("collect")
        assert(luajava.newInstance("java.io.File", string.rep("x", 800)):getName():len() == 800)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn java_file_failed_lua_allocation_returns_reservation_without_gc() {
    let (lua, dir) = java_file_vm(JAVA_FILE_NATIVE_MAX_BYTES);
    let facade = Arc::new(JavaFileFacade {
        context: SkinPathContext::new(&dir.join("skin.lua"), [dir.to_path_buf()]).unwrap(),
        overlay: SharedLuaFileOverlay::default(),
        dependencies: None,
        remaining: AtomicUsize::new(512),
    });
    lua.gc_stop();
    lua.set_memory_limit(lua.used_memory() + 256).unwrap();
    assert!(matches!(
        create_java_file(&lua, &facade, "missing".into()),
        Err(mlua::Error::MemoryError(_))
    ));
    lua.set_memory_limit(0).unwrap();
    assert_eq!(facade.remaining.load(Ordering::Relaxed), 512);
    create_java_file(&lua, &facade, "missing".into()).unwrap();
}

fn counted_bytes(layer: &LuaFileLayer) -> usize {
    layer
        .files
        .iter()
        .map(|(key, file)| LuaFileLayer::entry_bytes(key, file.as_ref().map_or(0, Vec::capacity)))
        .sum::<usize>()
        + layer.directories.iter().map(|key| LuaFileLayer::entry_bytes(key, 0)).sum::<usize>()
}

#[test]
fn empty_files_directories_and_tombstones_share_the_entry_limit() {
    let mut layer = LuaFileLayer::default();
    for i in 0..LUA_FILE_STORE_MAX_ENTRIES {
        let key = PathBuf::from(format!("entry-{i}"));
        match i % 3 {
            0 => layer.create_directory(key).unwrap(),
            1 => layer.write(key, Some(Vec::new())).unwrap(),
            _ => layer.write(key, None).unwrap(),
        }
    }
    assert!(layer.create_directory("overflow-dir".into()).is_err());
    assert!(layer.write("overflow-file".into(), Some(Vec::new())).is_err());
    assert!(layer.write("overflow-deletion".into(), None).is_err());
    layer.create_directory("entry-0".into()).unwrap();
    layer.write("entry-1".into(), None).unwrap();
    assert_eq!(layer.bytes, counted_bytes(&layer));
    assert!(layer.bytes > LUA_FILE_STORE_MAX_ENTRIES * LUA_FILE_ENTRY_OVERHEAD);
}

#[test]
fn layer_counts_metadata_capacity_and_replacements_before_mutation() {
    let mut layer = LuaFileLayer::default();
    let long = PathBuf::from("x".repeat(LUA_FILE_PATH_MAX_BYTES + 1));
    assert!(layer.write(long.clone(), None).is_err());
    assert!(layer.create_directory(long).is_err());
    assert_eq!(layer.bytes, 0);
    let key = PathBuf::from("file");
    let capacity = LUA_FILE_STORE_MAX_BYTES - LuaFileLayer::entry_bytes(&key, 0);
    layer.write(key.clone(), Some(Vec::with_capacity(capacity))).unwrap();
    assert_eq!(layer.bytes, LUA_FILE_STORE_MAX_BYTES);
    assert!(layer.create_directory("dir".into()).is_err());
    assert!(layer.write("empty".into(), None).is_err());
    assert_eq!(layer.bytes, counted_bytes(&layer));
    layer.write(key.clone(), None).unwrap();
    layer.create_directory("dir".into()).unwrap();
    assert_eq!(layer.bytes, counted_bytes(&layer));
    assert!(layer.files[&key].is_none());
}

#[test]
fn merged_session_limits_are_atomic_and_account_for_overwritten_entries() {
    let store = LuaSkinFileStore::default();
    let mut first = LuaFileOverlay::new(Some(store.clone()));
    for i in 0..LUA_FILE_STORE_MAX_ENTRIES {
        first.create_directory(PathBuf::from(format!("dir-{i}"))).unwrap();
    }
    first.commit();
    assert!(first.staged.directories.is_empty());
    let mut next = LuaFileOverlay::new(Some(store.clone()));
    next.create_directory("extra".into()).unwrap();
    next.commit();
    assert!(next.staged.directories.is_empty());
    assert!(!store.0.lock().unwrap().directories.contains(Path::new("extra")));
    next.go_live();
    assert!(next.create_directory("extra".into()).is_err());
    assert!(next.write("extra".into(), None).is_err());
    next.begin_inference();
    next.create_directory("probe".into()).unwrap();
    next.commit();
    let stored = store.0.lock().unwrap();
    assert_eq!(stored.directories.len(), LUA_FILE_STORE_MAX_ENTRIES);
    assert_eq!(stored.bytes, counted_bytes(&stored));
    drop(stored);

    let mut a = LuaFileLayer::default();
    a.write("same".into(), Some(vec![0; 100])).unwrap();
    a.create_directory("dir".into()).unwrap();
    let mut b = LuaFileLayer::default();
    b.write("same".into(), None).unwrap();
    b.create_directory("dir".into()).unwrap();
    b.merge_into(&mut a).unwrap();
    assert_eq!(a.bytes, counted_bytes(&a));
    assert_eq!(a.files.len() + a.directories.len(), 2);
    assert_eq!(b.bytes, 0);
}
