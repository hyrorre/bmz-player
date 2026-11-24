use std::time::Instant;

use bmz_core::lane::Lane;
use bmz_core::score::score_rank_index;

use super::*;
use crate::screens::result_ir::ResultIrState;
use crate::screens::result_model::{ResultLaneDetails, ResultLaneSource};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
enum ResultPanelTab {
    #[default]
    Details,
    Ranking,
}

#[derive(Default)]
pub(super) struct ResultPanelState {
    started_at: Option<Instant>,
    tab: ResultPanelTab,
}

#[derive(Clone, Copy)]
pub(super) enum ResultPanelDetails<'a> {
    Play(Option<&'a ResultLaneDetails>),
    Course(&'a CourseResultSummary),
}

impl ResultPanelState {
    pub(super) fn request_raise(&mut self) {
        self.started_at = None;
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        details: ResultPanelDetails<'_>,
        ir: Option<&mut ResultIrState>,
        started_at: Instant,
        text: Localizer,
    ) {
        let raise_window = self.started_at.replace(started_at) != Some(started_at);

        let constrain = ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN);
        let chrome = panel_window_chrome(ctx);
        let width = (constrain.width() - chrome.x).clamp(32.0, 544.0);
        let height = (constrain.height() - chrome.y).max(32.0);
        let pos = egui::pos2(constrain.right() - width - chrome.x, constrain.top());
        let id = egui::Id::new("result_overlay");
        egui::Window::new(tr!(text, "result-panel-title"))
            .id(id)
            .resizable(false)
            .constrain_to(constrain)
            .default_pos(pos)
            .default_width(width)
            .min_width(width)
            .max_width(width)
            .max_height(height)
            .show(ctx, |ui| {
                let tabs = ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.tab,
                        ResultPanelTab::Details,
                        tr!(text, "result-panel-details"),
                    );
                    ui.selectable_value(
                        &mut self.tab,
                        ResultPanelTab::Ranking,
                        tr!(text, "result-ir-title"),
                    );
                });
                ui.separator();
                egui::ScrollArea::both()
                    .id_salt(self.tab)
                    .min_scrolled_width(0.0)
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, true])
                    .max_height((height - tabs.response.rect.height() - 12.0).max(24.0))
                    .show(ui, |ui| match self.tab {
                        ResultPanelTab::Details => match details {
                            ResultPanelDetails::Play(play) => {
                                build_play_result_content(ui, play, text)
                            }
                            ResultPanelDetails::Course(course) => {
                                build_course_result_content(ui, course, text)
                            }
                        },
                        ResultPanelTab::Ranking => {
                            if let Some(ir) = ir {
                                build_result_ir_content(ui, ir, text);
                            } else {
                                ui.label(tr!(text, "result-panel-ranking-unavailable"));
                            }
                        }
                    });
            });
        if raise_window {
            // The right-hand window can overlap BMZ Menu on smaller screens.
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, id));
        }
    }
}

fn build_play_result_content(
    ui: &mut egui::Ui,
    details: Option<&ResultLaneDetails>,
    text: Localizer,
) {
    let Some(details) = details else {
        ui.label(tr!(text, "result-panel-lanes-unavailable"));
        return;
    };
    let lanes = details.key_mode.active_lanes();
    let is_double = matches!(details.key_mode, KeyMode::K10 | KeyMode::K14);
    if is_double {
        let (left, right) = lanes.split_at(lanes.len() / 2);
        show_lane_grid(ui, details, left, "1P", details.arrange[0].as_str(), text);
        ui.add_space(5.0);
        show_lane_grid(ui, details, right, "2P", details.arrange[1].as_str(), text);
    } else {
        show_lane_grid(ui, details, lanes, "", details.arrange[0].as_str(), text);
    }
}

fn show_lane_grid(
    ui: &mut egui::Ui,
    details: &ResultLaneDetails,
    lanes: &[Lane],
    side: &str,
    arrange: &str,
    text: Localizer,
) {
    let is_double = !side.is_empty();
    let stats = |lane: &Lane| details.lanes[lane.index()];
    ui.horizontal(|ui| {
        ui.strong(if is_double { side.to_owned() } else { tr!(text, "result-panel-lanes") });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.strong(arrange);
        });
    });
    ui.separator();
    let max_bp = lanes.iter().map(|lane| stats(lane).bp).max().unwrap_or(0);
    let cell = |ui: &mut egui::Ui, label: egui::RichText| {
        ui.allocate_ui_with_layout(
            egui::vec2(46.0, 19.0),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| ui.label(label),
        );
    };
    egui::Grid::new(("result_lane_stats", side)).spacing([4.0, 3.0]).striped(true).show(ui, |ui| {
        ui.strong(tr!(text, "result-panel-lane"));
        for &lane in lanes {
            cell(ui, egui::RichText::new(lane_label(lane, is_double)).strong());
        }
        ui.end_row();
        ui.label(tr!(text, "result-panel-source-lane"));
        for lane in lanes {
            let source = stats(lane).source;
            ui.allocate_ui_with_layout(
                egui::vec2(46.0, 20.0),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| match source {
                    ResultLaneSource::Fixed(source) => {
                        let label = lane_label(source, is_double);
                        let cross_side = is_double && (lane.index() < 8) != (source.index() < 8);
                        let label = if cross_side {
                            let side = if source.index() < 8 { "1P" } else { "2P" };
                            format!("{side} {label}")
                        } else {
                            label
                        };
                        let (fill, fg) = match source {
                            Lane::Scratch | Lane::Scratch2 => {
                                (egui::Color32::from_rgb(133, 48, 48), egui::Color32::WHITE)
                            }
                            _ if (if is_double && source.index() >= 8 {
                                source.index() - 7
                            } else {
                                source.index()
                            }) % 2
                                == 0 =>
                            {
                                (egui::Color32::from_rgb(69, 87, 109), egui::Color32::WHITE)
                            }
                            _ => (egui::Color32::from_gray(223), egui::Color32::from_gray(30)),
                        };
                        egui::Frame::new()
                            .fill(fill)
                            .corner_radius(3)
                            .inner_margin(egui::Margin::symmetric(5, 1))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(label).color(fg).strong());
                            });
                    }
                    ResultLaneSource::PerNote => {
                        ui.label("↔").on_hover_text(tr!(text, "result-panel-source-per-note"));
                    }
                    ResultLaneSource::Unknown => {
                        ui.label("—");
                    }
                },
            );
        }
        ui.end_row();
        ui.label(tr!(text, "result-panel-lane-rank"));
        for lane in lanes {
            let stats = stats(lane);
            let rank = if stats.auto_scratch {
                "AUTO"
            } else {
                score_rank_index(stats.ex_score, stats.notes)
                    .map_or("—", |rank| ["AAA", "AA", "A", "B", "C", "D", "E", "F"][rank])
            };
            cell(ui, egui::RichText::new(rank).strong());
        }
        ui.end_row();
        ui.label(tr!(text, "result-panel-ex-rate"));
        for lane in lanes {
            let stats = stats(lane);
            let label = if stats.notes > 0 && !stats.auto_scratch {
                format!("{:.1}%", 50.0 * f64::from(stats.ex_score) / f64::from(stats.notes))
            } else {
                "—".into()
            };
            cell(ui, egui::RichText::new(label).monospace());
        }
        ui.end_row();
        ui.label("BP");
        for lane in lanes {
            let stats = stats(lane);
            let label = if stats.auto_scratch { "—".into() } else { stats.bp.to_string() };
            let mut label = egui::RichText::new(label).monospace();
            if stats.bp > 0 && stats.bp == max_bp {
                label = label.color(ui.visuals().warn_fg_color).strong();
            }
            cell(ui, label);
        }
        ui.end_row();
        ui.label(tr!(text, "course-notes"));
        for lane in lanes {
            cell(ui, egui::RichText::new(stats(lane).notes.to_string()).monospace());
        }
        ui.end_row();
    });
    if lanes.iter().any(|lane| stats(lane).source == ResultLaneSource::PerNote) {
        ui.small(format!("↔ {}", tr!(text, "result-panel-source-per-note")));
    }
}

fn lane_label(lane: Lane, is_double: bool) -> String {
    match lane {
        Lane::Scratch | Lane::Scratch2 => "SCR".into(),
        _ if is_double && lane.index() >= 8 => (lane.index() - 7).to_string(),
        _ => lane.index().to_string(),
    }
}
