//! egui overlay for practice configuration (pre-play).

use egui::{Context, RichText};

use crate::config::profile_config::GaugeTypeConfig;
use crate::screens::practice::PracticeProperty;
use crate::select_options::ArrangeOption;

pub struct PracticePanelContext<'a> {
    pub property: &'a mut PracticeProperty,
    pub chart_title: &'a str,
    pub media_ready: bool,
    pub max_end_time_ms: u32,
}

pub struct PracticePanelOutput {
    pub start_play: bool,
    pub leave: bool,
}

pub fn build_practice_panel(
    ctx: &Context,
    practice: &mut PracticePanelContext<'_>,
) -> PracticePanelOutput {
    let mut start_play = false;
    let mut leave = false;

    egui::Area::new(egui::Id::new("practice_config_panel"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .fill(egui::Color32::from_rgba_unmultiplied(16, 20, 32, 230))
                .show(ui, |ui| {
                    ui.set_min_width(360.0);
                    ui.heading("Practice Mode");
                    ui.label(RichText::new(practice.chart_title).weak());
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Start");
                        time_ms_field(
                            ui,
                            &mut practice.property.start_time_ms,
                            practice.max_end_time_ms,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("End");
                        time_ms_field(
                            ui,
                            &mut practice.property.end_time_ms,
                            practice.max_end_time_ms,
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("Gauge");
                        egui::ComboBox::from_id_salt("practice_gauge")
                            .selected_text(gauge_label(practice.property.gauge))
                            .show_ui(ui, |ui| {
                                for gauge in practice_gauges() {
                                    ui.selectable_value(
                                        &mut practice.property.gauge,
                                        gauge,
                                        gauge_label(gauge),
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Gauge %");
                        ui.add(
                            egui::DragValue::new(&mut practice.property.start_gauge)
                                .range(1..=100)
                                .speed(0.2),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Judgerank");
                        ui.add(
                            egui::DragValue::new(&mut practice.property.judgerank)
                                .range(1..=400)
                                .speed(0.5),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Arrange");
                        egui::ComboBox::from_id_salt("practice_arrange")
                            .selected_text(practice.property.arrange.as_str())
                            .show_ui(ui, |ui| {
                                for arrange in ArrangeOption::VALUES {
                                    ui.selectable_value(
                                        &mut practice.property.arrange,
                                        arrange,
                                        arrange.as_str(),
                                    );
                                }
                            });
                    });
                    if let Some(total) = practice.property.total.as_mut() {
                        ui.horizontal(|ui| {
                            ui.label("TOTAL");
                            ui.add(egui::DragValue::new(total).range(20.0..=5000.0).speed(1.0));
                        });
                    }

                    ui.separator();
                    if practice.media_ready {
                        ui.colored_label(
                            egui::Color32::LIGHT_GREEN,
                            "Enter / ボタンで区間プレイを開始",
                        );
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "メディア読込中…");
                    }

                    ui.horizontal(|ui| {
                        if ui.button("プレイ開始").clicked() {
                            start_play = true;
                        }
                        if ui.button("選曲へ戻る (Esc)").clicked() {
                            leave = true;
                        }
                    });
                });
        });

    if ctx.input(|input| input.key_pressed(egui::Key::Enter)) && practice.media_ready {
        start_play = true;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
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

fn practice_gauges() -> [GaugeTypeConfig; 6] {
    [
        GaugeTypeConfig::AssistEasy,
        GaugeTypeConfig::Easy,
        GaugeTypeConfig::Normal,
        GaugeTypeConfig::Hard,
        GaugeTypeConfig::ExHard,
        GaugeTypeConfig::Hazard,
    ]
}

fn gauge_label(gauge: GaugeTypeConfig) -> &'static str {
    match gauge {
        GaugeTypeConfig::AssistEasy => "ASSIST EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX-HARD",
        GaugeTypeConfig::Hazard => "HAZARD",
        GaugeTypeConfig::AutoShift => "AUTO SHIFT",
    }
}
