//! Load-time function inference must not depend on Lua's `pairs` order, which
//! follows a hash seed that changes for every VM. Each test loads the same
//! skin several times so that both table orders are exercised.

use super::*;

const LOADS: usize = 8;

fn load_fixture(name: &str, source: &str) -> LoadedSkinDocument {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(&path, source).unwrap();
    load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap()
}

fn destination_draws(loaded: &LoadedSkinDocument) -> Vec<(&str, &str)> {
    loaded
        .document
        .destination
        .iter()
        .map(|entry| {
            let bmz_skin_document::DestinationListEntry::Single(destination) = entry else {
                panic!("expected single destinations")
            };
            (destination.id.as_str(), destination.draw.as_str())
        })
        .collect()
}

/// Luxe Flat's Select score parts: number callbacks write the `flag_score` and
/// `scorerate` globals that the rank and score-rate `draw` functions read.
const LUXE_FLAT_SELECT_SCORE_SKIN: &str = r#"
    local main_state = require("main_state")
    flag_score = false
    rank_plus = true
    local function part(id, draw)
        return {id = id, draw = draw, dst = {{x = 0, y = 0, w = 1, h = 1}}}
    end
    return {
        type = 5,
        value = {
            {id = "default_playerdata_score_count", value = function()
                local score = main_state.number(71)
                if score < 0 then flag_score = false else flag_score = true end
                return score
            end},
            {id = "default_playerdata_mode_diff_count", value = function()
                local maxscore = main_state.number(74) * 2
                if maxscore < 1 then scorerate = 0
                else scorerate = main_state.number(71) / maxscore end
                return maxscore
            end},
        },
        destination = {
            part("diff_rank_max", function()
                return rank_plus and flag_score and scorerate < 18/18 and scorerate >= 17/18
            end),
            part("default_playerdata_scorerate_count", function()
                return flag_score and scorerate == 10/10
            end),
            part("per_sen", function()
                return flag_score and scorerate < 10/10 and scorerate >= 1/10
            end),
            part("dot_sen", function()
                return flag_score and scorerate < 1/10
            end),
            part("default_playerdata_diff_count", function()
                return not(rank_plus) and flag_score and scorerate >= 14/18
            end),
        },
    }
"#;

#[test]
fn luxe_flat_select_score_draws_do_not_depend_on_number_callbacks_running_first() {
    for _ in 0..LOADS {
        let mut loaded = load_fixture("bmz-lua-order-select-score", LUXE_FLAT_SELECT_SCORE_SKIN);
        let draws = destination_draws(&loaded)
            .into_iter()
            .map(|(id, draw)| (id.to_string(), draw.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(
            draws[..4],
            [
                ("diff_rank_max", "select_score_available() and nearest_rank(MAX,minus)"),
                (
                    "default_playerdata_scorerate_count",
                    "select_score_available() and score_rate_tenths(10,11)"
                ),
                ("per_sen", "select_score_available() and score_rate_tenths(1,10)"),
                ("dot_sen", "select_score_available() and score_rate_tenths(0,1)"),
            ]
            .map(|(id, draw)| (id.to_string(), draw.to_string())),
            "{draws:#?}"
        );
        // `not(rank_plus)` never holds in this skin. A leftover `scorerate`
        // from a number probe must not fold it into the score guard alone.
        let (_, hidden) = &draws[4];
        assert_ne!(hidden, "select_score_available()");
        let state = TestLuaMainState {
            numbers: BTreeMap::from([(71, 1_800), (74, 1_000)]),
            ..Default::default()
        };
        assert_compiled_or_runtime_draw(&mut loaded, hidden, "number(0) < 0", false, &state);
    }
}

/// Runtime callback IDs follow the conversion order, so a fixed order keeps
/// them stable between loads.
const RUNTIME_CALLBACK_SKIN: &str = r#"
    local main_state = require("main_state")
    local frames = 0
    local function tick()
        frames = frames + 1
        return frames
    end
    return {
        type = 5,
        value = {{id = "number", value = function() return tick() end}},
        text = {{id = "text", value = function() return tostring(tick()) end}},
        destination = {
            {id = "number", draw = function() return tick() % 2 == 0 end, dst = {{w = 1, h = 1}}},
            {id = "text", draw = function() return tick() % 3 == 0 end, dst = {{w = 1, h = 1}}},
        },
    }
"#;

#[test]
fn lua_runtime_callback_ids_follow_a_fixed_field_order() {
    for _ in 0..LOADS {
        let loaded = load_fixture("bmz-lua-order-callback-ids", RUNTIME_CALLBACK_SKIN);
        let runtime = loaded.lua_runtime.as_ref().unwrap();
        let paths = (0..runtime.callback_count())
            .map(|id| runtime.callback_path(id).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                "$.destination[1].draw",
                "$.destination[2].draw",
                "$.text[1].value",
                "$.value[1].value",
            ]
        );
    }
}
