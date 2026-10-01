use super::*;
use crate::scene::detail_options::{DETAIL_OPTION_ROWS, DetailOptionsSnapshot};
use crate::skin::{SkinClickHit, SkinClickTarget};
use bmz_skin_document::*;

fn label(
    commands: &mut Vec<DrawCommand>,
    value: &str,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    color: Color,
) {
    commands.push(DrawCommand::Text {
        origin: Point { x, y },
        text: value.to_string(),
        style: TextStyle {
            font_id: None,
            size,
            bitmap_size: None,
            color,
            layer: TextLayer::Ui,
            align: TextAlign::Left,
            max_width: width,
            overflow: TextOverflow::Shrink,
            wrapping: false,
            outline: None,
            shadow: None,
        },
        caret: None,
        post_scale: Point { x: 1.0, y: 1.0 },
    });
}

pub(super) fn push_detail_options(commands: &mut Vec<DrawCommand>, panel: &DetailOptionsSnapshot) {
    // Opaque full-canvas modal: old skins retain their timers and API state,
    // while no old panel or underlying song list is visible through this one.
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        color: Color::rgb(0.035, 0.055, 0.075),
    });
    let white = Color::rgb(0.93, 0.96, 0.98);
    let muted = Color::rgb(0.64, 0.75, 0.82);
    let accent = Color::rgb(0.42, 0.91, 0.79);
    label(commands, &panel.title, 0.06, 0.045, 0.78, 0.033, white);
    label(commands, &panel.position, 0.86, 0.052, 0.10, 0.023, muted);
    label(commands, &panel.scope_label, 0.06, 0.10, 0.35, 0.025, accent);
    if let Some(selected) = panel.selected() {
        label(commands, &selected.category, 0.46, 0.10, 0.48, 0.025, muted);
    }
    for slot in 0..DETAIL_OPTION_ROWS {
        let Some(row) = panel.row(slot) else {
            continue;
        };
        let rect = column_rect(slot);
        let selected = panel.viewport_start + slot == panel.cursor;
        commands.push(DrawCommand::Rect {
            rect,
            color: if selected {
                Color::rgb(0.11, 0.27, 0.29)
            } else {
                Color::rgb(0.07, 0.10, 0.13)
            },
        });
        label(
            commands,
            if selected { "▼" } else { "" },
            rect.x + 0.054,
            0.145,
            0.025,
            0.024,
            accent,
        );
        label(commands, &row.label, rect.x + 0.007, 0.204, 0.11, 0.022, white);
        if row.value_index < 0 {
            label(commands, &row.value_label, rect.x + 0.007, 0.244, 0.11, 0.021, accent);
        }
        for (index, choice) in row.choices.iter().enumerate() {
            let cell = choice_rect(slot, index);
            let current = row.value_index >= 0 && row.value == choice.value;
            commands.push(DrawCommand::Rect {
                rect: cell,
                color: if current {
                    Color::rgb(0.16, 0.38, 0.37)
                } else {
                    Color::rgb(0.075, 0.12, 0.145)
                },
            });
            label(
                commands,
                if current { "●" } else { "" },
                cell.x + 0.003,
                cell.y + 0.006,
                0.012,
                0.019,
                accent,
            );
            label(
                commands,
                &choice.label,
                cell.x + 0.016,
                cell.y + 0.006,
                cell.width - 0.019,
                0.022,
                if current { white } else { muted },
            );
        }
        label(commands, &row.status, rect.x + 0.007, 0.61, 0.11, 0.019, muted);
    }
    if let Some(row) = panel.selected() {
        label(commands, &row.auxiliary, 0.06, 0.66, 0.88, 0.023, accent);
        label(commands, &row.description, 0.06, 0.715, 0.88, 0.024, white);
        label(commands, &row.reason, 0.06, 0.77, 0.88, 0.022, muted);
    }
    for (id, caption, x) in [
        (SKIN_EVENT_DETAIL_OPTIONS_PREVIOUS, "←", 0.06),
        (SKIN_EVENT_DETAIL_OPTIONS_NEXT, "→", 0.19),
        (SKIN_EVENT_DETAIL_OPTIONS_DECREASE, "−", 0.68),
        (SKIN_EVENT_DETAIL_OPTIONS_INCREASE, "+", 0.81),
    ] {
        let rect = button_rect(id).expect("button");
        commands.push(DrawCommand::Rect { rect, color: Color::rgb(0.12, 0.20, 0.25) });
        label(commands, caption, x + 0.044, 0.856, 0.06, 0.030, white);
    }
    label(commands, &panel.guide, 0.06, 0.94, 0.88, 0.020, muted);
}

fn column_rect(slot: usize) -> Rect {
    Rect { x: 0.05 + slot as f32 * 0.129, y: 0.18, width: 0.124, height: 0.46 }
}

fn choice_rect(slot: usize, index: usize) -> Rect {
    Rect {
        x: column_rect(slot).x + 0.006,
        y: 0.28 + index as f32 * 0.041,
        width: 0.112,
        height: 0.033,
    }
}

fn button_rect(id: i32) -> Option<Rect> {
    let x = match id {
        SKIN_EVENT_DETAIL_OPTIONS_PREVIOUS => 0.06,
        SKIN_EVENT_DETAIL_OPTIONS_NEXT => 0.19,
        SKIN_EVENT_DETAIL_OPTIONS_DECREASE => 0.68,
        SKIN_EVENT_DETAIL_OPTIONS_INCREASE => 0.81,
        _ => return None,
    };
    Some(Rect { x, y: 0.84, width: 0.12, height: 0.065 })
}

pub(crate) fn detail_options_click_hit(
    panel: &DetailOptionsSnapshot,
    x: f32,
    y: f32,
) -> Option<SkinClickHit> {
    for id in SKIN_EVENT_DETAIL_OPTIONS_PREVIOUS..=SKIN_EVENT_DETAIL_OPTIONS_INCREASE {
        let rect = button_rect(id)?;
        if contains(rect, x, y) {
            return Some(SkinClickHit {
                target: SkinClickTarget::Event { event_id: id, click: 0 },
                rect,
            });
        }
    }
    for slot in 0..DETAIL_OPTION_ROWS {
        let Some(row) = panel.row(slot) else {
            continue;
        };
        if row.editable {
            for index in 0..row.choices.len().min(SKIN_DETAIL_OPTIONS_CHOICES) {
                let rect = choice_rect(slot, index);
                if contains(rect, x, y) {
                    return Some(SkinClickHit {
                        target: SkinClickTarget::Event {
                            event_id: SKIN_DETAIL_OPTIONS_CHOICE_BASE
                                + slot as i32 * SKIN_DETAIL_OPTIONS_CHOICE_ITEM_STRIDE
                                + index as i32 * SKIN_DETAIL_OPTIONS_CHOICE_STRIDE,
                            click: 0,
                        },
                        rect,
                    });
                }
            }
        }
        let rect = column_rect(slot);
        if contains(rect, x, y) {
            return Some(SkinClickHit {
                target: SkinClickTarget::Event {
                    event_id: SKIN_EVENT_DETAIL_OPTIONS_ROW_BASE + slot as i32,
                    click: 0,
                },
                rect,
            });
        }
    }
    None
}

fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
}
