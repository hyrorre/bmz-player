use super::*;
use crate::song_download::{
    BulkDownloadCounts, BulkDownloadPreview, BulkDownloadScope, ChartDownloadManager,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn build_download_panel(
    ctx: &egui::Context,
    visible: &mut bool,
    manager: &ChartDownloadManager,
    preview: &BulkDownloadPreview,
    scope: Option<&BulkDownloadScope>,
    select: bool,
    tables: &[DifficultyTableRecord],
    text: Localizer,
) -> Vec<DownloadUiCommand> {
    if !*visible {
        return Vec::new();
    }
    let mut commands = Vec::new();
    egui::Window::new(tr!(text, "downloads-title")).id(egui::Id::new("chart_downloads"))
        .open(visible).default_width(660.0).constrain_to(ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN))
        .show(ctx, |ui| {
            if let Some(scope) = scope {
                scope_label(ui, scope, tables, text);
                if ui.add_enabled(select && !preview.preparing, egui::Button::new(tr!(text, "downloads-bulk-prepare"))).clicked() {
                    commands.push(DownloadUiCommand::Prepare(scope.clone()));
                }
            } else { ui.label(tr!(text, "downloads-bulk-hint")); }
            if preview.preparing { ui.horizontal(|ui| { ui.spinner(); ui.label(tr!(text, "downloads-bulk-preparing")); }); }
            if let Some(error) = &preview.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
            if let Some(plan) = &preview.plan {
                ui.separator(); scope_label(ui, &plan.scope, tables, text);
                counts_label(ui, &plan.counts, text);
                ui.horizontal(|ui| {
                    if ui.add_enabled(select && !plan.requests.is_empty(), egui::Button::new(tr!(text, "downloads-bulk-confirm"))).clicked() { commands.push(DownloadUiCommand::Confirm); }
                    if ui.button(tr!(text, "downloads-bulk-dismiss")).clicked() { commands.push(DownloadUiCommand::Dismiss); }
                });
            }
            for batch in manager.batches.iter().rev().take(8) {
                ui.separator(); scope_label(ui, &batch.scope, tables, text); counts_label(ui, &batch.counts, text);
                let (completed, failed) = batch.results(manager);
                ui.label(tr!(text, "downloads-bulk-results", "queued" => batch.counts.queued as i64, "merged" => batch.counts.merged as i64, "completed" => completed as i64, "failed" => failed as i64));
            }
        });
    commands
}

fn scope_label(
    ui: &mut egui::Ui,
    scope: &BulkDownloadScope,
    tables: &[DifficultyTableRecord],
    text: Localizer,
) {
    let name = tables
        .iter()
        .find(|table| table.source_url == scope.source_url)
        .map(|table| table.name.as_str())
        .unwrap_or(&scope.source_url);
    ui.label(format!(
        "{} / {}",
        name,
        scope.level.clone().unwrap_or_else(|| tr!(text, "downloads-all-levels"))
    ))
    .on_hover_text(&scope.source_url);
}

fn counts_label(ui: &mut egui::Ui, counts: &BulkDownloadCounts, text: Localizer) {
    ui.label(tr!(text, "downloads-bulk-counts", "total" => counts.total as i64, "owned" => counts.owned as i64, "downloadable" => counts.downloadable as i64, "unavailable" => counts.unavailable as i64, "merged" => counts.merged as i64));
}
