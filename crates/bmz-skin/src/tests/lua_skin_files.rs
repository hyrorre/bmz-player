//! Files Lua skins write through `io`, `main_state.file_*` and luajava's
//! `java.io.File`, kept in memory (ModernChic's rotations, detail mode and
//! play history read back what they wrote).

use super::*;
use bmz_skin_document::lua_act_callback_id;

const LOG_SKIN: &str = r#"
if skin_config == nil then return {type = 5, name = "header"} end
local path = skin_config.get_path("io/log.txt")
local function read_all()
    local f = io.open(path, "r")
    if f == nil then return "nil" end
    local lines = {}
    for line in f:lines() do lines[#lines + 1] = line end
    f:close()
    return table.concat(lines, ",")
end
local before = read_all()
io.open(path, "a"):write("load\n"):close()
return {
    type = 5,
    name = before .. "|" .. read_all(),
    image = {{id = "btn", src = 0, x = 0, y = 0, w = 1, h = 1,
        act = function() io.open(path, "a"):write("act\n"):close() end}},
    destination = {
        {id = "btn", dst = {{x = 0, y = 0, w = 1, h = 1}}},
        -- Inference probes this; the probe's write must not reach the session.
        {id = "btn", draw = function()
            io.open(path, "a"):write("draw\n"):close()
            return true
        end, dst = {{x = 0, y = 0, w = 1, h = 1}}},
    },
}
"#;

fn skin_dir() -> TestDir {
    let root = TestDir::created("bmz-lua-skin-files");
    fs::create_dir(root.join("io")).unwrap();
    fs::create_dir(root.join("parts")).unwrap();
    for file in ["Play_b.png", "Play_a.png", "readme.txt"] {
        fs::write(root.join("parts").join(file), b"").unwrap();
    }
    root
}

fn load(path: &Path, store: Option<&LuaSkinFileStore>) -> Result<LoadedSkinDocument> {
    let state = LuaLoadRuntimeState { file_store: store.cloned(), ..Default::default() };
    load_lua_skin_with_runtime_state(path, &BTreeMap::new(), &BTreeMap::new(), &state)
}

#[test]
fn skin_io_writes_read_back_and_stay_in_the_session_store() {
    let root = skin_dir();
    let path = root.join("log.luaskin");
    fs::write(&path, LOG_SKIN).unwrap();

    // Without a store each load reads back only its own writes.
    for _ in 0..2 {
        assert_eq!(load(&path, None).unwrap().document.name, "nil|load");
    }

    let store = LuaSkinFileStore::default();
    assert_eq!(load(&path, Some(&store)).unwrap().document.name, "nil|load");
    // The runtime VM's repeated `main` and inference writes are dropped.
    let mut loaded = load(&path, Some(&store)).unwrap();
    assert_eq!(loaded.document.name, "load|load,load");
    assert!(loaded.dependencies.opaque, "loads that keep files are not cached");

    let image = loaded.document.image.iter().find(|image| image.id == "btn").unwrap();
    let act = lua_act_callback_id(image.act.unwrap()).unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    assert!(runtime.execute_act(act, 1, 0, &TestLuaMainState::default()));
    assert_eq!(runtime.failure_log_count(), 0);

    // A failed load keeps none of its writes.
    let broken = root.join("broken.luaskin");
    fs::write(
        &broken,
        r#"if skin_config == nil then return {type = 5} end
        io.open(skin_config.get_path("io/log.txt"), "a"):write("broken\n"):close()
        error("boom")"#,
    )
    .unwrap();
    assert!(load(&broken, Some(&store)).is_err());

    assert_eq!(
        load(&path, Some(&store)).unwrap().document.name,
        "load,load,act|load,load,act,load"
    );
    assert!(!root.join("io/log.txt").exists(), "nothing is written to disk");
}

#[test]
fn main_state_file_api_lists_skin_folders_and_written_files() {
    let root = skin_dir();
    let path = root.join("files.luaskin");
    fs::write(
        &path,
        r#"
        if skin_config == nil then return {type = 5} end
        local s = require("main_state")
        local list, count = s.file_list(skin_config.get_path("parts"), "Play.+%.png")
        local folder = skin_config.get_path("io") .. "/sub"
        local log = folder .. "/data.txt"
        local made = s.file_mkdir(folder)
        local folder_exists = s.file_exists(folder)
        s.file_write(log, "a\n")
        s.file_append(log, "b\n")
        local lines = s.file_read_lines(log)
        local listed, listed_count = s.file_list(folder)
        local result = {
            count, list, tostring(made), tostring(folder_exists), lines[1] .. lines[2],
            tostring(s.file_exists(log)), s.file_count_lines(log), listed_count,
            tostring(s.file_delete(log)), tostring(s.file_exists(log)),
            tostring(s.file_delete(log)), select(2, s.file_list(folder)),
        }
        for index, value in ipairs(result) do result[index] = tostring(value) end
        return {type = 5, name = table.concat(result, "|")}
        "#,
    )
    .unwrap();
    let name = load(&path, None).unwrap().document.name;
    assert_eq!(
        name, "2|Play_a.png\nPlay_b.png\n|true|true|ab|true|2|1|true|false|false|0",
        "{name}"
    );
    assert!(!root.join("io/sub").exists(), "nothing is written to disk");
}

#[test]
fn java_file_facade_lists_folders_for_skins_using_luajava() {
    let root = skin_dir();
    let path = root.join("java.luaskin");
    fs::write(
        &path,
        r#"
        if skin_config == nil then return {type = 5} end
        local luajava = require("luajava")
        local File = luajava.bindClass("java.io.File")
        local files = luajava.new(File, skin_config.get_path("parts")):listFiles()
        local names = {}
        for i = 1, #files do
            local text = string.gsub(tostring(files[i]), "\\", "/")
            names[#names + 1] = string.match(text, "Play.+%.png") or "-"
        end
        local folder = skin_config.get_path("io") .. "/new"
        local created = luajava.new(File, folder):mkdir()
        local again = luajava.new(File, folder):mkdir()
        local missing = luajava.new(File, skin_config.get_path("io") .. "/none"):listFiles()
        return {type = 5, name = table.concat(names, ",") .. "|" .. tostring(created)
            .. tostring(again) .. tostring(luajava.new(File, folder):isDirectory())
            .. tostring(missing)}
        "#,
    )
    .unwrap();
    assert_eq!(
        load(&path, None).unwrap().document.name,
        "Play_a.png,Play_b.png,-|truefalsetruenil"
    );
    assert!(!root.join("io/new").exists(), "nothing is written to disk");
}

/// MILLIONDOLLAR reads its daily note count back with `file:read("*n")`.
/// ModernChic keeps `string.sub(title, 1, 32)` as the key of its consecutive-play
/// log. The cut can fall inside a UTF-8 character, and LuaJ keeps those bytes.
#[test]
fn skin_io_keeps_strings_cut_inside_a_utf8_character() {
    let root = skin_dir();
    let path = root.join("cut.luaskin");
    fs::write(
        &path,
        r#"
        if skin_config == nil then return {type = 5, name = "header"} end
        local main_state = require("main_state")
        local file = skin_config.get_path("io/cut.txt")
        local cut = string.sub("日本語の曲名", 1, 4)
        local out = io.open(file, "w")
        out:write(cut, "\n", "second\r\n")
        out:close()
        local first
        for line in io.open(file):lines() do first = first or line end
        local read_all = io.open(file):read("*a")
        local listed = main_state.file_read_lines(file)
        return {
            type = 5,
            name = table.concat({
                tostring(first == cut),
                tostring(read_all == cut .. "\nsecond\r\n"),
                tostring(listed[1] == cut),
                listed[2],
                tostring(#listed),
                tostring(main_state.file_count_lines(file)),
                tostring(string.len(io.open(file):read(2))),
            }, "|"),
        }
        "#,
    )
    .unwrap();
    let store = LuaSkinFileStore::default();
    assert_eq!(load(&path, Some(&store)).unwrap().document.name, "true|true|true|second|2|2|2");
}

/// ModernChic's detail mode shows the log of consecutive plays of one chart. The
/// key is the load-time title cut to 32 bytes, here inside a UTF-8 character.
#[test]
fn modern_chic_consecutive_play_log_follows_the_chart_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/ModernChic471/play7_hw.luaskin");
    if !path.is_file() {
        return;
    }
    let options = BTreeMap::from([("プレイ状況詳細モード".to_string(), "有効".to_string())]);
    let store = LuaSkinFileStore::default();
    let play = |title: &str| {
        let state = LuaLoadRuntimeState {
            file_store: Some(store.clone()),
            option_values: BTreeMap::from([(32, true), (33, false)]),
            text_values: BTreeMap::from([(12, title.to_string()), (1003, String::new())]),
            ..Default::default()
        };
        let mut loaded =
            load_lua_skin_with_runtime_state(&path, &options, &BTreeMap::new(), &state).unwrap();
        let shown =
            loaded.document.value.iter().filter(|value| value.id.starts_with("n_count_")).count();
        let performance = loaded
            .document
            .value
            .iter()
            .find(|value| value.id == "performance")
            .and_then(|value| value.value_expr.strip_prefix("bmz:lua_value_callback:"))
            .map(|id| id.parse::<usize>().unwrap())
            .expect("detail mode is on");
        // The end of the chart writes the log, once.
        let mut main_state = TestLuaMainState { now_us: 60_000_000, ..Default::default() };
        main_state.options.insert(32, true);
        main_state.timers.insert(143, 100);
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        runtime.begin_frame();
        assert!(runtime.evaluate_number(performance, &main_state).is_some());
        assert_eq!(runtime.failure_log_count(), 0);
        shown
    };

    let first = "日本語の長い曲名テストケースその一 -ANOTHER-";
    assert_eq!(play(first), 0);
    assert_eq!(play(first), 1, "the second play shows the first");
    assert_eq!(play(first), 2);
    assert_eq!(play("別の曲"), 0, "another chart starts a new log");
    assert_eq!(play("別の曲"), 1);
    assert_eq!(play(first), 0);
}

#[test]
fn skin_io_reads_numbers_from_written_files() {
    let root = skin_dir();
    let path = root.join("number.luaskin");
    fs::write(
        &path,
        r#"
        if skin_config == nil then return {type = 5} end
        local path = skin_config.get_path("io/count.txt")
        local f = io.open(path, "w")
        f:write("2026-11-02", "\n", 42, "\n", " -1.5e2 x")
        f:close()
        f = io.open(path, "r")
        local day, count, scaled, rest = f:read("*l"), f:read("*n"), f:read("n"), f:read("*n")
        f:close()
        return {type = 5, name = day .. "|" .. math.type(count) .. count .. "|" .. scaled
            .. "|" .. tostring(rest)}
        "#,
    )
    .unwrap();
    assert_eq!(load(&path, None).unwrap().document.name, "2026-11-02|integer42|-150.0|nil");
}
