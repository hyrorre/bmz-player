//! egui overlay for practice configuration (pre-play).

use egui::{Context, RichText};

use crate::i18n::Localizer;
use crate::screens::practice::{
    PracticeCursorAction, PracticeGaugeType, PracticeGraphType, PracticeProperty,
    PracticeRuleContext,
};
use crate::select_options::ArrangeOption;
use bmz_gameplay::gauge::GaugeProperty;
use bmz_render::snapshot::ResultGraphSnapshot;

pub struct PracticePanelContext<'a> {
    pub property: &'a mut PracticeProperty,
    pub rules: PracticeRuleContext,
    pub graph: &'a ResultGraphSnapshot,
    pub graph_start_time_ms: u32,
    pub is_double: bool,
    pub cursor: &'a mut usize,
    pub chart_title: &'a str,
    pub media_ready: bool,
    /// Practice の終了演出中は表示だけを維持し、egui 操作を受け付けない。
    pub input_enabled: bool,
    pub max_end_time_ms: u32,
    /// Surface 左上原点の正規化座標。beatoraja skin の practice destination 由来。
    pub default_position: Option<(f32, f32)>,
}

pub struct PracticePanelOutput {
    pub start_play: bool,
    pub leave: bool,
}

pub fn build_practice_panel(
    ctx: &Context,
    practice: &mut PracticePanelContext<'_>,
    text: Localizer,
) -> PracticePanelOutput {
    let mut start_play = false;
    let mut leave = false;
    let cursor_count = crate::screens::practice::practice_cursor_count(practice.is_double);
    if practice.input_enabled && ctx.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
        *practice.cursor = (*practice.cursor + 1) % cursor_count;
    }
    if practice.input_enabled && ctx.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
        *practice.cursor = (*practice.cursor + cursor_count - 1) % cursor_count;
    }
    let decrement =
        practice.input_enabled && ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft));
    let increment =
        practice.input_enabled && ctx.input(|input| input.key_pressed(egui::Key::ArrowRight));
    if decrement || increment {
        match crate::screens::practice::apply_practice_cursor_horizontal(
            practice.property,
            *practice.cursor,
            practice.is_double,
            increment,
            practice.max_end_time_ms,
            practice.rules,
        ) {
            PracticeCursorAction::None => {}
            PracticeCursorAction::Start => start_play = true,
            PracticeCursorAction::Leave => leave = true,
        }
    }

    let mut window = egui::Window::new(text.text("practice-title"))
        .id(egui::Id::new("practice_config_panel"))
        .order(egui::Order::Foreground)
        .movable(practice.input_enabled)
        .resizable(false)
        .collapsible(false)
        .default_width(360.0)
        .frame(
            egui::Frame::window(ctx.global_style().as_ref())
                .fill(egui::Color32::from_rgba_unmultiplied(16, 20, 32, 230)),
        );
    let screen = ctx.content_rect();
    let default_position = practice
        .default_position
        .map(|(x, y)| {
            egui::pos2(screen.left() + x * screen.width(), screen.top() + y * screen.height())
        })
        .unwrap_or_else(|| egui::pos2(screen.left() + 12.0, screen.top() + 12.0));
    window = window.default_pos(default_position);
    window.show(ctx, |ui| {
        ui.set_min_width(360.0);
        if !practice.input_enabled {
            ui.disable();
        }
        ui.label(RichText::new(practice.chart_title).weak());
        ui.separator();

        practice_field_label(ui, practice.cursor, 0, text.text("practice-start-time"));

        ui.horizontal(|ui| {
            time_ms_field(
                ui,
                &mut practice.property.start_time_ms,
                practice.max_end_time_ms.saturating_sub(3000),
            );
        });
        practice_field_label(ui, practice.cursor, 1, text.text("practice-end-time"));
        ui.horizontal(|ui| {
            time_ms_field(ui, &mut practice.property.end_time_ms, practice.max_end_time_ms);
        });

        practice_field_label(ui, practice.cursor, 2, text.text("practice-gauge"));
        ui.horizontal(|ui| {
            let mut selected_gauge = practice.property.gauge;
            egui::ComboBox::from_id_salt("practice_gauge")
                .selected_text(gauge_label(text, practice.property.gauge))
                .show_ui(ui, |ui| {
                    for gauge in practice_gauges() {
                        ui.selectable_value(&mut selected_gauge, gauge, gauge_label(text, gauge));
                    }
                });
            practice.rules.set_gauge(practice.property, selected_gauge);
        });
        practice_field_label(ui, practice.cursor, 3, text.text("practice-gauge-category"));
        ui.horizontal(|ui| {
            if practice.rules.field_is_fixed(3) {
                ui.label(RichText::new(dx_rule_label(practice.rules)).weak());
            } else {
                let mut category = practice
                    .property
                    .gauge_category
                    .unwrap_or_else(|| GaugeProperty::from_keymode(practice.rules.key_mode));
                let previous_category = category;
                egui::ComboBox::from_id_salt("practice_gauge_category")
                    .selected_text(gauge_category_label(category))
                    .show_ui(ui, |ui| {
                        for value in practice_gauge_categories() {
                            ui.selectable_value(&mut category, value, gauge_category_label(value));
                        }
                    });
                if category != previous_category {
                    practice.rules.set_gauge_category(practice.property, category);
                }
            }
        });
        practice_field_label(ui, practice.cursor, 4, text.text("practice-gauge-percent"));
        ui.horizontal(|ui| {
            let max = practice.rules.gauge_bounds(practice.property).max;
            ui.add(
                egui::DragValue::new(&mut practice.property.start_gauge).range(1..=max).speed(0.2),
            );
        });
        practice_field_label(ui, practice.cursor, 5, text.text("practice-judge-rank"));
        ui.horizontal(|ui| {
            if practice.rules.field_is_fixed(5) {
                ui.label(RichText::new(dx_rule_label(practice.rules)).weak());
            } else {
                ui.add(
                    egui::DragValue::new(&mut practice.property.judgerank)
                        .range(1..=400)
                        .speed(0.5),
                );
            }
        });
        practice_field_label(ui, practice.cursor, 6, text.text("practice-total"));
        if practice.rules.field_is_fixed(6) {
            ui.label(RichText::new("AUTO (DX MODE)").weak());
        } else if let Some(total) = practice.property.total.as_mut() {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(total).range(10.0..=5000.0).speed(1.0));
            });
        }
        practice_field_label(ui, practice.cursor, 7, text.text("practice-frequency"));
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut practice.property.playback_rate_percent)
                    .range(50..=200)
                    .suffix(" %"),
            );
        });
        practice_field_label(ui, practice.cursor, 8, text.text("practice-graph-type"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("practice_graph_type")
                .selected_text(graph_type_label(practice.property.graph_type))
                .show_ui(ui, |ui| {
                    for graph_type in [
                        PracticeGraphType::NoteType,
                        PracticeGraphType::Judge,
                        PracticeGraphType::EarlyLate,
                    ] {
                        ui.selectable_value(
                            &mut practice.property.graph_type,
                            graph_type,
                            graph_type_label(graph_type),
                        );
                    }
                });
        });
        practice_field_label(ui, practice.cursor, 9, text.text("practice-arrange"));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("practice_arrange")
                .selected_text(arrange_label(text, practice.property.arrange))
                .show_ui(ui, |ui| {
                    for arrange in ArrangeOption::VALUES {
                        ui.selectable_value(
                            &mut practice.property.arrange,
                            arrange,
                            arrange_label(text, arrange),
                        );
                    }
                });
        });
        if practice.is_double {
            practice_field_label(ui, practice.cursor, 10, text.text("practice-arrange-2p"));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("practice_arrange_2p")
                    .selected_text(arrange_label(text, practice.property.arrange_2p))
                    .show_ui(ui, |ui| {
                        for arrange in ArrangeOption::VALUES {
                            ui.selectable_value(
                                &mut practice.property.arrange_2p,
                                arrange,
                                arrange_label(text, arrange),
                            );
                        }
                    });
            });
            practice_field_label(ui, practice.cursor, 11, text.text("practice-dp-option"));
            ui.checkbox(&mut practice.property.dp_flip, "FLIP");
        }

        *practice.cursor = (*practice.cursor).min(cursor_count - 1);
        draw_practice_graph(ui, practice);

        ui.separator();
        if practice.media_ready {
            ui.colored_label(egui::Color32::LIGHT_GREEN, text.text("practice-ready-hint"));
        } else {
            ui.colored_label(egui::Color32::YELLOW, text.text("practice-media-loading"));
        }

        ui.horizontal(|ui| {
            let start_cursor = crate::screens::practice::practice_start_cursor(practice.is_double);
            let start_button = egui::Button::new(text.text("practice-start-play"))
                .selected(*practice.cursor == start_cursor);
            if ui.add_enabled(practice.media_ready, start_button).clicked() {
                *practice.cursor = start_cursor;
                start_play = true;
            }
            let leave_cursor = crate::screens::practice::practice_leave_cursor(practice.is_double);
            let leave_button = egui::Button::new(text.text("practice-back-to-select"))
                .selected(*practice.cursor == leave_cursor);
            if ui.add(leave_button).clicked() {
                *practice.cursor = leave_cursor;
                leave = true;
            }
        });
    });

    if practice.input_enabled
        && ctx.input(|input| input.key_pressed(egui::Key::Enter))
        && practice.media_ready
    {
        start_play = true;
    }
    if practice.input_enabled && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        leave = true;
    }

    PracticePanelOutput { start_play, leave }
}

fn time_ms_field(ui: &mut egui::Ui, value: &mut u32, max_ms: u32) {
    let mut ms = i64::from(*value);
    if ui.add(egui::DragValue::new(&mut ms).range(0..=i64::from(max_ms)).speed(50.0)).changed() {
        *value = u32::try_from(ms).unwrap_or(max_ms);
    }
    ui.label(format_time_ms(*value));
}

fn format_time_ms(ms: u32) -> String {
    let minutes = ms / 60_000;
    let seconds = (ms / 1000) % 60;
    let tenths = (ms / 100) % 10;
    format!("{minutes:02}:{seconds:02}.{tenths}")
}

fn dx_rule_label(rules: PracticeRuleContext) -> &'static str {
    if rules.key_mode == bmz_core::lane::KeyMode::K9 { "POP (DX MODE)" } else { "IIDX (DX MODE)" }
}

fn practice_gauges() -> [PracticeGaugeType; 9] {
    PracticeGaugeType::VALUES
}

fn gauge_label(text: Localizer, gauge: PracticeGaugeType) -> String {
    text.text(match gauge {
        PracticeGaugeType::AssistEasy => "practice-gauge-assist-easy",
        PracticeGaugeType::Easy => "practice-gauge-easy",
        PracticeGaugeType::Normal => "practice-gauge-normal",
        PracticeGaugeType::Hard => "practice-gauge-hard",
        PracticeGaugeType::ExHard => "practice-gauge-ex-hard",
        PracticeGaugeType::Hazard => "practice-gauge-hazard",
        PracticeGaugeType::Class => "practice-gauge-class",
        PracticeGaugeType::ExClass => "practice-gauge-ex-class",
        PracticeGaugeType::ExHardClass => "practice-gauge-ex-hard-class",
        PracticeGaugeType::AutoShift => "practice-gauge-auto-shift",
    })
}

fn practice_gauge_categories() -> [GaugeProperty; 4] {
    [GaugeProperty::FiveKeys, GaugeProperty::SevenKeys, GaugeProperty::Pms, GaugeProperty::Lr2]
}

fn gauge_category_label(category: GaugeProperty) -> &'static str {
    match category {
        GaugeProperty::FiveKeys => "5KEYS",
        GaugeProperty::SevenKeys => "7KEYS",
        GaugeProperty::Pms => "PMS",
        GaugeProperty::Keyboard => "KEYBOARD",
        GaugeProperty::Lr2 => "LR2",
    }
}

fn graph_type_label(graph_type: PracticeGraphType) -> &'static str {
    match graph_type {
        PracticeGraphType::NoteType => "NOTETYPE",
        PracticeGraphType::Judge => "JUDGE",
        PracticeGraphType::EarlyLate => "EARLYLATE",
    }
}

fn practice_field_label(ui: &mut egui::Ui, cursor: &mut usize, index: usize, label: String) {
    if ui.selectable_label(*cursor == index, label).clicked() {
        *cursor = index;
    }
}

fn draw_practice_graph(ui: &mut egui::Ui, practice: &PracticePanelContext<'_>) {
    let desired = egui::vec2(ui.available_width().max(320.0), 120.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(8, 12, 20));
    let buckets: Vec<[u32; 10]> = match practice.property.graph_type {
        PracticeGraphType::NoteType => practice
            .graph
            .note_graph_buckets
            .iter()
            .map(|bucket| {
                let mut values = [0; 10];
                values[..7].copy_from_slice(&bucket.values);
                values
            })
            .collect(),
        PracticeGraphType::Judge => practice
            .graph
            .judge_graph_buckets
            .iter()
            .map(|bucket| {
                let mut values = [0; 10];
                values[..6].copy_from_slice(&bucket.values);
                values
            })
            .collect(),
        PracticeGraphType::EarlyLate => {
            practice.graph.early_late_graph_buckets.iter().map(|bucket| bucket.values).collect()
        }
    };
    if buckets.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "NO DATA",
            egui::FontId::proportional(14.0),
            egui::Color32::GRAY,
        );
        return;
    }
    let start = (practice.property.start_time_ms.saturating_sub(practice.graph_start_time_ms)
        / 1000) as usize;
    let end = (practice
        .property
        .end_time_ms
        .saturating_sub(practice.graph_start_time_ms)
        .saturating_add(999)
        / 1000) as usize;
    let visible =
        &buckets[start.min(buckets.len())..end.min(buckets.len()).max(start.min(buckets.len()))];
    if visible.is_empty() {
        return;
    }
    let max_total =
        visible.iter().map(|bucket| bucket.iter().copied().sum::<u32>()).max().unwrap_or(1).max(1)
            as f32;
    let colors = practice_graph_colors(practice.property.graph_type);
    let width = rect.width() / visible.len() as f32;
    for (index, bucket) in visible.iter().enumerate() {
        let mut bottom = rect.bottom();
        for (state, count) in bucket.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            let height = rect.height() * *count as f32 / max_total;
            let bar = egui::Rect::from_min_max(
                egui::pos2(rect.left() + index as f32 * width, bottom - height),
                egui::pos2(rect.left() + (index + 1) as f32 * width, bottom),
            );
            painter.rect_filled(bar, 0.0, colors[state]);
            bottom -= height;
        }
    }
}

fn practice_graph_colors(graph_type: PracticeGraphType) -> [egui::Color32; 10] {
    if graph_type == PracticeGraphType::NoteType {
        // beatoraja SkinNoteDistributionGraph.JGRAPH[TYPE_NORMAL] と同じ系列順。
        return [
            egui::Color32::from_rgb(0x44, 0xff, 0x44),
            egui::Color32::from_rgb(0x22, 0x88, 0x22),
            egui::Color32::from_rgb(0xff, 0x44, 0x44),
            egui::Color32::from_rgb(0x44, 0x44, 0xff),
            egui::Color32::from_rgb(0x22, 0x22, 0x88),
            egui::Color32::from_rgb(0xcc, 0xcc, 0xcc),
            egui::Color32::from_rgb(0x88, 0x00, 0x00),
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
        ];
    }
    [
        egui::Color32::from_rgb(90, 100, 120),
        egui::Color32::from_rgb(80, 210, 255),
        egui::Color32::from_rgb(255, 220, 80),
        egui::Color32::from_rgb(120, 220, 120),
        egui::Color32::from_rgb(255, 150, 70),
        egui::Color32::from_rgb(245, 80, 90),
        egui::Color32::from_rgb(210, 70, 210),
        egui::Color32::from_rgb(70, 160, 255),
        egui::Color32::from_rgb(255, 100, 180),
        egui::Color32::from_rgb(150, 80, 220),
    ]
}

fn arrange_label(text: Localizer, arrange: ArrangeOption) -> String {
    text.text(match arrange {
        ArrangeOption::Normal => "practice-arrange-normal",
        ArrangeOption::Mirror => "practice-arrange-mirror",
        ArrangeOption::Random => "practice-arrange-random",
        ArrangeOption::RRandom => "practice-arrange-r-random",
        ArrangeOption::SRandom => "practice-arrange-s-random",
        ArrangeOption::Spiral => "practice-arrange-spiral",
        ArrangeOption::HRandom => "practice-arrange-h-random",
        ArrangeOption::AllScratch => "practice-arrange-all-scratch",
        ArrangeOption::RandomEx => "practice-arrange-random-ex",
        ArrangeOption::SRandomEx => "practice-arrange-s-random-ex",
        ArrangeOption::FRandom => "practice-arrange-f-random",
        ArrangeOption::MFRandom => "practice-arrange-mf-random",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::AppLocale;

    #[test]
    fn practice_labels_resolve_for_every_locale() {
        let keys = [
            "practice-title",
            "practice-start-time",
            "practice-end-time",
            "practice-gauge",
            "practice-gauge-percent",
            "practice-judge-rank",
            "practice-arrange",
            "practice-total",
            "practice-ready-hint",
            "practice-media-loading",
            "practice-start-play",
            "practice-back-to-select",
        ];
        for locale in AppLocale::SUPPORTED {
            let text = Localizer::new(locale);
            for key in keys {
                assert_ne!(text.text(key), key, "{} is missing {key}", locale.code());
            }
            for gauge in practice_gauges() {
                assert!(!gauge_label(text, gauge).starts_with("practice-"));
            }
            for arrange in ArrangeOption::VALUES {
                assert!(!arrange_label(text, arrange).starts_with("practice-"));
            }
        }
    }

    #[test]
    fn dx_practice_panel_keeps_pop_maximum_and_fixed_fields() {
        let ctx = egui::Context::default();
        let rules = PracticeRuleContext {
            rule_mode: bmz_gameplay::rule::RuleMode::Dx,
            key_mode: bmz_core::lane::KeyMode::K9,
        };
        let mut property = PracticeProperty {
            start_gauge: 120,
            judgerank: 222,
            total: Some(4321.0),
            gauge_category: Some(GaugeProperty::FiveKeys),
            ..Default::default()
        };
        let before = property.clone();
        let graph = ResultGraphSnapshot::default();
        for mut cursor in [4, 3, 5, 6] {
            let mut panel = PracticePanelContext {
                property: &mut property,
                rules,
                graph: &graph,
                graph_start_time_ms: 0,
                is_double: false,
                cursor: &mut cursor,
                chart_title: "DX Practice",
                media_ready: true,
                input_enabled: true,
                max_end_time_ms: 120_000,
                default_position: None,
            };
            for pressed in [false, true] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 1024.0),
                        )),
                        events: vec![egui::Event::Key {
                            key: egui::Key::ArrowRight,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        ..Default::default()
                    },
                    |ui| {
                        build_practice_panel(ui.ctx(), &mut panel, Localizer::new(AppLocale::En));
                    },
                );
                if pressed {
                    let labels: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(label) => Some(label.galley.job.text.as_str()),
                            _ => None,
                        })
                        .collect();
                    assert!(labels.contains(&"POP (DX MODE)"));
                    assert!(labels.contains(&"AUTO (DX MODE)"));
                }
                assert_eq!(*panel.property, before);
            }
        }
    }

    #[test]
    fn time_format_is_locale_neutral() {
        assert_eq!(format_time_ms(0), "00:00.0");
        assert_eq!(format_time_ms(125_678), "02:05.6");
    }

    #[test]
    fn note_type_graph_uses_beatoraja_key_and_scratch_colors() {
        let colors = practice_graph_colors(PracticeGraphType::NoteType);

        assert_eq!(colors[2], egui::Color32::from_rgb(0xff, 0x44, 0x44));
        assert_eq!(colors[5], egui::Color32::from_rgb(0xcc, 0xcc, 0xcc));
    }
}
