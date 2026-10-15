use super::*;

pub(super) fn push_default_play_fadeout_overlay(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let Some(elapsed_ms) = snapshot.fadeout_elapsed_ms else {
        return;
    };
    let alpha = (elapsed_ms as f32 / crate::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS as f32)
        .clamp(0.0, 1.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        color: Color::rgba(0.0, 0.0, 0.0, alpha),
    });
}

pub(super) fn push_start_overlay(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let Some(label) = start_overlay_label(snapshot.time) else {
        return;
    };
    let cell = 0.018;
    text.push_text(
        commands,
        label,
        BitmapTextStyle {
            x: 0.5 - label_width(label, cell) / 2.0,
            y: 0.385,
            cell,
            color: if label == "READY" {
                Color::rgb(0.74, 0.88, 0.9)
            } else {
                Color::rgb(0.96, 0.92, 0.54)
            },
        },
    );
}

pub(super) fn push_default_failed_overlay(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let Some(elapsed_ms) = snapshot.failed_elapsed_ms else {
        return;
    };
    let alpha = (elapsed_ms as f32 / 700.0).clamp(0.0, 0.82);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        color: Color::rgba(0.0, 0.0, 0.0, alpha),
    });
    let label = "FAILED";
    let cell = 0.02;
    text.push_text(
        commands,
        label,
        BitmapTextStyle {
            x: 0.5 - label_width(label, cell) / 2.0,
            y: 0.43,
            cell,
            color: Color::rgba(1.0, 0.24, 0.28, alpha.clamp(0.35, 1.0)),
        },
    );
}

pub(super) fn push_judge_count_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.05, y: 0.36, width: 0.11, height: 0.235 },
        color: Color::rgb(0.032, 0.036, 0.04),
    });

    let rows = [
        ("PG", snapshot.judge_counts.pgreat, Color::rgb(0.66, 0.92, 0.98)),
        ("GR", snapshot.judge_counts.great, Color::rgb(0.66, 0.92, 0.98)),
        ("GD", snapshot.judge_counts.good, Color::rgb(0.84, 0.88, 0.48)),
        ("BD", snapshot.judge_counts.bad, Color::rgb(0.94, 0.56, 0.36)),
        ("PR", snapshot.judge_counts.poor, Color::rgb(0.96, 0.4, 0.44)),
        ("EP", snapshot.judge_counts.empty_poor, Color::rgb(0.96, 0.4, 0.44)),
    ];

    for (index, (label, count, color)) in rows.into_iter().enumerate() {
        text.push_text(
            commands,
            &format!("{label} {count}"),
            BitmapTextStyle { x: 0.065, y: 0.382 + index as f32 * 0.032, cell: 0.004, color },
        );
    }
}

pub(super) fn push_lane_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lane_width: f32,
    active_lanes: &[Lane],
) {
    for (display_index, &lane) in active_lanes.iter().enumerate() {
        let center_x = board.x + display_index as f32 * lane_width + lane_width / 2.0;
        let label = lane_label(lane);
        text.push_text(
            commands,
            label,
            BitmapTextStyle {
                x: center_x - label_width(label, 0.0035) / 2.0,
                y: board.y + 0.018,
                cell: 0.0035,
                color: Color::rgb(0.45, 0.55, 0.58),
            },
        );
        let key = lane_key_label(lane);
        text.push_text(
            commands,
            key,
            BitmapTextStyle {
                x: center_x - label_width(key, 0.004) / 2.0,
                y: board.y + board.height * 0.9,
                cell: 0.004,
                color: Color::rgb(0.78, 0.86, 0.84),
            },
        );
    }
}

pub(super) fn push_play_status_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.05, y: 0.08, width: 0.11, height: 0.285 },
        color: Color::rgb(0.035, 0.04, 0.044),
    });
    text.push_text(
        commands,
        &format!("EX {}", snapshot.ex_score),
        BitmapTextStyle { x: 0.065, y: 0.105, cell: 0.0055, color: Color::rgb(0.82, 0.9, 0.92) },
    );
    text.push_text(
        commands,
        &format!("MAX {}", snapshot.max_combo),
        BitmapTextStyle { x: 0.065, y: 0.15, cell: 0.0055, color: Color::rgb(0.82, 0.9, 0.92) },
    );
    text.push_text(
        commands,
        &format!("NOTE {}", snapshot.past_notes.min(snapshot.total_notes)),
        BitmapTextStyle { x: 0.065, y: 0.195, cell: 0.005, color: Color::rgb(0.68, 0.78, 0.8) },
    );
    text.push_text(
        commands,
        &format!("/{}", snapshot.total_notes),
        BitmapTextStyle { x: 0.065, y: 0.235, cell: 0.005, color: Color::rgb(0.68, 0.78, 0.8) },
    );
    text.push_text(
        commands,
        &format_time(snapshot.time),
        BitmapTextStyle { x: 0.065, y: 0.28, cell: 0.0045, color: Color::rgb(0.48, 0.62, 0.66) },
    );
    text.push_text(
        commands,
        &format!("HS {:.2}", snapshot.hispeed_display.unwrap_or(snapshot.hispeed)),
        BitmapTextStyle { x: 0.065, y: 0.32, cell: 0.0045, color: Color::rgb(0.72, 0.82, 0.8) },
    );
}

pub(super) fn push_judgement_history(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    if snapshot.recent_judgements.is_empty() {
        return;
    }

    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.885, y: 0.17, width: 0.09, height: 0.19 },
        color: Color::rgb(0.03, 0.035, 0.038),
    });
    text.push_text(
        commands,
        "JUDGE",
        BitmapTextStyle { x: 0.897, y: 0.188, cell: 0.004, color: Color::rgb(0.5, 0.62, 0.64) },
    );

    for (index, judgement) in snapshot.recent_judgements.iter().rev().take(4).enumerate() {
        let y = 0.225 + index as f32 * 0.032;
        text.push_text(
            commands,
            &judgement_history_label(judgement),
            BitmapTextStyle {
                x: 0.897,
                y,
                cell: 0.0038,
                color: judgement_history_color(&judgement.text),
            },
        );
    }
}
