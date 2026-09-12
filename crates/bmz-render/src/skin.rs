use std::collections::{BTreeMap, HashMap, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use bmz_core::input::InputKind;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_gameplay::session::{SkinRuntimeEvent, SkinRuntimeEventKind};
use serde::Deserialize;

use crate::assets::load_png_rgba;
use crate::plan::{
    Color, DrawCommand, PLAY_BACKBMP_TEXTURE, Point, Rect, RectBatchCache, RectBatchCacheKey,
    RectCommand, SELECT_BANNER_TEXTURE, SELECT_STAGE_TEXTURE, TextAlign, TextCaret, TextLayer,
    TextOutline, TextOverflow, TextShadow, TextStyle, TextureId, UvRect,
};
use crate::scene::{
    CourseConstraintFlags, CourseResultSkinSnapshot, DailyPlayerStatsSnapshot, PlayerStatsSnapshot,
    SelectRowKind, SelectRowSnapshot, SelectSnapshot,
};
use crate::skin_offset::{
    BEATORAJA_SKIN_OFFSET_MAX, SKIN_OFFSET_BAR_LINE, SkinOffsetValue, SkinOffsetValues,
};
use crate::snapshot::{
    CourseStageMarker, DisplayJudgeCounts, LongBodyState, SKIN_SOURCE_LN_DEFINED_CN_BIT,
    SKIN_SOURCE_LN_DEFINED_HCN_BIT, SKIN_SOURCE_LN_DEFINED_LN_BIT, SKIN_SOURCE_LN_UNDEFINED_BIT,
    SkinAttemptState,
};
use bmz_chart::model::LongNoteMode;

pub use bmz_skin_document::*;

mod condition;
mod document_render;
#[path = "skin/pm_chara.rs"]
mod pm_chara;
mod runtime;
mod select_state;
#[path = "skin/state_values/gauge_graph.rs"]
mod state_value_gauge_graph;
#[path = "skin/state_values/graph.rs"]
mod state_value_graph;
#[path = "skin/state_values/image.rs"]
mod state_value_image;
#[path = "skin/state_values/note_graph.rs"]
mod state_value_note_graph;
#[path = "skin/state_values/number/mod.rs"]
mod state_value_number;
#[path = "skin/state_values/text.rs"]
mod state_value_text;
#[path = "skin/state_values/text_state.rs"]
mod state_value_text_state;
#[path = "skin/state_values/timer.rs"]
mod state_value_timer;
#[path = "skin/state_values/timing_graph.rs"]
mod state_value_timing_graph;

pub use condition::test_skin_ops;
use condition::*;
pub use document_render::SkinDocumentRenderExt;
use pm_chara::*;
use runtime::*;
pub use runtime::{
    DynamicTimerRuntime, JudgeRegionState, MAX_JUDGE_REGIONS, SkinClickHit, SkinClickTarget,
    SkinContext, SkinDrawState, SkinLuaDrawRuntime, SkinLuaRuntimeContext, SkinSliderHit,
    build_judge_region_state, lane_judge_region,
};
pub use select_state::play_target_name;
pub use select_state::target_setting_name;
use select_state::*;
use state_value_gauge_graph::*;
use state_value_graph::*;
use state_value_image::*;
use state_value_note_graph::*;
pub use state_value_number::lane::{duration_to_green_number_ms, green_duration_to_duration_i32};
pub(crate) use state_value_number::result::result_grade_diff_label;
use state_value_number::*;
use state_value_text::*;
use state_value_text_state::*;
pub use state_value_text_state::{
    format_rm_skin_course_table_text, lua_main_state_event_index, lua_main_state_float,
    lua_main_state_number, lua_main_state_option, lua_main_state_timer,
};
pub use state_value_timer::skin_start_input_elapsed_ms;
use state_value_timer::*;
use state_value_timing_graph::*;

const OFFSET_ALL: i32 = 10;
#[cfg(test)]
const OFFSET_NOTES_1P: i32 = 30;
#[cfg(test)]
const OFFSET_JUDGE_1P: i32 = 32;

#[path = "skin/model.rs"]
mod skin_model;
pub use skin_model::*;

impl SkinObject {
    pub fn resolve(
        &self,
        phase: SkinPhase,
        elapsed_ms: i32,
        text: impl Fn(TextSlot) -> String,
        number: impl Fn(NumberSlot) -> i64,
    ) -> Vec<SkinRenderItem> {
        self.placements
            .iter()
            .filter(|placement| placement.phase == phase)
            .map(|placement| {
                let resolved = placement.resolve(elapsed_ms);
                match &self.source {
                    SkinSource::Image { texture, uv } => SkinRenderItem::Image {
                        texture: *texture,
                        rect: resolved.rect,
                        uv: *uv,
                        tint: Color::rgba(1.0, 1.0, 1.0, resolved.alpha),
                        blend: resolved.blend,
                        scale: SkinImageScale::Stretch,
                        border: None,
                        source_size: None,
                        linear_filter: false,
                    },
                    SkinSource::Text { slot, style } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: text(*slot),
                        style: style.clone().with_alpha(resolved.alpha),
                        caret: None,
                        blend: resolved.blend,
                        post_scale: Point { x: 1.0, y: 1.0 },
                    },
                    SkinSource::Number { slot, style, digits } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: format_number(number(*slot), *digits),
                        style: style.clone().with_alpha(resolved.alpha),
                        caret: None,
                        blend: resolved.blend,
                        post_scale: Point { x: 1.0, y: 1.0 },
                    },
                    SkinSource::Rect { color } => SkinRenderItem::Rect {
                        rect: resolved.rect,
                        color: color.with_alpha(color.a * resolved.alpha),
                        blend: resolved.blend,
                    },
                }
            })
            .collect()
    }
}

impl SkinDefinition {
    pub fn resolve(&self, context: &SkinRenderContext<'_>) -> Vec<SkinRenderItem> {
        self.objects
            .iter()
            .flat_map(|object| {
                object.resolve(
                    context.phase,
                    context.elapsed_ms,
                    |slot| lookup_text(context.text, slot),
                    |slot| lookup_number(context.numbers, slot),
                )
            })
            .collect()
    }
}

/// beatoraja スキンの `note` 配列インデックスをキーモードに応じて返す。
/// スキン側の並び順: 1P [Key1..KeyN, Scratch], 2P [Key(N+1)..Key(2N), Scratch2]
fn beatoraja_note_index(lane: Lane, key_mode: KeyMode) -> usize {
    match key_mode {
        KeyMode::K5 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            _ => 5, // Scratch
        },
        KeyMode::K7 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            _ => 7, // Scratch
        },
        KeyMode::K6 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            _ => 5,
        },
        KeyMode::K4 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            _ => 3,
        },
        KeyMode::K10 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Scratch => 5,
            Lane::Key8 => 6,
            Lane::Key9 => 7,
            Lane::Key10 => 8,
            Lane::Key11 => 9,
            Lane::Key12 => 10,
            _ => 11, // Scratch2
        },
        KeyMode::K14 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            Lane::Scratch => 7,
            Lane::Key8 => 8,
            Lane::Key9 => 9,
            Lane::Key10 => 10,
            Lane::Key11 => 11,
            Lane::Key12 => 12,
            Lane::Key13 => 13,
            Lane::Key14 => 14,
            _ => 15, // Scratch2
        },
        KeyMode::K9 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            Lane::Key8 => 7,
            Lane::Key9 => 8,
            _ => 8,
        },
        KeyMode::K8 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            Lane::Key8 => 7,
            _ => 0,
        },
    }
}

fn imageset_ref_lane(ref_id: i32) -> Option<Lane> {
    match ref_id {
        500 => Some(Lane::Scratch),
        501 => Some(Lane::Key1),
        502 => Some(Lane::Key2),
        503 => Some(Lane::Key3),
        504 => Some(Lane::Key4),
        505 => Some(Lane::Key5),
        506 => Some(Lane::Key6),
        507 => Some(Lane::Key7),
        _ => None,
    }
}

fn lr2_gauge_image_index(gauge_type: i32) -> i64 {
    // LR2: NORMAL, HARD, HAZARD, EASY, P-ATTACK, G-ATTACK.
    // Types without a legacy sprite use the closest gauge family.
    match gauge_type {
        0 | 1 => 3,
        3 | 4 | 6..=8 => 1,
        5 => 2,
        _ => 0,
    }
}

/// beatoraja `getImageIndexProperty` (IndexType) 相当。image / imageset の ref 専用で、
/// value の `skin_state_number` (ValueType) とは ID が重なっても別解決する。
fn skin_image_index_number(ref_id: i32, state: &SkinDrawState) -> Option<i64> {
    match ref_id {
        SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P => Some(lr2_gauge_image_index(state.gauge_type)),
        SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P => state.opponent_gauge_type.map(lr2_gauge_image_index),
        11 if state.select_screen => Some(state.select_mode_index as i64),
        12 if state.select_screen => Some(state.select_sort_index as i64),
        221 if state.select_screen => Some(state.select_difficulty_filter_index as i64),
        // beatoraja's `gaugetype_1p` image index is state-dependent: MusicSelector
        // reads the configured gauge, while Play/Result read the gauge that is
        // actually active after gauge auto shift has been applied.
        40 if state.select_screen => Some(state.select_gauge_index as i64),
        40 => Some(state.gauge_type.max(0) as i64),
        SKIN_REF_PLAY_GAUGE_TYPE => Some(state.gauge_type.max(0) as i64),
        // Target sprites use both the legacy ref=41 and BUTTON_TARGET=77.
        // They share beatoraja's 11-entry target-list index rather than BMZ's
        // compact target enumeration.
        41 | 77 => Some(state.select_target_index as i64),
        42 => Some(arrange_ref_index(state) as i64),
        43 => Some(arrange_2p_ref_index(state) as i64),
        54 => {
            Some(state.skin_attempt.double_option_index.unwrap_or(state.select_double_option_index)
                as i64)
        }
        55 => Some(state.skin_attempt.hsfix_index.unwrap_or(state.select_hs_fix_index) as i64),
        72 => Some(state.select_bga_index as i64),
        75 => Some(i64::from(state.judge_timing_auto_adjust)),
        78 => Some(
            state.skin_attempt.gauge_auto_shift_index.unwrap_or(state.select_gauge_auto_shift_index)
                as i64,
        ),
        89 if state.select_screen && select_chart_metadata_available(state) => {
            Some(i64::from(state.select_favorite_song))
        }
        90 if state.select_screen && select_chart_metadata_available(state) => {
            Some(i64::from(state.select_favorite_chart))
        }
        90 if state.result_favorite_chart.is_some() => {
            Some(i64::from(state.result_favorite_chart.unwrap_or(false)))
        }
        301..=307 => Some(i64::from(state.assist_flags[(ref_id - 301) as usize])),
        308 => Some(
            state
                .skin_attempt
                .ln_mode_index
                .or(state.result_ln_mode_index)
                .unwrap_or(state.select_ln_mode_index) as i64,
        ),
        330 => Some(i64::from(state.lanecover_enabled)),
        331 => Some(i64::from(state.lift_enabled)),
        332 => Some(i64::from(state.hidden_enabled)),
        340 => Some(
            state.skin_attempt.judge_algorithm_index.unwrap_or(state.select_judge_algorithm_index)
                as i64,
        ),
        341 => Some(
            state
                .skin_attempt
                .bottom_shiftable_gauge_index
                .unwrap_or(state.select_bottom_shiftable_gauge_index) as i64,
        ),
        342 => Some(i64::from(state.hispeed_auto_adjust)),
        344 => Some(extended_arrange_ref_index(state) as i64),
        345 => Some(extended_arrange_2p_ref_index(state) as i64),
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT => {
            score_grade_facts(state).map(|facts| facts.current_index as i64)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEXT => {
            score_grade_facts(state).map(|facts| facts.next_index as i64)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST => {
            score_grade_facts(state).map(|facts| facts.nearest_index as i64)
        }
        SKIN_REF_BMZ_RULE_MODE => Some(state.rule_mode_index as i64),
        SKIN_REF_BMZ_LN_POLICY_SETTING => state.ln_policy_setting_index.map(|index| index as i64),
        SKIN_REF_BMZ_LN_SCORE_POLICY => state.ln_score_policy_index.map(|index| index as i64),
        SKIN_REF_BMZ_SOURCE_KEY_MODE => {
            state.skin_attempt.source_key_mode.map(skin_key_mode_number_i64)
        }
        SKIN_REF_BMZ_SOURCE_LN_PROFILE => state.skin_attempt.source_ln_profile_bits.map(i64::from),
        321..=324 => {
            let slot = (ref_id - 321) as usize;
            Some(state.select_replay_slot_rule_indices[slot])
        }
        350 => Some(i64::from(state.assist_extra_note_depth)),
        351 => Some(state.assist_mine_mode),
        352 => Some(state.assist_scroll_mode),
        353 => Some(state.assist_long_note_mode),
        343 => Some(i64::from(state.guide_se_enabled)),
        400 => Some(i64::from(state.constant_enabled)),
        // beatorajaの7K TO 9K設定。patternは無効時0、有効時1..=6。
        360 => Some(i64::from(state.skin_attempt.seven_to_nine_pattern)),
        361 => Some(i64::from(state.skin_attempt.seven_to_nine_type)),
        370 if state.select_screen || state.result_failed.is_some() => {
            Some(state.select_clear_index)
        }
        371 if state.result_failed.is_some() => result_mybest_clear_index_display(state),
        371 => state.target_clear_index,
        // beatoraja's image/index property table is separate from numeric values:
        // value 390..399 is ranking position, while image 390..399 is clear type.
        390..=399 => {
            ir_ranking_entry(&state.ir_ranking, ref_id - 390).and_then(|entry| entry.clear_index)
        }
        ref_id if random_lane_ref_slot(ref_id).is_some() => {
            skin_random_lane_ref_number(ref_id, state)
        }
        _ => None,
    }
}

fn skin_state_imageset_index(ref_id: i32, state: &SkinDrawState) -> Option<usize> {
    skin_image_index_number(ref_id, state).map(|value| value.max(0) as usize)
}

/// imageset の画像を判定インデックス (0=PGREAT..4=POOR,5=MISS) で選ぶ。
/// 2枚構成 (通常/PGREAT) は PGREAT 判定でのみ2枚目を使う。
fn imageset_image_for_index(
    imageset: &SkinImageSetDef,
    judge_index: Option<usize>,
) -> Option<String> {
    let len = imageset.images.len();
    if len == 0 {
        return None;
    }
    let index = if len == 2 {
        usize::from(judge_index == Some(0))
    } else {
        judge_index.unwrap_or(0).min(len - 1)
    };
    imageset.images.get(index).cloned()
}

pub(crate) fn judge_image_index(judge: &str) -> Option<usize> {
    let judge = judge.trim();
    if judge.starts_with("PGREAT") {
        Some(0)
    } else if judge.starts_with("GREAT") {
        Some(1)
    } else if judge.starts_with("GOOD") {
        Some(2)
    } else if judge.starts_with("BAD") {
        Some(3)
    } else if judge.starts_with("POOR") {
        Some(4)
    } else if judge.starts_with("EMPTY") {
        Some(5)
    } else {
        None
    }
}

pub(crate) fn judge_image_index_for_judge(judge: Judge) -> usize {
    match judge {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor => 4,
        Judge::EmptyPoor => 5,
    }
}

#[path = "skin/animation.rs"]
mod skin_animation;
#[path = "skin/gauge.rs"]
mod skin_gauge;
#[path = "skin/geometry.rs"]
mod skin_geometry;
#[path = "skin/interaction.rs"]
mod skin_interaction;
#[path = "skin/manifest.rs"]
mod skin_manifest;
#[path = "skin/value_helpers/datetime.rs"]
mod skin_value_datetime;
#[path = "skin/value_helpers/format.rs"]
mod skin_value_format;
#[path = "skin/value_helpers/option.rs"]
mod skin_value_option;
#[path = "skin/value_helpers/rank.rs"]
mod skin_value_rank;

use skin_animation::*;
use skin_gauge::*;
use skin_geometry::*;
use skin_interaction::*;
pub use skin_manifest::*;
use skin_value_datetime::*;
use skin_value_format::*;
pub use skin_value_option::*;
use skin_value_rank::*;

#[cfg(test)]
#[path = "skin/tests.rs"]
mod tests;
