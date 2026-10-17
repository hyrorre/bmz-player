/// 一般ページの「ディレクトリを開く」。各 app path をファイルマネージャで開く。
pub(super) fn build_directory_buttons(
    ui: &mut egui::Ui,
    app_paths: &AppPaths,
    directory_open_status: &mut Option<DirectoryOpenStatus>,
    text: Localizer,
) {
    ui.horizontal_wrapped(|ui| {
        for target in directory_open_targets(app_paths) {
            if ui.button(target.label).on_hover_text(target.path.display().to_string()).clicked() {
                *directory_open_status = Some(open_directory_target(target, text));
            }
        }
    });
    if let Some(status) = directory_open_status.as_ref() {
        match status.error.as_deref() {
            Some(error) => {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    tr!(
                        text,
                        "menu-directory-open-failed",
                        "label" => status.label,
                        "error" => error
                    ),
                )
                .on_hover_text(status.path.display().to_string());
            }
            None => {
                ui.small(tr!(
                    text,
                    "menu-directory-opened",
                    "label" => status.label
                ))
                .on_hover_text(status.path.display().to_string());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RandomTrainerLaneDrag {
    pub(super) index: usize,
}

pub(super) fn build_random_trainer_page(
    ui: &mut egui::Ui,
    trainer: &mut RandomTrainerState,
    text: Localizer,
) {
    let mut enabled = trainer.is_enabled();
    if ui.checkbox(&mut enabled, tr!(text, "random-trainer-enabled")).changed() {
        trainer.set_enabled(enabled);
    }
    ui.label(tr!(text, "random-trainer-description"));
    ui.label(tr!(text, "random-trainer-next-play"));
    let mut black_white_random = trainer.black_white_random();
    if ui
        .checkbox(&mut black_white_random, tr!(text, "random-trainer-black-white"))
        .on_hover_text(tr!(text, "random-trainer-black-white-help"))
        .changed()
    {
        trainer.set_black_white_random(black_white_random);
    }
    ui.separator();
    ui.label(format!("{} {}", tr!(text, "random-trainer-order"), trainer.lane_order_string()));

    let lane_order = *trainer.lane_order();
    let mut swap = None;
    let mut toggle_partial = None;
    ui.horizontal(|ui| {
        for (index, lane) in lane_order.into_iter().enumerate() {
            ui.push_id(("random_trainer_lane", index), |ui| {
                let is_blue = lane % 2 == 0;
                let is_partial_random = trainer.is_lane_partial_random(lane);
                let fill = if is_blue {
                    egui::Color32::from_rgb(0, 60, 150)
                } else {
                    egui::Color32::from_rgb(235, 238, 244)
                };
                let text_color =
                    if is_blue { egui::Color32::WHITE } else { egui::Color32::from_gray(35) };
                let label = if is_partial_random { format!("{lane}\n?") } else { lane.to_string() };
                let mut button =
                    egui::Button::new(egui::RichText::new(label).size(20.0).color(text_color))
                        .fill(fill)
                        .sense(egui::Sense::click_and_drag());
                if is_partial_random {
                    button = button
                        .stroke(egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(220, 80, 150)));
                }
                let (_, dropped) =
                    ui.dnd_drop_zone::<RandomTrainerLaneDrag, _>(egui::Frame::NONE, |ui| {
                        let response = ui.add_sized([42.0, 64.0], button);
                        response.dnd_set_drag_payload(RandomTrainerLaneDrag { index });
                        let response = response
                            .on_hover_cursor(egui::CursorIcon::Grab)
                            .on_hover_text(tr!(text, "random-trainer-drag"))
                            .on_hover_text(tr!(text, "random-trainer-partial-help"));
                        if response.secondary_clicked() {
                            toggle_partial = Some(lane);
                        }
                    });
                if let Some(payload) = dropped {
                    swap = Some((payload.index, index));
                }
            });
        }
    });
    if let Some((from, to)) = swap {
        trainer.swap_positions(from, to);
    }
    if let Some(lane) = toggle_partial {
        trainer.toggle_lane_partial_random(lane);
    }

    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(text, "random-trainer-reset")).clicked() {
            trainer.reset();
        }
        if ui.button(tr!(text, "random-trainer-mirror")).clicked() {
            trainer.mirror();
        }
        if ui.button(tr!(text, "random-trainer-shift-left")).clicked() {
            trainer.shift_left();
        }
        if ui.button(tr!(text, "random-trainer-shift-right")).clicked() {
            trainer.shift_right();
        }
    });
}
use super::*;
