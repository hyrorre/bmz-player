//! egui overlay for practice configuration (pre-play).

use egui::{Context, RichText};

use crate::config::profile_config::GaugeTypeConfig;
use crate::i18n::Localizer;
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
    text: Localizer,
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
                    ui.heading(text.text("practice-title"));
                    ui.label(RichText::new(practice.chart_title).weak());
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-start-time"));
                        time_ms_field(
                            ui,
                            &mut practice.property.start_time_ms,
                            practice.max_end_time_ms,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-end-time"));
                        time_ms_field(
                            ui,
                            &mut practice.property.end_time_ms,
                            practice.max_end_time_ms,
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-gauge"));
                        egui::ComboBox::from_id_salt("practice_gauge")
                            .selected_text(gauge_label(text, practice.property.gauge))
                            .show_ui(ui, |ui| {
                                for gauge in practice_gauges() {
                                    ui.selectable_value(
                                        &mut practice.property.gauge,
                                        gauge,
                                        gauge_label(text, gauge),
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-gauge-percent"));
                        ui.add(
                            egui::DragValue::new(&mut practice.property.start_gauge)
                                .range(1..=100)
                                .speed(0.2),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-judge-rank"));
                        ui.add(
                            egui::DragValue::new(&mut practice.property.judgerank)
                                .range(1..=400)
                                .speed(0.5),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(text.text("practice-arrange"));
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
                    if let Some(total) = practice.property.total.as_mut() {
                        ui.horizontal(|ui| {
                            ui.label(text.text("practice-total"));
                            ui.add(egui::DragValue::new(total).range(20.0..=5000.0).speed(1.0));
                        });
                    }

                    ui.separator();
                    if practice.media_ready {
                        ui.colored_label(
                            egui::Color32::LIGHT_GREEN,
                            text.text("practice-ready-hint"),
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            text.text("practice-media-loading"),
                        );
                    }

                    ui.horizontal(|ui| {
                        if ui.button(text.text("practice-start-play")).clicked() {
                            start_play = true;
                        }
                        if ui.button(text.text("practice-back-to-select")).clicked() {
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

fn gauge_label(text: Localizer, gauge: GaugeTypeConfig) -> String {
    text.text(match gauge {
        GaugeTypeConfig::AssistEasy => "practice-gauge-assist-easy",
        GaugeTypeConfig::Easy => "practice-gauge-easy",
        GaugeTypeConfig::Normal => "practice-gauge-normal",
        GaugeTypeConfig::Hard => "practice-gauge-hard",
        GaugeTypeConfig::ExHard => "practice-gauge-ex-hard",
        GaugeTypeConfig::Hazard => "practice-gauge-hazard",
        GaugeTypeConfig::AutoShift => "practice-gauge-auto-shift",
    })
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
    fn time_format_is_locale_neutral() {
        assert_eq!(format_time_ms(0), "00:00.0");
        assert_eq!(format_time_ms(125_678), "02:05.6");
    }
}
