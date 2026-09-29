use super::*;
use bmz_skin::{LuaLoadRuntimeState, SkinLoadDependencies};

fn state(score: i32) -> LuaLoadRuntimeState {
    LuaLoadRuntimeState { number_values: BTreeMap::from([(380, score)]), ..Default::default() }
}

fn installed(score: i32) -> SkinPipelineRuntime {
    let mut pipeline = SkinPipelineRuntime::new();
    pipeline.result_load_dependencies = Some(SkinLoadDependencies {
        number_values: state(score).number_values,
        ..Default::default()
    });
    pipeline
}

fn queue_refresh(
    pipeline: &mut SkinPipelineRuntime,
    previous: &mut LuaLoadRuntimeState,
    current: LuaLoadRuntimeState,
) -> u64 {
    assert!(pipeline.result_load_numbers_need_refresh(previous, current.clone()));
    *previous = current;
    let generation = pipeline.begin_result_load(true);
    pipeline.set_pending(SkinKind::Result, true);
    generation
}

#[test]
fn result_refresh_return_to_installed_values_rejects_late_upload() {
    // Hold B's completion until after the user has scrolled back to A.
    // Unrelated values may also change; only installed dependencies matter.
    for unrelated_change in [false, true] {
        let mut pipeline = installed(100);
        let mut previous = state(100);
        let b = queue_refresh(&mut pipeline, &mut previous, state(200));
        assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, state(200)));
        assert!(pipeline.is_pending(SkinKind::Result));

        let mut a = state(100);
        if unrelated_change {
            a.number_values.insert(381, 999);
        }
        assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, a.clone()));
        assert!(!pipeline.finish_upload(SkinKind::Result, b), "obsolete B must be rejected");
        assert!(!pipeline.is_pending(SkinKind::Result));
        assert_eq!(pipeline.result_refresh_generation, None);
        assert_eq!(previous, a);
        assert_eq!(pipeline.result_load_dependencies.as_ref().unwrap().number_values[&380], 100);
        let generation = pipeline.generation(SkinKind::Result);
        assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, a));
        assert_eq!(pipeline.generation(SkinKind::Result), generation);

        // Returning to B again must create a new request. A second late result
        // from the cancelled generation cannot clear this request's pending flag.
        let next_b = queue_refresh(&mut pipeline, &mut previous, state(200));
        assert!(!pipeline.finish_upload(SkinKind::Result, b));
        assert!(pipeline.is_pending(SkinKind::Result));
        assert!(pipeline.finish_upload(SkinKind::Result, next_b));
        assert!(!pipeline.is_pending(SkinKind::Result));
    }
}

#[test]
fn result_refresh_newer_values_win_in_either_completion_order() {
    for stale_first in [false, true] {
        let mut pipeline = installed(100);
        let mut previous = state(100);
        let b = queue_refresh(&mut pipeline, &mut previous, state(200));
        let c = queue_refresh(&mut pipeline, &mut previous, state(300));
        if stale_first {
            assert!(!pipeline.finish_upload(SkinKind::Result, b));
            assert!(pipeline.is_pending(SkinKind::Result));
        }
        assert!(pipeline.finish_upload(SkinKind::Result, c));
        if !stale_first {
            assert!(!pipeline.finish_upload(SkinKind::Result, b));
        }
        assert!(!pipeline.is_pending(SkinKind::Result));
        assert_eq!(previous, state(300));
        assert_eq!(pipeline.result_refresh_generation, Some(c));
    }
}

#[test]
fn result_refresh_does_not_supersede_pending_full_reload() {
    let mut pipeline = installed(100);
    let mut previous = state(100);
    let b = queue_refresh(&mut pipeline, &mut previous, state(200));
    let full = pipeline.begin_result_load(false);
    pipeline.set_pending(SkinKind::Result, true);
    assert_eq!(pipeline.result_refresh_generation, None);

    for current in [state(100), state(300)] {
        assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, current));
        assert_eq!(previous, state(200));
        assert_eq!(pipeline.generation(SkinKind::Result), full);
        assert!(pipeline.is_pending(SkinKind::Result));
    }
    assert!(!pipeline.finish_upload(SkinKind::Result, b));
    assert!(pipeline.is_pending(SkinKind::Result));
    assert!(pipeline.finish_upload(SkinKind::Result, full));

    // Once the full load is installed, IR changes can be refreshed normally.
    assert!(pipeline.result_load_numbers_need_refresh(&mut previous, state(300)));
}

#[test]
fn result_refresh_reuses_installed_skin_without_disturbing_other_scenes() {
    let mut pipeline = installed(100);
    let mut previous = state(100);
    let select = pipeline.bump_generation(SkinKind::Select);
    pipeline.set_pending(SkinKind::Select, true);
    let result = pipeline.generation(SkinKind::Result);
    let mut current = state(100);
    current.number_values.insert(381, 200);

    assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, current.clone()));
    assert_eq!(previous, current);
    assert_eq!(pipeline.generation(SkinKind::Result), result);
    assert!(pipeline.is_pending(SkinKind::Select));
    assert_eq!(pipeline.generation(SkinKind::Select), select);

    let b = queue_refresh(&mut pipeline, &mut previous, state(200));
    assert!(!pipeline.result_load_numbers_need_refresh(&mut previous, state(100)));
    assert!(!pipeline.finish_upload(SkinKind::Result, b));
    assert!(pipeline.is_pending(SkinKind::Select));
    assert_eq!(pipeline.generation(SkinKind::Select), select);
}
