use super::*;
use crate::skin::{DynamicTimerRuntime, SkinDrawState, SkinLuaDrawRuntime};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct HashReader(Mutex<Vec<(String, String)>>);

impl SkinLuaDrawRuntime for HashReader {
    fn advance_custom_timers(
        &self,
        _: &SkinDrawState,
        _: &[i32],
        texts: &BTreeMap<i32, String>,
    ) -> BTreeMap<i32, Option<i64>> {
        self.0.lock().unwrap().push((texts[&1030].clone(), texts[&1031].clone()));
        BTreeMap::new()
    }

    fn evaluate_draw(
        &self,
        _: usize,
        _: &SkinDrawState,
        _: &[i32],
        _: &BTreeMap<i32, String>,
    ) -> bool {
        true
    }
}

#[test]
fn chart_hashes_follow_the_current_chart_in_every_scene() {
    for skin_type in [0, 5, 6, 7] {
        let document = serde_json::from_value(serde_json::json!({"type": skin_type})).unwrap();
        let mut skin = SkinContext::from_manifest_and_document(
            crate::skin::default_skin_manifest(),
            document,
            Vec::new(),
        );
        let reader = Arc::new(HashReader::default());
        skin.set_lua_draw_runtime(Some(reader.clone()));
        let mut timers = DynamicTimerRuntime::default();
        for byte in [Some(0xab), Some(0x12), None] {
            let chart_md5 = byte.map(|b| [b; 16]);
            let chart_sha256 = byte.map(|b| [b; 32]);
            let snapshot = RenderSnapshot { chart_md5, chart_sha256, ..Default::default() };
            skin.begin_frame();
            match skin_type {
                0 => {
                    play::plan_play(&snapshot, &skin, &mut timers);
                }
                6 => {
                    decide::plan_decide(&snapshot, &skin, &mut timers);
                }
                7 => {
                    let crate::scene::AppSceneSnapshot::Result(mut snapshot) =
                        crate::sample::sample_result_scene()
                    else {
                        unreachable!()
                    };
                    snapshot.chart_md5 = chart_md5;
                    snapshot.chart_sha256 = chart_sha256;
                    result::plan_result(&snapshot, &skin, &mut timers);
                }
                5 => {
                    select::plan_select(
                        &SelectSnapshot {
                            rows: vec![crate::scene::SelectRowSnapshot {
                                chart_md5,
                                chart_sha256,
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                        &skin,
                        &mut timers,
                    );
                }
                _ => unreachable!(),
            }
            let (md5, sha256) = reader.0.lock().unwrap().last().cloned().unwrap();
            assert_eq!(
                md5,
                byte.map(|b| format!("{b:02x}").repeat(16)).unwrap_or_default(),
                "type={skin_type}"
            );
            assert_eq!(
                sha256,
                byte.map(|b| format!("{b:02x}").repeat(32)).unwrap_or_default(),
                "type={skin_type}"
            );
        }
    }
}
