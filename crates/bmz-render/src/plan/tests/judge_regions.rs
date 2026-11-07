use super::*;
use crate::skin::{DynamicTimerRuntime, SkinContext, SkinManifest};
use crate::snapshot::OpponentRenderSnapshot;

// One atlas cell per judgement, with a separate texture for combo digits.
// Child destinations use the public judge timers, including after history expires.
fn judge_skin(regions: usize) -> SkinContext {
    let images = (0..6)
        .map(|index| {
            serde_json::json!({
                "id": format!("label-{index}"), "src": "labels",
                "x": index * 10, "y": 0, "w": 10, "h": 10
            })
        })
        .collect::<Vec<_>>();
    let judges = (0..regions)
        .map(|region| {
            let timer = [46, 47, 247][region];
            let labels = (0..6)
                .map(|index| {
                    serde_json::json!({
                        "id": format!("label-{index}"), "timer": timer, "loop": -1,
                        "dst": [{"time": 0, "x": 10 + region * 50, "y": 40, "w": 20, "h": 10}, {"time": 1000}]
                    })
                })
                .collect::<Vec<_>>();
            let number = serde_json::json!({
                "id": "combo", "timer": timer, "loop": -1,
                "dst": [{"time": 0, "x": 20, "y": 0, "w": 5, "h": 10}, {"time": 1000}]
            });
            serde_json::json!({
                "id": format!("judge-{region}"), "index": region, "shift": false,
                "images": labels, "numbers": [number.clone(), number.clone(), number]
            })
        })
        .collect::<Vec<_>>();
    let document = serde_json::from_value(serde_json::json!({
        "type": 0, "w": 100, "h": 100,
        "image": images,
        "value": [{"id": "combo", "src": "digits", "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 4}],
        "judge": judges,
        "destination": (0..regions).map(|region| serde_json::json!({"id": format!("judge-{region}")})).collect::<Vec<_>>()
    }))
    .unwrap();
    SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        [
            SkinDocumentTexture {
                source_id: "labels".into(),
                texture: SkinTextureId(201),
                source_size: SkinImageSize { width: 60.0, height: 10.0 },
            },
            SkinDocumentTexture {
                source_id: "digits".into(),
                texture: SkinTextureId(202),
                source_size: SkinImageSize { width: 100.0, height: 10.0 },
            },
        ],
    )
}

fn judgement(lane: Lane, judge: Judge, combo: u32, time: i64) -> DisplayJudgement {
    DisplayJudgement {
        lane,
        judge,
        side: Some(TimingSide::Fast),
        text: format!("{judge:?}"),
        combo,
        delta_us: -3_000,
        time: TimeUs(time),
        is_miss: judge == Judge::Poor,
        timing_ms_suppressed: false,
    }
}

fn battle_snapshot(key_mode: KeyMode, time: i64, history: Vec<DisplayJudgement>) -> RenderSnapshot {
    RenderSnapshot {
        key_mode,
        skin_attempt: crate::snapshot::SkinAttemptState {
            source_key_mode: Some(match key_mode {
                KeyMode::K10 => KeyMode::K5,
                KeyMode::K14 => KeyMode::K7,
                _ => key_mode,
            }),
            ..Default::default()
        },
        time: TimeUs(time),
        play_elapsed_time: TimeUs(time),
        opponent: Some(OpponentRenderSnapshot::default()),
        recent_judgements: history,
        ..Default::default()
    }
}

#[test]
fn native_dp_opponent_keeps_a_shared_judge_region() {
    for key_mode in [KeyMode::K10, KeyMode::K14] {
        let skin = judge_skin(1);
        let mut snapshot = battle_snapshot(
            key_mode,
            40_000,
            vec![
                judgement(Lane::Key1, Judge::Great, 7, 30_000),
                judgement(Lane::Scratch2, Judge::PGreat, 160, 31_000),
            ],
        );
        // A native DP chart with an opponent has no duplicated SP presentation lanes.
        snapshot.skin_attempt.source_key_mode = Some(key_mode);
        let state = build_play_skin_state(&snapshot, &skin, 40);
        assert_eq!(state.judge_combo, [160, 0, 0]);
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut DynamicTimerRuntime::default(),
        );
        assert_eq!(rendered_digit_counts(&plan), [3, 0]);
    }
}

fn rendered_judges(plan: &DrawPlan) -> Vec<(usize, usize)> {
    plan.commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { texture: TextureId(201), rect, uv, .. } => {
                Some((usize::from(rect.x >= 0.5), (uv.x * 6.0).round() as usize))
            }
            _ => None,
        })
        .collect()
}

fn rendered_digit_counts(plan: &DrawPlan) -> [usize; 2] {
    let mut counts = [0; 2];
    for command in &plan.commands {
        if let DrawCommand::Image { texture: TextureId(202), rect, .. } = command {
            counts[usize::from(rect.x >= 0.5)] += 1;
        }
    }
    counts
}

#[test]
fn battle_fallback_judge_keeps_combo_break_while_opponent_continues() {
    for (key_mode, own_lane, other_lane) in
        [(KeyMode::K10, Lane::Key5, Lane::Scratch2), (KeyMode::K14, Lane::Key7, Lane::Key14)]
    {
        let skin = judge_skin(1);
        let mut runtime = DynamicTimerRuntime::default();
        let mut history = vec![
            judgement(own_lane, Judge::PGreat, 7, 10_000),
            judgement(other_lane, Judge::PGreat, 160, 11_000),
        ];
        let mut draw = |now, history: Vec<DisplayJudgement>| {
            DrawPlan::from_scene_with_skin(
                &AppSceneSnapshot::Play(battle_snapshot(key_mode, now, history)),
                &skin,
                &mut runtime,
            )
        };
        assert_eq!(rendered_digit_counts(&draw(20_000, history.clone())), [1, 0]);

        history.extend([
            judgement(own_lane, Judge::Poor, 0, 30_000),
            judgement(other_lane, Judge::PGreat, 161, 31_000),
        ]);
        for plan in [draw(40_000, history), draw(900_000, Vec::new())] {
            assert_eq!(rendered_judges(&plan), [(0, 4)], "{key_mode:?}");
            assert_eq!(rendered_digit_counts(&plan), [0, 0]);
        }

        let plan = draw(920_000, vec![judgement(other_lane, Judge::PGreat, 162, 910_000)]);
        assert_eq!(rendered_judges(&plan), [(0, 4)]);
        assert_eq!(rendered_digit_counts(&plan), [0, 0]);

        // Opponent hits must not restart the player's timer or keep an expired label visible.
        let plan = draw(1_040_000, vec![judgement(other_lane, Judge::PGreat, 163, 1_030_000)]);
        assert!(rendered_judges(&plan).is_empty());
        assert_eq!(rendered_digit_counts(&plan), [0, 0]);

        let plan = draw(
            1_070_000,
            vec![
                judgement(own_lane, Judge::Great, 1, 1_050_000),
                judgement(other_lane, Judge::PGreat, 164, 1_060_000),
            ],
        );
        assert_eq!(rendered_judges(&plan), [(0, 1)]);
        assert_eq!(rendered_digit_counts(&plan), [1, 0]);
    }
}

#[test]
fn battle_dedicated_skin_draws_each_players_judge_and_combo() {
    for key_mode in [KeyMode::K10, KeyMode::K14] {
        let skin = judge_skin(2);
        let snapshot = battle_snapshot(
            key_mode,
            40_000,
            vec![
                judgement(Lane::Scratch, Judge::Poor, 0, 30_000),
                judgement(Lane::Scratch2, Judge::PGreat, 160, 31_000),
            ],
        );
        let mut runtime = DynamicTimerRuntime::default();
        for snapshot in [snapshot, battle_snapshot(key_mode, 900_000, Vec::new())] {
            let plan = DrawPlan::from_scene_with_skin(
                &AppSceneSnapshot::Play(snapshot),
                &skin,
                &mut runtime,
            );
            assert_eq!(rendered_judges(&plan), [(0, 4), (1, 0)]);
            assert_eq!(rendered_digit_counts(&plan), [0, 3]);
        }
    }
}

#[test]
fn play_judge_regions_preserve_sp_dp_and_nine_key_layouts() {
    // DP without an opponent keeps its skin-defined shared or split judgement.
    // A 9K opponent does not turn the three PMS regions into a two-side battle layout.
    for (key_mode, regions, opponent, right_lane, right_region) in [
        (KeyMode::K5, 1, false, Lane::Key5, 0),
        (KeyMode::K7, 1, false, Lane::Key7, 0),
        (KeyMode::K9, 3, false, Lane::Key9, 2),
        (KeyMode::K9, 3, true, Lane::Key9, 2),
        (KeyMode::K10, 1, false, Lane::Scratch2, 0),
        (KeyMode::K14, 1, false, Lane::Key14, 0),
        (KeyMode::K10, 2, false, Lane::Key8, 1),
        (KeyMode::K14, 2, false, Lane::Scratch2, 1),
        (KeyMode::K10, 1, true, Lane::Key8, 1),
        (KeyMode::K14, 1, true, Lane::Scratch2, 1),
        (KeyMode::K14, 2, true, Lane::Key14, 1),
        (KeyMode::K14, 3, true, Lane::Key14, 1),
    ] {
        let skin = judge_skin(regions);
        let mut right = judgement(right_lane, Judge::PGreat, 160, 31_000);
        right.side = Some(TimingSide::Slow);
        right.delta_us = 7_000;
        let mut snapshot = battle_snapshot(
            key_mode,
            40_000,
            vec![judgement(Lane::Key1, Judge::Great, 7, 30_000), right],
        );
        if !opponent {
            snapshot.opponent = None;
        }
        let state = build_play_skin_state(&snapshot, &skin, 40);
        let mut expected_combo = [7, 0, 0];
        expected_combo[right_region] = 160;
        assert_eq!(
            state.judge_combo, expected_combo,
            "{key_mode:?} regions={regions} opponent={opponent}"
        );
        assert_eq!(state.judge_index[right_region], Some(0));
        assert_eq!(state.judge_ms[right_region], Some(9));
        assert_eq!(state.judge_timing_ms[right_region], Some(7));
        assert_eq!(state.judge_timing_sign[right_region], Some(-1));
        if right_region != 0 {
            assert_eq!(state.judge_index[0], Some(1));
            assert_eq!(state.judge_ms[0], Some(10));
            assert_eq!(state.judge_timing_ms[0], Some(-3));
            assert_eq!(state.judge_timing_sign[0], Some(1));
        }
    }
}
