use super::*;
use crate::song_download::{
    BulkDownloadCounts, BulkDownloadPreview, BulkDownloadScope, ChartDownloadManager,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn build_download_panel(
    ctx: &egui::Context,
    visible: &mut bool,
    manager: &ChartDownloadManager,
    list: &mut super::download_task_list::DownloadListUi,
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
                scope_label(ui, scope, tables);
                if ui.add_enabled(select && !preview.preparing, egui::Button::new(tr!(text, "downloads-bulk-prepare"))).clicked() {
                    commands.push(DownloadUiCommand::Prepare(scope.clone()));
                }
            } else { ui.label(tr!(text, "downloads-bulk-hint")); }
            if preview.preparing { ui.horizontal(|ui| { ui.spinner(); ui.label(tr!(text, "downloads-bulk-preparing")); }); }
            if let Some(error) = &preview.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
            if let Some(plan) = &preview.plan {
                ui.separator(); scope_label(ui, &plan.scope, tables);
                counts_label(ui, &plan.counts, text);
                ui.horizontal(|ui| {
                    if ui.add_enabled(select && !plan.requests.is_empty(), egui::Button::new(tr!(text, "downloads-bulk-confirm"))).clicked() { commands.push(DownloadUiCommand::Confirm); }
                    if ui.button(tr!(text, "downloads-bulk-dismiss")).clicked() { commands.push(DownloadUiCommand::Dismiss); }
                });
            }
            super::download_task_list::build_task_list(ui, manager, list, &mut commands, text);
            for (index, batch) in manager.batches.iter().enumerate().rev().take(4) {
                ui.separator(); scope_label(ui, &batch.scope, tables); counts_label(ui, &batch.counts, text);
                let (completed, failed, merged) = list.batch_results.iter().find(|(id,_)| *id == index).map(|(_,results)| *results).unwrap_or_default();
                ui.label(tr!(text, "downloads-bulk-results", "queued" => batch.counts.queued as i64, "merged" => merged as i64, "completed" => completed as i64, "failed" => failed as i64));
            }
        });
    commands
}

fn scope_label(ui: &mut egui::Ui, scope: &BulkDownloadScope, tables: &[DifficultyTableRecord]) {
    ui.label(bulk_scope_breadcrumb(scope, tables)).on_hover_text(&scope.source_url);
}

/// The Select skin's breadcrumb for the scope: "Stella" or "Stella > st0".
fn bulk_scope_breadcrumb(scope: &BulkDownloadScope, tables: &[DifficultyTableRecord]) -> String {
    use crate::screens::select_model::{fallback_table_name, table_level_breadcrumb};
    let table = tables.iter().find(|table| table.source_url == scope.source_url);
    let name = table.map_or_else(|| fallback_table_name(&scope.source_url), |t| t.name.clone());
    match &scope.level {
        Some(level) => {
            table_level_breadcrumb(&name, table.map_or("", |table| table.symbol.as_str()), level)
        }
        None => name,
    }
}

fn counts_label(ui: &mut egui::Ui, counts: &BulkDownloadCounts, text: Localizer) {
    ui.label(tr!(text, "downloads-bulk-counts", "total" => counts.total as i64, "owned" => counts.owned as i64, "downloadable" => counts.downloadable as i64, "unavailable" => counts.unavailable as i64, "merged" => counts.merged as i64));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_scope_uses_the_select_skin_breadcrumb() {
        let tables = [DifficultyTableRecord {
            id: 1,
            source_url: "https://table.test/stella/".into(),
            name: "Stella".into(),
            symbol: "st".into(),
            level_order: Vec::new(),
            fetched_at: 0,
        }];
        let scope = |source_url: &str, level: Option<&str>| BulkDownloadScope {
            source_url: source_url.into(),
            level: level.map(Into::into),
        };
        let stella = "https://table.test/stella/";
        assert_eq!(bulk_scope_breadcrumb(&scope(stella, Some("0")), &tables), "Stella > st0");
        assert_eq!(bulk_scope_breadcrumb(&scope(stella, None), &tables), "Stella");
        // Unknown tables fall back to the URL's last segment, as Select does.
        assert_eq!(
            bulk_scope_breadcrumb(&scope("https://other.test/table.html", Some("3")), &tables),
            "table.html > 3"
        );
    }
}
