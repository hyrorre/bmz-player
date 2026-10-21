//! Release-only Lua API probe, excluding skin loading.
//! Usage: cargo run --release -p bmz-skin --example lua_runtime_profile
//! BMZ_LUA_PROFILE_FRAMES controls measured frames (default 10000).

use std::{collections::BTreeMap, hint::black_box, time::Instant};

use anyhow::{Result, ensure};
use bmz_skin::{LuaLoadRuntimeState, LuaMainState, LuaSkinRuntimeMode};

struct State;

impl LuaMainState for State {
    fn option(&self, id: i32) -> bool {
        id == 1
    }
    fn number(&self, id: i32) -> i64 {
        i64::from(id)
    }
    fn float(&self, _: i32) -> f64 {
        0.5
    }
    fn text(&self, _: i32) -> String {
        "profile".into()
    }
    fn timer(&self, _: i32) -> Option<i32> {
        Some(100)
    }
    fn event_index(&self, _: i32) -> i32 {
        2
    }
    fn gauge_type(&self) -> i32 {
        2
    }
    fn gauge(&self) -> f64 {
        82.0
    }
    fn time_us(&self) -> i32 {
        1_000_000
    }
}

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "run with --release");
    let frames = std::env::var("BMZ_LUA_PROFILE_FRAMES")
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(10_000);
    ensure!((1..=1_000_000).contains(&frames), "frames must be 1..=1000000");
    let root = tempfile::tempdir()?;
    let path = root.path().join("skin.luaskin");
    let source = r#"
        local s = require('main_state')
        local d = {}
        for i=1,500 do
            d[i] = {id='d'..i, draw=function() return s.option(1) end,
                dst={{x=0,y=0,w=1,h=1}}}
        end
        return {type=5, destination=d, text={{id='api', value=function()
            local sum = 0
            for i=1,10 do
                sum = sum + s.number(71) + s.rate() + s.exscore_best() + s.gauge()
                    + s.event_index(1)
            end
            return tostring(sum) .. ':' .. s.text(10) .. ':' .. s.time() .. ':' .. s.gauge_type()
        end}}}
    "#;
    std::fs::write(&path, source)?;
    let mut loaded = bmz_skin::load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    )?;
    let runtime = loaded.lua_runtime.as_mut().expect("compat runtime");
    let draw: Vec<_> = (0..runtime.callback_count())
        .filter(|&id| runtime.callback_path(id).is_some_and(|p| p.ends_with(".draw")))
        .collect();
    let text = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .expect("text callback");
    ensure!(draw.len() == 500, "expected 500 draw callbacks");
    let scope = runtime.state_scope();
    for shared in [false, true] {
        for api_heavy in [false, true] {
            let mut samples = Vec::with_capacity(frames);
            let mut result = None;
            for frame in 0..frames + 300 {
                runtime.begin_frame();
                let begin = Instant::now();
                let mut evaluate = || {
                    if api_heavy {
                        result = if shared {
                            runtime.evaluate_text_in_scope(text)
                        } else {
                            runtime.evaluate_text(text, &State)
                        };
                        black_box(&result);
                    } else {
                        for &id in &draw {
                            let visible = if shared {
                                runtime.evaluate_draw_in_scope(id)
                            } else {
                                runtime.evaluate_draw(id, &State)
                            };
                            assert!(black_box(visible));
                        }
                    }
                };
                if shared {
                    scope.with_state(&State, evaluate)?;
                } else {
                    evaluate();
                }
                if frame >= 300 {
                    samples.push(begin.elapsed().as_nanos() as u64);
                }
            }
            ensure!(runtime.failure_log_count() == 0, "callback failures");
            if api_heavy {
                ensure!(
                    result.as_deref() == Some("3055.0:profile:1000000:2"),
                    "unexpected {result:?}"
                );
            }
            let mean = samples.iter().sum::<u64>() as f64 / frames as f64;
            samples.sort_unstable();
            println!(
                "{}",
                serde_json::json!({"shared_scope":shared,
                "workload":if api_heavy { "api53" } else { "draw500" },
                "frames":frames,"mean_ns":mean,"median_ns":samples[frames/2],
                "result":result})
            );
        }
    }
    Ok(())
}
