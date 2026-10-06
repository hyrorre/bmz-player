use super::*;

mod default;
mod document;
mod state;

use default::push_default_playfield;
use document::push_document_playfield;
pub(super) use state::build_play_skin_state;
use state::{build_play_skin_text, play_elapsed_ms};

#[derive(Clone, Copy)]
struct PlayfieldLayout<'a> {
    board: Rect,
    lane_width: f32,
    active_lanes: &'a [Lane],
}

pub(super) fn plan_play(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    let text = TextRenderer;
    let skin_manifest = skin.manifest();
    let has_document = skin.document().is_some();
    let mut commands = Vec::with_capacity(play_command_capacity(snapshot, has_document));
    if !has_document {
        push_fallback_bga_background(&mut commands, snapshot);
    }
    let key_mode = snapshot.key_mode;
    let active_lanes = key_mode.active_lanes();
    let active_lane_count = active_lanes.len();
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    let lane_width = board.width / active_lane_count as f32;
    let layout = PlayfieldLayout { board, lane_width, active_lanes };

    let play_elapsed_ms = play_elapsed_ms(snapshot);
    let mut skin_state = build_play_skin_state(snapshot, skin, play_elapsed_ms);
    let skin_input_ms = skin.document().map_or(0, |document| document.input);
    skin_state.start_input_ms = if snapshot.seamless_play_entry {
        crate::skin::skin_start_input_elapsed_ms(play_elapsed_ms, skin_input_ms)
    } else {
        dynamic_timers.start_input_elapsed_ms(play_elapsed_ms, skin_input_ms)
    };
    dynamic_timers.ingest_skin_events(
        skin.document(),
        &snapshot.skin_events,
        key_mode,
        snapshot.time.0,
    );
    let judge_region_count =
        skin.document().map(|document| document.judge_region_count()).unwrap_or(1);
    dynamic_timers.ingest_judge_lane_state(
        &snapshot.recent_judgements,
        judge_region_count,
        snapshot.time.0,
    );
    advance_skin_dynamic_timers(skin, dynamic_timers, &mut skin_state, play_elapsed_ms);
    let skin_text = build_play_skin_text(snapshot);
    // `{"id":"notes"}` マーカーと `timer: 3` (FAILED) で3分割。
    // 描画順: 背面skin → ロング/ノーツ → 前面skin → 暗転/閉店オーバーレイ
    let (behind_notes_items, front_notes_items, failed_overlay_items) = skin
        .static_document_play_items_split_for_state_and_text(
            &skin_state,
            &skin_text,
            &snapshot.judge_graph_density,
            &snapshot.bpm_graph_segments,
        );
    let behind_notes_items = skin.apply_play_skin_global_offset(behind_notes_items, &skin_state);
    append_skin_render_items(&mut commands, &behind_notes_items);

    if !has_document {
        push_default_playfield(&mut commands, snapshot, skin, &skin_state, layout);
    } else {
        push_document_playfield(&mut commands, snapshot, skin, &skin_state, &skin_text, layout);
    }

    // ノーツより前面の skin 要素（レーンカバー・枠・スコア等）をノーツの上に重ねる
    let front_notes_items = skin.apply_play_skin_global_offset(front_notes_items, &skin_state);
    append_skin_render_items(&mut commands, &front_notes_items);

    // 閉店の暗転 (`black` の a:0→255) 等、timer:3 を最前面に描画
    let failed_overlay_items =
        skin.apply_play_skin_global_offset(failed_overlay_items, &skin_state);
    append_skin_render_items(&mut commands, &failed_overlay_items);

    if !has_document {
        push_combo_panel(skin_manifest, &mut commands, snapshot.combo);
        push_default_play_skin(skin, &mut commands, snapshot);
        push_play_text(&text, &mut commands, snapshot);
        push_lane_text(&text, &mut commands, board, lane_width, active_lanes);
        push_judgement_history(&text, &mut commands, snapshot);
        // READY/GO オーバーレイはデフォルトスキン専用。
        // JSON skin 等は skin 側の演出を使うため描画しない。
        push_start_overlay(&text, &mut commands, snapshot);
        push_default_failed_overlay(&text, &mut commands, snapshot);
    }
    if !skin.has_timer_destination(2) {
        push_default_play_fadeout_overlay(&mut commands, snapshot);
    }
    push_scene_overlays(&mut commands, &snapshot.overlay);

    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

pub(super) fn judge_starts_bomb(judge_index: Option<usize>, judge_timer_limit: usize) -> bool {
    judge_index.is_some_and(|judge| judge <= judge_timer_limit)
}

pub(super) fn play_command_capacity(snapshot: &RenderSnapshot, has_document: bool) -> usize {
    let visible_note_count: usize = snapshot.visible_notes.iter().map(Vec::len).sum();
    let visible_mine_count: usize = snapshot.visible_mines.iter().map(Vec::len).sum();
    let long_note_command_count = snapshot.visible_long_notes.len().saturating_mul(3);
    let skin_command_floor: usize = if has_document { 192 } else { 96 };
    skin_command_floor
        .saturating_add(snapshot.bar_lines.len())
        .saturating_add(visible_note_count)
        .saturating_add(visible_mine_count)
        .saturating_add(long_note_command_count)
}
