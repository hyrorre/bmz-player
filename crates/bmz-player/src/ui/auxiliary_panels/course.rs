use super::*;

pub(super) fn build_course_result_content(
    ui: &mut egui::Ui,
    summary: &CourseResultSummary,
    text: Localizer,
) {
    ui.heading(&summary.title);

    ui.horizontal(|ui| {
        let kind_label = match summary.kind {
            bmz_core::course::CourseKind::Dan => tr!(text, "course-kind-dan"),
            bmz_core::course::CourseKind::Course => tr!(text, "course-kind-course"),
        };
        ui.label(kind_label);
        ui.separator();
        if summary.course_failed {
            ui.colored_label(egui::Color32::LIGHT_RED, tr!(text, "course-status-failed"));
        } else if summary.course_clear {
            ui.colored_label(egui::Color32::LIGHT_GREEN, tr!(text, "course-status-clear"));
        } else {
            ui.colored_label(egui::Color32::LIGHT_YELLOW, tr!(text, "course-status-no-trophy"));
        }
        ui.separator();
        ui.label(format!("{}/{}", summary.played_entries, summary.total_entries));
    });

    ui.separator();

    // Totals.
    let score_rate = if summary.max_ex_score > 0 {
        summary.total_ex_score as f32 / summary.max_ex_score as f32 * 100.0
    } else {
        0.0
    };
    egui::Grid::new("course_result_totals").num_columns(2).show(ui, |ui| {
        ui.label(tr!(text, "course-ex-score"));
        ui.label(format!(
            "{} / {} ({:.2}%)",
            summary.total_ex_score, summary.max_ex_score, score_rate
        ));
        ui.end_row();
        ui.label(tr!(text, "course-notes"));
        ui.label(format!("{}", summary.total_notes));
        ui.end_row();
        ui.label("BP");
        ui.label(format!("{}", summary.bp));
        ui.end_row();
        ui.label(tr!(text, "course-pg-great"));
        ui.label(format!("{} / {}", summary.judge_counts.pgreat, summary.judge_counts.great));
        ui.end_row();
        ui.label(tr!(text, "course-good-bad-poor"));
        ui.label(format!(
            "{} / {} / {}",
            summary.judge_counts.good, summary.judge_counts.bad, summary.judge_counts.poor,
        ));
        ui.end_row();
    });

    if !summary.trophy_results.is_empty() {
        ui.separator();
        ui.label(tr!(text, "course-trophies"));
        // `trophy_results` is built only from `definition.trophies`
        // in `ActiveCourseSession::into_result`, so it cannot show
        // a name that the course author did not declare.
        ui.horizontal_wrapped(|ui| {
            for trophy in &summary.trophy_results {
                let color = if trophy.achieved {
                    egui::Color32::from_rgb(255, 215, 0) // gold
                } else {
                    egui::Color32::DARK_GRAY
                };
                ui.colored_label(color, &trophy.name);
            }
        });
    }

    // BEST section: shows the highest persisted attempt for this
    // course.  Includes the current attempt if it improved the
    // record (the lookup runs after insert_course_score).
    if let Some(best) = &summary.best_score {
        ui.separator();
        ui.label(tr!(text, "course-best"));
        let best_rate = if best.max_ex_score > 0 {
            best.ex_score as f32 / best.max_ex_score as f32 * 100.0
        } else {
            0.0
        };
        let is_new_record = best.ex_score == summary.total_ex_score
            && best.max_ex_score == summary.max_ex_score
            && !summary.course_failed;
        egui::Grid::new("course_result_best").num_columns(2).show(ui, |ui| {
            ui.label(tr!(text, "course-ex-score"));
            let ex_text = format!("{} / {} ({:.2}%)", best.ex_score, best.max_ex_score, best_rate);
            if is_new_record {
                ui.colored_label(egui::Color32::from_rgb(255, 215, 0), ex_text);
            } else {
                ui.label(ex_text);
            }
            ui.end_row();
            ui.label(tr!(text, "course-column-clear"));
            ui.label(&best.clear_type);
            ui.end_row();
            ui.label(tr!(text, "course-max-combo"));
            ui.label(format!("{}", best.max_combo));
            ui.end_row();
            ui.label("BP");
            ui.label(format!("{}", best.bp));
            ui.end_row();
        });
        if is_new_record {
            ui.colored_label(egui::Color32::from_rgb(255, 215, 0), tr!(text, "course-new-record"));
        }
    }

    if !summary.entry_summaries.is_empty() {
        ui.separator();
        ui.label(tr!(text, "course-each-song"));
        egui::Grid::new("course_result_entries").num_columns(6).striped(true).show(ui, |ui| {
            // Header row.
            ui.label("#");
            ui.label(tr!(text, "course-column-title"));
            ui.label(tr!(text, "course-column-ex"));
            ui.label(tr!(text, "course-column-combo"));
            ui.label(tr!(text, "course-column-clear"));
            ui.label(tr!(text, "course-column-bp"));
            ui.end_row();
            for (i, entry) in summary.entry_summaries.iter().enumerate() {
                ui.label(format!("{}", i + 1));
                let title = if entry.title.is_empty() {
                    tr!(text, "common-no-title")
                } else {
                    entry.title.clone()
                };
                ui.label(title);
                ui.label(format!("{}", entry.ex_score));
                ui.label(format!("{}", entry.max_combo));
                // Color the clear cell so failed entries stand out.
                let clear_text = entry.clear_type.as_str();
                let clear_color = match entry.clear_type {
                    bmz_core::clear::ClearType::Failed => egui::Color32::LIGHT_RED,
                    bmz_core::clear::ClearType::FullCombo
                    | bmz_core::clear::ClearType::Perfect
                    | bmz_core::clear::ClearType::Max => egui::Color32::LIGHT_GREEN,
                    _ => ui.visuals().text_color(),
                };
                ui.colored_label(clear_color, clear_text);
                ui.label(format!("{}", entry.bp));
                ui.end_row();
            }
        });
    }
}

/// 選曲画面でコース行にカーソルがある間、コース内の各曲のメタ情報を表示する
/// プレビューパネル。
pub(super) fn build_course_preview_panel(
    ctx: &egui::Context,
    preview: &SelectCourseRow,
    text: Localizer,
) {
    let content_rect = ctx.content_rect();
    let pos = egui::pos2(16.0, content_rect.bottom() - 320.0);

    egui::Window::new(tr!(text, "course-preview-title"))
        .id(egui::Id::new("course_preview_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .title_bar(true)
        .constrain_to(content_rect.shrink(PANEL_VIEWPORT_MARGIN))
        .default_pos(pos)
        .default_width(380.0)
        .max_height(300.0)
        .show(ctx, |ui| {
            ui.heading(&preview.title);
            ui.horizontal(|ui| {
                ui.label(&preview.category_label);
                ui.separator();
                ui.label(tr!(
                    text,
                    "course-preview-resolved",
                    "resolved" => preview.resolved_count,
                    "total" => preview.entry_count
                ));
                ui.separator();
                ui.label(tr!(text, "course-preview-notes", "notes" => preview.total_notes));
            });
            if !preview.trophy_names.is_empty() {
                ui.label(tr!(
                    text,
                    "course-preview-trophies",
                    "trophies" => preview.trophy_names.join(" / ")
                ));
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("course_preview_entries").num_columns(4).striped(true).show(
                    ui,
                    |ui| {
                        ui.label("#");
                        ui.label(tr!(text, "course-column-title"));
                        ui.label("☆");
                        ui.label(tr!(text, "course-notes"));
                        ui.end_row();
                        for (i, entry) in preview.entry_previews.iter().enumerate() {
                            ui.label(format!("{}", i + 1));
                            let title = if entry.title.is_empty() {
                                tr!(text, "common-no-title")
                            } else {
                                entry.title.clone()
                            };
                            if entry.resolved {
                                ui.label(&title);
                            } else {
                                ui.colored_label(
                                    egui::Color32::GRAY,
                                    tr!(
                                        text,
                                        "course-preview-missing",
                                        "title" => title.as_str()
                                    ),
                                );
                            }
                            ui.label(&entry.play_level);
                            ui.label(format!("{}", entry.total_notes));
                            ui.end_row();
                        }
                    },
                );
            });
        });
}
