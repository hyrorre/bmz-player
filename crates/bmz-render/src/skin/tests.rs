use bmz_core::ids::NoteId;
use bmz_core::input::{InputDeviceKind, InputEvent, InputKind, InputSource};
use bmz_core::time::TimeUs;

use crate::plan::TextLayer;

use super::*;

fn judge_region_state(region: usize, ms: i32, image_index: usize) -> JudgeRegionState {
    let mut judge_ms = [None; MAX_JUDGE_REGIONS];
    let mut judge_index = [None; MAX_JUDGE_REGIONS];
    let mut judge_combo = [0; MAX_JUDGE_REGIONS];
    let mut judge_timing_sign = [None; MAX_JUDGE_REGIONS];
    if region < MAX_JUDGE_REGIONS {
        judge_ms[region] = Some(ms);
        judge_index[region] = Some(image_index);
        judge_combo[region] = 42;
        judge_timing_sign[region] = Some(1);
    }
    JudgeRegionState {
        judge_ms,
        judge_index,
        judge_combo,
        judge_timing_sign,
        judge_timing_ms: [None; MAX_JUDGE_REGIONS],
    }
}

fn mock_source(id: &str, width: f32, height: f32) -> HashMap<String, SkinDocumentTexture> {
    let mut map = HashMap::new();
    map.insert(
        id.to_string(),
        SkinDocumentTexture {
            source_id: id.to_string(),
            texture: SkinTextureId(9999),
            source_size: SkinImageSize { width, height },
        },
    );
    map
}

fn approx_eq(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.0001
}

fn skin_render_item_has_rect_color(
    item: &SkinRenderItem,
    predicate: impl Fn(&Color) -> bool,
) -> bool {
    match item {
        SkinRenderItem::Rect { color, .. } => predicate(color),
        SkinRenderItem::RectBatch { rects, .. } => rects.iter().any(|rect| predicate(&rect.color)),
        _ => false,
    }
}

#[derive(Debug, Default)]
struct AlternatingLuaDrawRuntime {
    calls: std::sync::atomic::AtomicUsize,
}

impl SkinLuaDrawRuntime for AlternatingLuaDrawRuntime {
    fn evaluate_draw(
        &self,
        callback_id: usize,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
    ) -> bool {
        assert_eq!(callback_id, 0);
        (self.calls.fetch_add(1, Ordering::Relaxed) + 1).is_multiple_of(2)
    }

    fn evaluate_number(
        &self,
        callback_id: usize,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
    ) -> Option<f64> {
        (callback_id == 1).then_some(42.75)
    }

    fn evaluate_text(
        &self,
        callback_id: usize,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
    ) -> Option<String> {
        (callback_id == 2).then(|| "runtime text".to_string())
    }
}

#[path = "tests/core.rs"]
mod core;
#[path = "tests/graphs.rs"]
mod graphs;
#[path = "tests/graphs_more.rs"]
mod graphs_more;
#[path = "tests/play.rs"]
mod play;
#[path = "tests/property_names.rs"]
mod property_names;
#[path = "tests/result.rs"]
mod result;
#[path = "tests/runtime.rs"]
mod runtime;
#[path = "tests/select.rs"]
mod select;
