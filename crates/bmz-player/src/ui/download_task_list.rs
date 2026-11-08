use super::*;
use crate::song_download::{ChartDownloadManager, DownloadState};

#[derive(Default)]
pub(super) struct DownloadListUi {
    filter: usize,
    indices: Vec<usize>,
    counts: DownloadCounts,
    refreshed: Option<std::time::Instant>,
    task_count: usize,
    batch_count: usize,
    pub(super) batch_results: Vec<(usize, (usize, usize, usize))>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DownloadCounts {
    queued: usize,
    running: usize,
    completed: usize,
    failed: usize,
    cancelled: usize,
}

impl DownloadCounts {
    fn record(&mut self, state: DownloadState) {
        // Keep this exhaustive so new states must be classified before the UI compiles.
        match state {
            DownloadState::Queued => self.queued += 1,
            DownloadState::Resolving
            | DownloadState::Downloading
            | DownloadState::Validating
            | DownloadState::Extracting
            | DownloadState::Scanning => self.running += 1,
            DownloadState::Completed => self.completed += 1,
            DownloadState::Failed => self.failed += 1,
            DownloadState::Cancelled => self.cancelled += 1,
        }
    }

    fn finished(&self) -> usize {
        self.completed + self.failed + self.cancelled
    }
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
    let refresh = view.task_count != manager.tasks().len()
        || view.batch_count != manager.batches.len()
        || view.refreshed.is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(150));
    if refresh {
        view.counts = DownloadCounts::default();
        view.indices.clear();
        for (index, task) in manager.tasks().iter().enumerate() {
            let state = task.state();
            view.counts.record(state);
            let included = match view.filter {
                1 => matches!(
                    state,
                    DownloadState::Resolving
                        | DownloadState::Downloading
                        | DownloadState::Validating
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
        view.task_count = manager.tasks().len();
        view.batch_count = manager.batches.len();
        view.batch_results = manager
            .batches
            .iter()
            .enumerate()
            .rev()
            .take(4)
            .map(|(index, batch)| {
                let (completed, failed) = batch.results(manager);
                (index, (completed, failed, batch.merged_count(manager)))
            })
            .collect();
    }
    ui.separator();
    let finished = view.counts.finished();
    let total = manager.tasks().len();
    ui.label(tr!(text, "downloads-summary", "total" => total as i64, "completed" => view.counts.completed as i64, "failed" => view.counts.failed as i64, "cancelled" => view.counts.cancelled as i64));
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
                view.counts.queued > 0,
                egui::Button::new(tr!(text, "downloads-cancel-queued")),
            )
            .clicked()
        {
            commands.push(DownloadUiCommand::CancelQueued);
            view.refreshed = None;
        }
    });
    // Keep the nested scrollbar clear of the page scrollbar, including its hover width.
    let scrollbar_gap = ui.spacing().scroll.bar_width + ui.spacing().item_spacing.x;
    egui::ScrollArea::vertical()
        .id_salt("chart_download_tasks")
        .max_width((ui.available_width() - scrollbar_gap).max(0.0))
        .max_height(400.0)
        .show_rows(ui, 78.0, view.indices.len(), |ui, range| {
            for row in range {
                let Some(task) = manager.tasks().get(view.indices[row]) else {
                    continue;
                };
                ui.push_id(task.id, |ui| {
                    let state = task.state();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(format!(
                                "#{} {} ({})",
                                task.id,
                                task.requests[0].title,
                                task.requests.len()
                            ))
                            .truncate(),
                        )
                        .on_hover_ui(|ui| {
                            for request in task.requests.iter().take(30) {
                                ui.label(&request.title);
                            }
                            if task.requests.len() > 30 {
                                ui.label("…");
                            }
                        });
                        ui.label(format!("{} · {}", task.source_label(), state_label(state, text)));
                        if matches!(state, DownloadState::Failed | DownloadState::Cancelled) {
                            if ui.button(tr!(text, "downloads-retry")).clicked() {
                                commands.push(DownloadUiCommand::Retry(task.id));
                                view.refreshed = None;
                            }
                        } else if !state.terminal()
                            && state != DownloadState::Scanning
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
                    let notice = if task.registration_missing {
                        Some("downloads-target-missing")
                    } else if task.archive_unchanged {
                        Some("downloads-archive-unchanged")
                    } else {
                        None
                    };
                    if let Some(key) = notice {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text.text(key)).color(egui::Color32::LIGHT_RED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(text.text(key));
                    } else if let Some(error) = &task.error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(error).color(egui::Color32::LIGHT_RED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(error);
                    } else if let Some(result) = &task.result {
                        ui.add(egui::Label::new(result.scan_path.display().to_string()).truncate());
                    } else {
                        ui.label("");
                    }
                    ui.separator();
                });
            }
        });
    if finished < total {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    }
}

fn state_label(state: DownloadState, text: Localizer) -> String {
    text.text(match state {
        DownloadState::Queued => "downloads-state-queued",
        DownloadState::Resolving => "downloads-state-resolving",
        DownloadState::Downloading => "downloads-state-downloading",
        DownloadState::Validating => "downloads-state-validating",
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
    use crate::song_download::{
        ChartDownloadIdentity, ChartDownloadRequest, DownloadTaskId, MissingChartAction,
    };

    fn submit_task(manager: &mut ChartDownloadManager, state: DownloadState) -> DownloadTaskId {
        let request_id = manager.tasks().len() as u64;
        let (id, merged) = manager.submit(ChartDownloadRequest {
            keep_archive: true,
            request_id,
            identity: ChartDownloadIdentity::default(),
            title: format!("Chart {request_id}"),
            download_root: PathBuf::new(),
            action: MissingChartAction::Http {
                api_url: format!("https://download.test/{request_id}.zip"),
                md5: String::new(),
                sha256: String::new(),
            },
        });
        assert!(!merged);
        // Do not poll the manager: these UI tests need no worker, network or archive.
        manager.task(id).unwrap().control.set_state(state);
        id
    }

    fn draw_list(
        ctx: &egui::Context,
        manager: &ChartDownloadManager,
        view: &mut DownloadListUi,
    ) -> egui::FullOutput {
        let mut commands = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            build_task_list(ui, manager, view, &mut commands, Localizer::new(AppLocale::En))
        });
        assert!(commands.is_empty());
        output
    }

    #[test]
    fn validating_task_renders_in_download_list() {
        let mut manager = ChartDownloadManager::default();
        submit_task(&mut manager, DownloadState::Validating);
        let ctx = egui::Context::default();
        let mut view = DownloadListUi::default();
        let output = draw_list(&ctx, &manager, &mut view);

        assert_eq!(view.indices, [0]);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Checking archive"))
        }));
    }

    #[test]
    fn all_download_states_are_counted_for_every_filter() {
        let mut manager = ChartDownloadManager::default();
        for state in [
            DownloadState::Queued,
            DownloadState::Resolving,
            DownloadState::Downloading,
            DownloadState::Extracting,
            DownloadState::Scanning,
            DownloadState::Completed,
            DownloadState::Failed,
            DownloadState::Cancelled,
            DownloadState::Validating,
        ] {
            submit_task(&mut manager, state);
        }
        let ctx = egui::Context::default();
        let mut view = DownloadListUi::default();
        let expected_counts =
            DownloadCounts { queued: 1, running: 5, completed: 1, failed: 1, cancelled: 1 };
        let expected_indices: [&[usize]; 6] = [
            &[0, 1, 2, 3, 4, 5, 6, 7, 8], // All
            &[1, 2, 3, 4, 8],             // Running, including Validating
            &[0],                         // Queued
            &[5],                         // Completed
            &[6],                         // Failed
            &[7],                         // Cancelled
        ];
        for (filter, indices) in expected_indices.iter().enumerate() {
            view.filter = filter;
            view.refreshed = None;
            draw_list(&ctx, &manager, &mut view);

            assert_eq!(view.task_count, 9);
            assert_eq!(view.counts, expected_counts, "filter {filter}");
            assert_eq!(view.counts.finished(), 3);
            assert_eq!(view.indices, *indices, "filter {filter}");
        }
    }

    #[test]
    fn download_state_transitions_refresh_counts_and_visible_tasks() {
        let mut manager = ChartDownloadManager::default();
        let id = submit_task(&mut manager, DownloadState::Downloading);
        let ctx = egui::Context::default();
        let mut view = DownloadListUi::default();

        for state in [
            DownloadState::Downloading,
            DownloadState::Validating,
            DownloadState::Scanning,
            DownloadState::Completed,
        ] {
            manager.task(id).unwrap().control.set_state(state);
            let completed = usize::from(state == DownloadState::Completed);
            let running = 1 - completed;
            let expected_counts =
                DownloadCounts { running, completed, ..DownloadCounts::default() };
            for (filter, visible) in [(0, true), (1, running > 0), (3, completed > 0)] {
                view.filter = filter;
                // Use the UI's invalidation mechanism instead of sleeping for 150ms.
                view.refreshed = None;
                draw_list(&ctx, &manager, &mut view);

                assert_eq!(view.task_count, 1);
                assert_eq!(view.counts, expected_counts, "state {state:?}, filter {filter}");
                assert_eq!(view.counts.finished(), completed);
                let indices: &[usize] = if visible { &[0] } else { &[] };
                assert_eq!(view.indices, indices, "state {state:?}, filter {filter}");
            }
        }
    }

    #[test]
    fn adding_a_task_refreshes_the_cached_download_list() {
        let mut manager = ChartDownloadManager::default();
        submit_task(&mut manager, DownloadState::Completed);
        let ctx = egui::Context::default();
        let mut view = DownloadListUi { filter: 1, ..Default::default() };
        draw_list(&ctx, &manager, &mut view);
        assert_eq!(view.task_count, 1);
        assert_eq!(view.counts.completed, 1);
        assert!(view.indices.is_empty());

        submit_task(&mut manager, DownloadState::Validating);
        // Keep the time cache fresh: task count changes must trigger the recount.
        view.refreshed = Some(std::time::Instant::now());
        draw_list(&ctx, &manager, &mut view);

        assert_eq!(view.task_count, 2);
        assert_eq!(view.counts, DownloadCounts { running: 1, completed: 1, ..Default::default() });
        assert_eq!(view.counts.finished(), 1);
        assert_eq!(view.indices, [1]);
    }

    #[test]
    fn large_download_list_is_virtualized_and_hidden_panel_does_no_list_work() {
        let mut manager = ChartDownloadManager::default();
        for id in 0..1000 {
            manager.submit(ChartDownloadRequest {
                keep_archive: false,
                request_id: id,
                identity: ChartDownloadIdentity::default(),
                title: format!("Chart {id}"),
                download_root: PathBuf::new(),
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
        assert_eq!(manager.tasks().len(), 1000);
    }
}
