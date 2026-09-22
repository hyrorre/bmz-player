use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unix_epoch_year_for_test() -> i32 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (y + if month <= 2 { 1 } else { 0 }) as i32
}

fn unique_test_dir(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
}

#[derive(Default)]
struct TestLuaMainState {
    options: BTreeMap<i32, bool>,
    numbers: BTreeMap<i32, i64>,
    floats: BTreeMap<i32, f64>,
    texts: BTreeMap<i32, String>,
    timers: BTreeMap<i32, i32>,
    offsets: BTreeMap<i32, LuaSkinOffsetValue>,
    session_counts: (i64, i64),
    score_date: i64,
}

impl LuaMainState for TestLuaMainState {
    fn option(&self, id: i32) -> bool {
        self.options.get(&id).copied().unwrap_or(false)
    }

    fn number(&self, id: i32) -> i64 {
        self.numbers.get(&id).copied().unwrap_or_default()
    }

    fn float(&self, id: i32) -> f64 {
        self.floats.get(&id).copied().unwrap_or_default()
    }

    fn text(&self, id: i32) -> String {
        self.texts.get(&id).cloned().unwrap_or_default()
    }

    fn timer(&self, id: i32) -> Option<i32> {
        self.timers.get(&id).copied()
    }

    fn event_index(&self, _id: i32) -> i32 {
        0
    }

    fn gauge_type(&self) -> i32 {
        0
    }

    fn time_us(&self) -> i32 {
        0
    }

    fn total_play_counts_in_session(&self) -> i64 {
        self.session_counts.0
    }
    fn total_play_notes_in_session(&self) -> i64 {
        self.session_counts.1
    }
    fn score_date_sec_time(&self) -> i64 {
        self.score_date
    }

    fn offset(&self, id: i32) -> LuaSkinOffsetValue {
        self.offsets.get(&id).copied().unwrap_or_default()
    }
}

fn load_runtime_draw_fixture(name: &str, draw_source: &str) -> LoadedSkinDocument {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        format!(
            r#"
                local main_state = require("main_state")
                {draw_source}
                return {{
                    type = 0,
                    destination = {{{{
                        id = "runtime",
                        draw = draw,
                        dst = {{{{ x = 0, y = 0, w = 1, h = 1 }}}}
                    }}}}
                }}
                "#
        ),
    )
    .unwrap();
    load_lua_skin(&path, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new()).unwrap()
}

fn load_runtime_value_fixture(
    name: &str,
    runtime_mode: LuaSkinRuntimeMode,
    value_source: &str,
) -> LoadedSkinDocument {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        format!(
            r#"
                local main_state = require("main_state")
                {value_source}
                return {{
                    type = 0,
                    value = {{{{
                        id = "runtime-number",
                        value = number_value
                    }}}},
                    text = {{{{
                        id = "runtime-text",
                        value = text_value
                    }}}}
                }}
                "#
        ),
    )
    .unwrap();
    let runtime_state = LuaLoadRuntimeState { runtime_mode, ..Default::default() };
    load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), &runtime_state)
        .unwrap()
}

fn only_destination_draw(loaded: &LoadedSkinDocument) -> &str {
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("expected single destination")
    };
    &destination.draw
}

fn assert_compiled_or_runtime_draw(
    loaded: &mut LoadedSkinDocument,
    draw: &str,
    expected: &str,
    visible: bool,
    state: &TestLuaMainState,
) {
    if let Some(id) = draw.strip_prefix("bmz:lua_draw_callback:") {
        assert_eq!(
            loaded.lua_runtime.as_mut().unwrap().evaluate_draw(id.parse().unwrap(), state),
            visible
        );
    } else {
        assert_eq!(draw, expected);
    }
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/cases_05.rs"]
mod cases_05;
#[path = "tests/cases_06.rs"]
mod cases_06;
#[path = "tests/cases_07.rs"]
mod cases_07;
