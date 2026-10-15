use super::*;
use crate::song_download::{ChartDownloadManager, DownloadState};

#[derive(Default)]
pub(super) struct DownloadListUi {
    filter: usize,
    indices: Vec<usize>,
    counts: [usize; 8],
    refreshed: Option<std::time::Instant>,
}

const FILTERS: [&str; 6] = [
    "downloads-filter-all",
    "downloads-filter-running",
    "downloads-state-queued",
    "downloads-state-completed",
    "downloads-state-failed",
    "downloads-state-cancelled",
];

pub(super) fn build_task_list(
    ui: &mut egui::Ui,
    manager: &ChartDownloadManager,
    view: &mut DownloadListUi,
    commands: &mut Vec<DownloadUiCommand>,
    text: Localizer,
) {
    let refresh =
        view.refreshed.is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(150));
    if refresh {
        view.counts.fill(0);
        view.indices.clear();
        for (index, task) in manager.tasks().iter().enumerate() {
            let state = task.state();
            view.counts[state as usize] += 1;
            let included = match view.filter {
                1 => matches!(
                    state,
                    DownloadState::Resolving
                        | DownloadState::Downloading
                        | DownloadState::Extracting
                        | DownloadState::Scanning
                ),
                2 => state == DownloadState::Queued,
                3 => state == DownloadState::Completed,
                4 => state == DownloadState::Failed,
                5 => state == DownloadState::Cancelled,
                _ => true,
            };
            if included {
                view.indices.push(index);
            }
        }
        view.refreshed = Some(std::time::Instant::now());
    }
    ui.separator();
    let finished = view.counts[5] + view.counts[6] + view.counts[7];
    let total = manager.tasks().len();
    ui.label(tr!(text, "downloads-summary", "total" => total as i64, "completed" => view.counts[5] as i64, "failed" => view.counts[6] as i64, "cancelled" => view.counts[7] as i64));
    if total > 0 {
        ui.add(egui::ProgressBar::new(finished as f32 / total as f32).text(tr!(text, "downloads-overall-progress", "finished" => finished as i64, "total" => total as i64)));
    }
    ui.horizontal_wrapped(|ui| {
        for (filter, key) in FILTERS.iter().enumerate() {
            if ui.selectable_value(&mut view.filter, filter, text.text(key)).changed() {
                view.refreshed = None;
            }
        }
        if ui
            .add_enabled(
                view.counts[0] > 0,
                egui::Button::new(tr!(text, "downloads-cancel-queued")),
            )
            .clicked()
        {
            commands.push(DownloadUiCommand::CancelQueued);
            view.refreshed = None;
        }
    });
    egui::ScrollArea::vertical().id_salt("chart_download_tasks").max_height(400.0).show_rows(
        ui,
        78.0,
        view.indices.len(),
        |ui, range| {
            for row in range {
                let Some(task) = manager.tasks().get(view.indices[row]) else {
                    continue;
                };
                ui.push_id(task.id, |ui| {
                    let state = task.state();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(format!("#{} {}", task.id, task.requests[0].title))
                                .truncate(),
                        )
                        .on_hover_text(&task.requests[0].title);
                        ui.label(format!("{} · {}", task.source_name(), state_label(state, text)));
                        if matches!(state, DownloadState::Failed | DownloadState::Cancelled) {
                            if ui.button(tr!(text, "downloads-retry")).clicked() {
                                commands.push(DownloadUiCommand::Retry(task.id));
                                view.refreshed = None;
                            }
                        } else if !state.terminal()
                            && ui.button(tr!(text, "downloads-cancel")).clicked()
                        {
                            commands.push(DownloadUiCommand::Cancel(task.id));
                            view.refreshed = None;
                        }
                    });
                    let (bytes, total) = task.control.bytes();
                    if let Some(total) = total {
                        ui.add(
                            egui::ProgressBar::new(
                                (bytes as f64 / total as f64).clamp(0.0, 1.0) as f32
                            )
                            .text(format!(
                                "{bytes} / {total} B ({:.1}%)",
                                (100.0 * bytes as f64 / total as f64).clamp(0.0, 100.0)
                            )),
                        );
                    } else {
                        ui.label(
                            tr!(text, "downloads-bytes-unknown", "bytes" => bytes.to_string()),
                        );
                    }
                    if let Some(error) = &task.error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(error).color(egui::Color32::LIGHT_RED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(error);
                    } else if let Some(result) = &task.result {
                        ui.add(egui::Label::new(result.chart_dir.display().to_string()).truncate());
                    } else {
                        ui.label("");
                    }
                    ui.separator();
                });
            }
        },
    );
    if finished < total {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }
}

fn state_label(state: DownloadState, text: Localizer) -> String {
    text.text(match state {
        DownloadState::Queued => "downloads-state-queued",
        DownloadState::Resolving => "downloads-state-resolving",
        DownloadState::Downloading => "downloads-state-downloading",
        DownloadState::Extracting => "downloads-state-extracting",
        DownloadState::Scanning => "downloads-state-scanning",
        DownloadState::Completed => "downloads-state-completed",
        DownloadState::Failed => "downloads-state-failed",
        DownloadState::Cancelled => "downloads-state-cancelled",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song_download::{ChartDownloadIdentity, ChartDownloadRequest, MissingChartAction};

    #[test]
    fn large_download_list_is_virtualized_and_hidden_panel_does_no_list_work() {
        let mut manager = ChartDownloadManager::default();
        for id in 0..1000 {
            manager.submit(ChartDownloadRequest {
                request_id: id,
                identity: ChartDownloadIdentity::default(),
                title: format!("Chart {id}"),
                data_dir: PathBuf::new(),
                action: MissingChartAction::Unavailable,
            });
        }
        let ctx = egui::Context::default();
        let mut view = DownloadListUi::default();
        let mut commands = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            build_task_list(ui, &manager, &mut view, &mut commands, Localizer::new(AppLocale::En))
        });
        assert_eq!(view.indices.len(), 1000);
        assert!(output.shapes.len() < 500, "only visible task rows should render");
        assert!(commands.is_empty());
        let refreshed = view.refreshed;
        let hidden = super::super::downloads::build_download_panel(
            &ctx,
            &mut false,
            &manager,
            &mut view,
            &Default::default(),
            None,
            true,
            &[],
            Localizer::new(AppLocale::En),
        );
        assert!(hidden.is_empty());
        assert_eq!(view.refreshed, refreshed);
        assert_eq!(manager.tasks().len(), 1000);
    }
}
