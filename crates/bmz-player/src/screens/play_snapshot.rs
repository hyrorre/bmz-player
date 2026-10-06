use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bmz_chart::model::LongNoteMode;
use bmz_chart::model::{
    BarLine, BgaArgbEvent, BgaAssetId, BgaEvent, BgaEventKind, BgaOpacityEvent, NoteEvent,
    NoteKind, PlayableChart, TimingEventKind,
};
use bmz_chart::timing::{TICKS_PER_MEASURE, TimingMap};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::{ChartTick, TimeUs};
use bmz_gameplay::gauge::gauge_total_for_chart;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_gameplay::score::scored_note_count;
use bmz_gameplay::session::GameSession;
use bmz_render::chart_graph::{
    build_bpm_graph_segments, build_judge_graph_density, compute_adjusted_cover_progress,
    compute_adjusted_rate, rm_skin_fs_threshold_ms,
};
use bmz_render::plan::CHART_BGA_TEXTURE_BASE;
use bmz_render::skin_offset::{SkinOffsetValue, SkinOffsetValues};
use bmz_render::snapshot::{
    DisplayBgaFrame, DisplayInput, DisplayJudgeCounts, DisplayJudgement, LongBodyState,
    NoteVisualKind, OpponentRenderSnapshot, OverlaySnapshot, RenderSnapshot, ResultGaugeGraphPoint,
    VisibleBarLine, VisibleLongNote, VisibleMine, VisibleNote,
};

pub(crate) const BEATORAJA_DURATION_BPM_FACTOR_MS: f32 = 240_000.0;
const SCRATCH_ANGLE_OFFSET_1P: i32 = 1;
const SCRATCH_ANGLE_OFFSET_2P: i32 = 2;
const SCRATCH_ANGLE_PERIOD_MS: i64 = 2_160;
const SCRATCH_ANGLE_DEGREES_DIVISOR: i64 = 6;
const BGA_EVENT_KIND_COUNT: usize = 4;
pub type BgaFrameCatalog = HashMap<BgaAssetId, DisplayBgaFrame>;

mod bga;
mod build;
mod cache;
mod display;
mod projection;
mod scroll;
mod state;
mod visuals;
pub(crate) use projection::{PlayfieldProjection, ProjectionClock};

pub use bga::{bga_texture_id, display_bga_frame, display_video_bga_frame};
pub(crate) use bga::{display_duration_ms_for_bpm_hispeed, hispeed_for_green_number_values};
pub(crate) use build::build_render_state_with_target_and_bga_frames_cached;
pub use build::{
    apply_prepared_chart_to_render_snapshot, build_render_snapshot,
    build_render_snapshot_with_bga_frames, build_render_snapshot_with_target_and_bga_frames,
    build_render_snapshot_with_target_and_bga_frames_cached, update_render_snapshot_play_options,
};
pub use cache::PlayRenderSnapshotCache;
pub use display::apply_fast_slow_display_filter;
pub(crate) use scroll::{current_scroll_multiplier, scroll_multiplier_at_tick};
pub use visuals::{
    refresh_pending_play_input_visuals, refresh_play_skin_visuals,
    refresh_play_skin_visuals_with_input_elapsed, skin_visual_time,
};

pub(crate) use bga::effective_bpm_for_playback_rate;
use bga::*;
use cache::*;
use display::*;
use scroll::*;
use state::*;
use visuals::*;

#[cfg(test)]
#[path = "play_snapshot/tests.rs"]
mod tests;
