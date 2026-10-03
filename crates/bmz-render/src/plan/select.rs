use super::*;

pub(super) fn plan_select(
    snapshot: &SelectSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if skin.document().is_some_and(|document| document.skin_type == 5) {
        let mut commands = Vec::new();
        crate::skin::append_skin_render_items(
            &mut commands,
            &skin.select_document_items_with_dynamic_timers(snapshot, Some(dynamic_timers)),
        );
        // timer/op条件で空になるフレームもスキンの演出として扱う。
        push_exit_hold_indicator(&mut commands, snapshot.exit_hold_progress);
        push_scene_overlays(&mut commands, &snapshot.overlay);
        return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
    }

    let chart_count = snapshot.chart_count;
    let selected_index = snapshot.selected_index;
    let rows = &snapshot.rows;

    let mut commands = Vec::new();
    if snapshot.stage_background {
        push_fullscreen_image(&mut commands, SELECT_STAGE_TEXTURE);
    }
    if snapshot.banner_image {
        push_select_banner_image(&mut commands);
    }
    let text = TextRenderer;
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.06, y: 0.08, width: 0.88, height: 0.08 },
        color: Color::rgb(0.08, 0.11, 0.13),
    });
    text.push_text(
        &mut commands,
        "SELECT",
        BitmapTextStyle { x: 0.08, y: 0.105, cell: 0.009, color: Color::rgb(0.82, 0.9, 0.95) },
    );
    if !snapshot.current_folder.is_empty() {
        text.push_text(
            &mut commands,
            &format!("> {}", snapshot.current_folder),
            BitmapTextStyle {
                x: 0.225,
                y: 0.108,
                cell: 0.006,
                color: Color::rgb(0.55, 0.72, 0.78),
            },
        );
    }
    text.push_text(
        &mut commands,
        &format!("{}", chart_count),
        BitmapTextStyle { x: 0.88, y: 0.112, cell: 0.005, color: Color::rgb(0.62, 0.78, 0.84) },
    );

    // Options bar
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.06, y: 0.163, width: 0.88, height: 0.030 },
        color: Color::rgb(0.05, 0.065, 0.08),
    });
    text.push_text(
        &mut commands,
        &format!(
            "ARRANGE: {}   TARGET: {}   GAUGE: {}   ASSIST: {}   BGA: {}",
            snapshot.arrange, snapshot.target, snapshot.gauge, snapshot.assist, snapshot.bga
        ),
        BitmapTextStyle { x: 0.08, y: 0.170, cell: 0.005, color: Color::rgb(0.72, 0.86, 0.92) },
    );
    push_select_option_panel(&text, &mut commands, snapshot);
    if snapshot.in_settings {
        let hint = if snapshot.settings_editing {
            "UP/DOWN/SCR: change   1/3/5/7 or ENTER: save   2/4/6 or LEFT/ESC: cancel"
        } else {
            "1/3/5/7 or ENTER: edit   2/4/6 or LEFT: back"
        };
        text.push_text(
            &mut commands,
            hint,
            BitmapTextStyle {
                x: 0.08,
                y: 0.198,
                cell: 0.0042,
                color: Color::rgb(0.62, 0.78, 0.72),
            },
        );
    }

    let visible_rows = rows.len().max(1);
    let selected_row_position = select_snapshot_selected_row_position(rows, selected_index);
    for row in 0..visible_rows {
        let snapshot_row = rows.get(row);
        let selected = snapshot_row.map_or(row == 0, |_| row == selected_row_position);
        let is_folder = snapshot_row.map(|r| r.is_folder).unwrap_or(false);
        let in_library = snapshot_row.map(|r| r.in_library).unwrap_or(true);
        let row_y = 0.2 + row as f32 * 0.09;
        let (left_bg, right_bg) = if is_folder {
            if selected {
                (Color::rgb(0.26, 0.21, 0.08), Color::rgb(0.20, 0.16, 0.06))
            } else {
                (Color::rgb(0.09, 0.075, 0.03), Color::rgb(0.07, 0.058, 0.023))
            }
        } else if !in_library {
            if selected {
                (Color::rgb(0.14, 0.14, 0.14), Color::rgb(0.10, 0.10, 0.10))
            } else {
                (Color::rgb(0.05, 0.05, 0.055), Color::rgb(0.04, 0.04, 0.045))
            }
        } else if selected {
            (Color::rgb(0.22, 0.28, 0.31), Color::rgb(0.16, 0.21, 0.23))
        } else {
            (Color::rgb(0.075, 0.09, 0.1), Color::rgb(0.055, 0.065, 0.072))
        };
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.08, y: row_y, width: 0.68, height: 0.065 },
            color: left_bg,
        });
        push_select_title_text(&text, &mut commands, snapshot_row, row_y, selected);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.78, y: row_y, width: 0.14, height: 0.065 },
            color: right_bg,
        });
        push_select_score_text(&text, &mut commands, snapshot_row, row_y, selected);
    }
    text.push_text(
        &mut commands,
        &snapshot.key_hint,
        BitmapTextStyle { x: 0.08, y: 0.86, cell: 0.006, color: Color::rgb(0.88, 0.9, 0.86) },
    );
    text.push_text(
        &mut commands,
        &snapshot.option_hint,
        BitmapTextStyle { x: 0.08, y: 0.895, cell: 0.005, color: Color::rgb(0.58, 0.67, 0.7) },
    );

    push_exit_hold_indicator(&mut commands, snapshot.exit_hold_progress);
    push_scene_overlays(&mut commands, &snapshot.overlay);

    DrawPlan { clear: Color::rgb(0.02, 0.025, 0.03), commands }
}

pub(super) fn push_select_option_panel(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &SelectSnapshot,
) {
    if snapshot.option_panel == 0 {
        return;
    }

    let (title, lines): (&str, Vec<String>) = match snapshot.option_panel {
        1 => (
            "PLAY OPTIONS",
            vec![
                format!("ARRANGE 1P {}", snapshot.arrange),
                format!("ARRANGE 2P {}", snapshot.arrange_2p),
                format!("GAUGE      {}", snapshot.gauge),
                format!("HS-FIX     {}", snapshot.hs_fix),
                format!("DP OPT     {}", snapshot.double_option),
                format!("AUTOPLAY   {}", snapshot.assist),
                "REPLAY   1 / 2 / 3 / 4".to_string(),
            ],
        ),
        2 => (
            "ASSIST OPTIONS",
            [
                "K1 EXPAND JUDGE",
                "K2 CONSTANT",
                "K3 JUDGE AREA",
                "K4 LEGACY NOTE",
                "K5 MARK NOTE",
                "K6 BPM GUIDE",
                "K7 NO MINE",
            ]
            .into_iter()
            .zip(snapshot.assist_flags)
            .map(|(label, enabled)| format!("{label:<17} {}", if enabled { "ON" } else { "OFF" }))
            .collect(),
        ),
        3 => {
            let mut lines = vec![
                format!("GAS      {}", snapshot.gauge_auto_shift),
                format!("GAUGE    {}", snapshot.gauge),
                format!("BGA      {}", snapshot.bga),
            ];
            if snapshot.judge_timing_offset_ms != i32::MIN {
                lines.push(format!("VISUAL   {} ms", snapshot.judge_timing_offset_ms));
            }
            ("DETAIL OPTIONS", lines)
        }
        _ => return,
    };

    let alpha = (snapshot.option_panel_time.0 as f32 / 120_000.0).clamp(0.0, 1.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.57, y: 0.225, width: 0.33, height: 0.12 + lines.len() as f32 * 0.035 },
        color: Color::rgba(0.02, 0.026, 0.032, 0.84 * alpha),
    });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.57, y: 0.225, width: 0.33, height: 0.028 },
        color: Color::rgba(0.11, 0.16, 0.19, 0.9 * alpha),
    });
    text.push_text(
        commands,
        title,
        BitmapTextStyle {
            x: 0.585,
            y: 0.232,
            cell: 0.0048,
            color: Color::rgba(0.74, 0.9, 0.96, alpha),
        },
    );
    for (index, line) in lines.iter().enumerate() {
        let selected = snapshot.option_panel == 3 && line.starts_with("GAS");
        text.push_text(
            commands,
            line,
            BitmapTextStyle {
                x: 0.595,
                y: 0.275 + index as f32 * 0.035,
                cell: 0.005,
                color: if selected {
                    Color::rgba(0.96, 0.9, 0.48, alpha)
                } else {
                    Color::rgba(0.78, 0.86, 0.88, alpha)
                },
            },
        );
    }
}

pub(super) fn select_snapshot_selected_row_position(
    rows: &[SelectRowSnapshot],
    selected_index: u32,
) -> usize {
    let center = rows.len() / 2;
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.index == selected_index)
        .min_by_key(|(index, _)| index.abs_diff(center))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(super) fn push_select_title_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    let is_folder = row.map(|r| r.is_folder).unwrap_or(false);
    let in_library = row.map(|r| r.in_library).unwrap_or(true);
    let title = display_title(row.map(SelectRowSnapshot::display_bar_text).unwrap_or_default());
    let color = if is_folder {
        if selected { Color::rgb(0.98, 0.88, 0.55) } else { Color::rgb(0.62, 0.54, 0.26) }
    } else if !in_library {
        if selected { Color::rgb(0.55, 0.55, 0.55) } else { Color::rgb(0.38, 0.38, 0.40) }
    } else if selected {
        Color::rgb(0.9, 0.96, 0.98)
    } else {
        Color::rgb(0.58, 0.66, 0.68)
    };
    text.push_text(
        commands,
        &title,
        BitmapTextStyle {
            x: 0.1,
            y: row_y + if selected { 0.016 } else { 0.022 },
            cell: if selected { 0.006 } else { 0.005 },
            color,
        },
    );

    if is_folder {
        return;
    }
    let Some(row) = row else {
        return;
    };
    if selected && !row.artist.is_empty() {
        text.push_text(
            commands,
            &display_label(&row.artist, 30),
            BitmapTextStyle {
                x: 0.1,
                y: row_y + 0.041,
                cell: 0.0032,
                color: Color::rgb(0.58, 0.71, 0.73),
            },
        );
    }
    if selected {
        let metadata =
            difficulty_level_label(&row.difficulty_name, &row.play_level, &row.table_level);
        if !metadata.is_empty() {
            text.push_text(
                commands,
                &metadata,
                BitmapTextStyle {
                    x: 0.1,
                    y: row_y + 0.053,
                    cell: 0.0032,
                    color: Color::rgb(0.7, 0.78, 0.7),
                },
            );
        }
    }
}

pub(super) fn push_select_score_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    if row.map(|r| r.is_folder).unwrap_or(false) {
        text.push_text(
            commands,
            ">",
            BitmapTextStyle {
                x: 0.838,
                y: row_y + 0.016,
                cell: 0.010,
                color: if selected {
                    Color::rgb(0.98, 0.85, 0.45)
                } else {
                    Color::rgb(0.52, 0.43, 0.18)
                },
            },
        );
        return;
    }

    let status = row_status_label(row);
    text.push_text(
        commands,
        &status,
        BitmapTextStyle {
            x: 0.805,
            y: row_y + if selected { 0.016 } else { 0.018 },
            cell: if selected { 0.0055 } else { 0.0045 },
            color: if selected {
                Color::rgb(0.74, 0.88, 0.9)
            } else {
                Color::rgb(0.38, 0.46, 0.48)
            },
        },
    );

    let Some(row) = row else {
        return;
    };
    if let Some(ex_score) = row.ex_score {
        text.push_text(
            commands,
            &format!("EX {}", ex_score),
            BitmapTextStyle {
                x: 0.805,
                y: row_y + 0.043,
                cell: if selected { 0.004 } else { 0.0035 },
                color: if selected {
                    Color::rgb(0.86, 0.9, 0.82)
                } else {
                    Color::rgb(0.35, 0.42, 0.38)
                },
            },
        );
    }
}

pub(super) fn row_status_label(row: Option<&SelectRowSnapshot>) -> String {
    let Some(row) = row else {
        return "EMPTY".to_string();
    };
    if row.kind == SelectRowKind::Config {
        return row.play_level.clone();
    }
    if !row.in_library {
        return "NOT OWNED".to_string();
    }
    let clear_type = clear_type_label(&row.clear_type);
    if !clear_type.is_empty() {
        clear_type.to_string()
    } else if !row.table_level.is_empty() {
        row.table_level.clone()
    } else if !row.play_level.is_empty() {
        format!("LV {}", display_label(&row.play_level, 4))
    } else {
        "READY".to_string()
    }
}

pub(super) fn difficulty_level_label(
    difficulty_name: &str,
    play_level: &str,
    table_level: &str,
) -> String {
    let difficulty = display_label(difficulty_name, 12);
    let level_source = if !table_level.is_empty() { table_level } else { play_level };
    let level = display_label(level_source, 8);
    match (difficulty.is_empty(), level.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("DIFFICULTY {difficulty}"),
        (true, false) => format!("LEVEL {level}"),
        (false, false) => format!("DIFFICULTY {difficulty}  LEVEL {level}"),
    }
}

pub(super) fn skin_level_number(label: &str) -> i64 {
    let mut value = 0_i64;
    for digit in label.bytes().filter(u8::is_ascii_digit) {
        value = value.saturating_mul(10).saturating_add((digit - b'0') as i64);
    }
    value
}

pub(super) fn skin_difficulty_code(label: &str) -> i64 {
    let label = label.trim();
    match label {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ if label.eq_ignore_ascii_case("BEGINNER") => 1,
        _ if label.eq_ignore_ascii_case("NORMAL") => 2,
        _ if label.eq_ignore_ascii_case("HYPER") => 3,
        _ if label.eq_ignore_ascii_case("ANOTHER") => 4,
        _ if label.eq_ignore_ascii_case("INSANE") => 5,
        _ => 0,
    }
}

pub(super) fn clear_type_label(clear_type: &str) -> &'static str {
    match clear_type {
        "Failed" => "FAILED",
        "AssistEasy" => "AEASY",
        "LightAssistEasy" => "LAEASY",
        "Easy" => "EASY",
        "Normal" => "NORMAL",
        "Hard" => "HARD",
        "ExHard" => "EXHARD",
        "FullCombo" => "FC",
        "Perfect" => "PERFECT",
        "Max" => "MAX",
        _ => "",
    }
}
