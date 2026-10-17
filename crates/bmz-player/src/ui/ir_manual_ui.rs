//! IR設定ページの「スコアの一括操作」と、IR送信キュー画面。
//!
//! 実行・世代管理・DB読み込みは `ir::manual_job::IrManualHandle` が持つ。ここは状態の
//! 表示と要求の発行だけを行い、UIスレッドでSQLiteを開かない。

use std::time::Duration;

use crate::ir::manual_job::{
    IR_QUEUE_PAGE_SIZE, IrEstimateState, IrManualHandle, IrManualOp, IrManualOutcomeStatus,
    IrManualRequest, IrManualSnapshot, IrQueueQuery, IrQueueState, IrQueueStatusFilter,
    outcome_message,
};
use crate::storage::common::hash_to_hex;
use crate::storage::network_db::IrJobKind;

use super::*;

const REPAINT_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IrConfirm {
    Upload,
    Download,
    ResendFailed,
}

pub(super) struct IrManualUi {
    pub(super) handle: IrManualHandle,
    confirm: Option<IrConfirm>,
    queue_status: IrQueueStatusFilter,
    queue_page: u32,
    /// 直近に発行した一覧要求。同じ条件を毎フレーム発行しないための照合用。
    queue_requested: Option<IrQueueQuery>,
}

impl IrManualUi {
    pub(super) fn new(handle: IrManualHandle) -> Self {
        Self {
            handle,
            confirm: None,
            queue_status: IrQueueStatusFilter::default(),
            queue_page: 0,
            queue_requested: None,
        }
    }

    /// profile切替後に、旧profileの画面状態を捨てる。
    pub(super) fn reset(&mut self) {
        self.confirm = None;
        self.queue_status = IrQueueStatusFilter::default();
        self.queue_page = 0;
        self.queue_requested = None;
    }

    /// 送信キューを表示しないフレームで呼び、次に開いたとき一覧を取り直す。
    pub(super) fn queue_hidden(&mut self) {
        self.queue_requested = None;
    }
}

/// 一括操作の対象（primary provider）。
struct IrBulkTarget {
    provider_key: String,
    account_id: String,
    label: String,
    can_upload: bool,
    can_download: bool,
}

fn primary_target(ir: &IrConfig) -> Option<IrBulkTarget> {
    let provider = crate::ir::provider_key::primary_provider_config(ir)?;
    let provider_key = crate::ir::provider_key::configured_provider_key(provider)?.to_string();
    let account_id = provider.account_id.trim().to_string();
    if account_id.is_empty() {
        return None;
    }
    let is_rian = crate::ir::rian_ir::is_rian_ir_config(provider);
    let is_bms_ir = crate::ir::bms_ir::is_bms_ir_config(provider);
    Some(IrBulkTarget {
        label: provider_key.clone(),
        provider_key,
        account_id,
        // ローカルスコアの一括送信はBMZ IRのみ。スコア取得はrianIRに無い。
        can_upload: !is_rian && !is_bms_ir,
        can_download: !is_rian,
    })
}

fn format_duration_short(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// IR設定ページ内の「スコアの一括操作」。
pub(super) fn build_ir_bulk_operations(
    ui: &mut egui::Ui,
    ir: &IrConfig,
    ir_manual: &mut IrManualUi,
    text: Localizer,
) {
    ui.separator();
    ui.strong(tr!(text, "ir-bulk-title"));
    ui.small(tr!(text, "ir-bulk-description"));
    let snapshot = ir_manual.handle.snapshot();
    let busy = snapshot.running.is_some();
    let Some(target) = primary_target(ir) else {
        ui.small(tr!(text, "ir-bulk-no-provider"));
        return;
    };
    ui.horizontal_wrapped(|ui| {
        let upload = ui.add_enabled(
            !busy && target.can_upload,
            egui::Button::new(tr!(text, "ir-bulk-upload-button")),
        );
        let upload = if target.can_upload {
            upload
        } else {
            upload.on_disabled_hover_text(tr!(
                text,
                "ir-bulk-unsupported-upload",
                "provider" => target.label.clone()
            ))
        };
        if upload.clicked() {
            ir_manual.confirm = Some(IrConfirm::Upload);
            ir_manual.handle.request(IrManualRequest::EstimateUpload);
        }
        let download = ui.add_enabled(
            !busy && target.can_download,
            egui::Button::new(tr!(text, "ir-bulk-download-button")),
        );
        let download = if target.can_download {
            download
        } else {
            download.on_disabled_hover_text(tr!(
                text,
                "ir-bulk-unsupported-download",
                "provider" => target.label.clone()
            ))
        };
        if download.clicked() {
            ir_manual.confirm = Some(IrConfirm::Download);
        }
    });
    build_ir_manual_status(ui, &snapshot, &ir_manual.handle, text);
}

/// 実行中の進捗と、直近の結果。
fn build_ir_manual_status(
    ui: &mut egui::Ui,
    snapshot: &IrManualSnapshot,
    handle: &IrManualHandle,
    text: Localizer,
) {
    if let Some(running) = &snapshot.running {
        let progress = &running.progress;
        ui.horizontal_wrapped(|ui| {
            ui.spinner();
            ui.label(match &running.op {
                IrManualOp::UploadLocalAll => tr!(
                    text,
                    "ir-bulk-running-upload",
                    "batch" => progress.batch,
                    "submitted" => progress.submitted,
                    "failed" => progress.failed,
                    "remaining" => progress.remaining,
                ),
                IrManualOp::DownloadScores => tr!(
                    text,
                    "ir-bulk-running-download",
                    "pages" => progress.download_pages,
                    "imported" => progress.download_imported,
                    "linked" => progress.download_linked,
                    "skipped" => progress.download_skipped,
                ),
                IrManualOp::ResendJob { .. } | IrManualOp::ResendFailed { .. } => tr!(
                    text,
                    "ir-bulk-running-resend",
                    "submitted" => progress.submitted,
                    "failed" => progress.failed,
                    "index" => progress.job_index,
                    "limit" => progress.job_limit,
                ),
            });
        });
        if progress.waiting_for_lease {
            ui.small(tr!(text, "ir-bulk-waiting-lease"));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !progress.cancel_requested,
                    egui::Button::new(tr!(text, "ir-bulk-cancel")),
                )
                .clicked()
            {
                handle.cancel();
            }
            if progress.cancel_requested {
                ui.small(tr!(text, "ir-bulk-cancelling"));
            }
        });
        ui.small(tr!(text, "ir-bulk-profile-locked"));
        ui.ctx().request_repaint_after(REPAINT_INTERVAL);
    } else if let Some(outcome) = &snapshot.last_outcome {
        let color = match outcome.status {
            IrManualOutcomeStatus::Completed => egui::Color32::LIGHT_GREEN,
            IrManualOutcomeStatus::Cancelled => ui.visuals().text_color(),
            _ => egui::Color32::LIGHT_RED,
        };
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(color, outcome_message(text, outcome));
            if ui.small_button(tr!(text, "common-close")).clicked() {
                handle.clear_outcome();
            }
        });
        for message in outcome.messages.iter().take(3) {
            ui.small(message);
        }
    }
}

/// 確認モーダル。一括操作・送信キューのどちらから開いても、ここで1回だけ描く。
pub(super) fn build_ir_confirm_modal(
    ctx: &egui::Context,
    ir: &IrConfig,
    ir_manual: &mut IrManualUi,
    text: Localizer,
) {
    let Some(confirm) = ir_manual.confirm else {
        return;
    };
    let snapshot = ir_manual.handle.snapshot();
    let Some(target) = primary_target(ir) else {
        ir_manual.confirm = None;
        return;
    };
    let busy = snapshot.running.is_some();
    let mut start = None;
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new("ir_manual_confirm")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading(match confirm {
            IrConfirm::Upload => tr!(text, "ir-bulk-upload-confirm-title"),
            IrConfirm::Download => tr!(text, "ir-bulk-download-confirm-title"),
            IrConfirm::ResendFailed => tr!(text, "ir-queue-resend-all-confirm-title"),
        });
        ui.label(tr!(
            text,
            "ir-bulk-target",
            "provider" => target.label.clone(),
            "account" => target.account_id.clone(),
        ));
        ui.add_space(6.0);
        let mut can_start = !busy;
        match confirm {
            IrConfirm::Upload => match &snapshot.estimate {
                IrEstimateState::Idle | IrEstimateState::Counting => {
                    can_start = false;
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr!(text, "ir-bulk-counting"));
                    });
                }
                IrEstimateState::Ready(estimate) => {
                    if estimate.total_jobs() == 0 {
                        can_start = false;
                        ui.label(tr!(text, "ir-bulk-upload-nothing"));
                    } else {
                        ui.label(tr!(
                            text,
                            "ir-bulk-upload-estimate",
                            "total" => estimate.total_jobs(),
                            "queued" => estimate.already_queued,
                            "duration" => format_duration_short(estimate.minimum_duration()),
                        ));
                    }
                }
                IrEstimateState::Failed(error) => {
                    can_start = false;
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        tr!(text, "ir-bulk-estimate-failed", "error" => error.clone()),
                    );
                }
            },
            IrConfirm::Download => {
                ui.label(tr!(text, "ir-bulk-download-notes"));
            }
            IrConfirm::ResendFailed => {
                let failed = match &snapshot.queue {
                    IrQueueState::Ready(view) => view.counts.failed,
                    _ => 0,
                };
                can_start &= failed > 0;
                ui.label(tr!(text, "ir-queue-resend-all-notes", "count" => failed));
            }
        }
        if confirm == IrConfirm::Upload {
            ui.label(tr!(text, "ir-bulk-upload-notes"));
        }
        ui.small(tr!(text, "ir-bulk-profile-locked"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(can_start, egui::Button::new(tr!(text, "ir-bulk-start"))).clicked() {
                start = Some(match confirm {
                    IrConfirm::Upload => IrManualOp::UploadLocalAll,
                    IrConfirm::Download => IrManualOp::DownloadScores,
                    IrConfirm::ResendFailed => IrManualOp::ResendFailed {
                        provider_key: target.provider_key.clone(),
                        account_id: target.account_id.clone(),
                    },
                });
            }
            if ui.button(tr!(text, "common-cancel")).clicked() {
                close = true;
            }
        });
    });
    if response.should_close() {
        close = true;
    }
    if let Some(op) = start {
        ir_manual.handle.request(IrManualRequest::Run(op));
        close = true;
    }
    if close {
        ir_manual.confirm = None;
        ir_manual.handle.clear_estimate();
    }
}

fn status_filter_label(filter: IrQueueStatusFilter, text: Localizer) -> String {
    match filter {
        IrQueueStatusFilter::Failed => tr!(text, "ir-queue-filter-failed"),
        IrQueueStatusFilter::Pending => tr!(text, "ir-queue-filter-pending"),
        IrQueueStatusFilter::Sending => tr!(text, "ir-queue-filter-sending"),
        IrQueueStatusFilter::Unfinished => tr!(text, "ir-queue-filter-unfinished"),
    }
}

fn job_kind_label(kind: IrJobKind, text: Localizer) -> String {
    match kind {
        IrJobKind::Score => tr!(text, "ir-queue-kind-score"),
        IrJobKind::Course => tr!(text, "ir-queue-kind-course"),
        IrJobKind::Replay => tr!(text, "ir-queue-kind-replay"),
        IrJobKind::Attestation => tr!(text, "ir-queue-kind-attestation"),
    }
}

fn job_status_label(status: &str, text: Localizer) -> String {
    match status {
        "failed" => tr!(text, "ir-queue-filter-failed"),
        "pending" => tr!(text, "ir-queue-filter-pending"),
        "sending" => tr!(text, "ir-queue-filter-sending"),
        other => other.to_string(),
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    let single_line = value.replace(['\r', '\n'], " ");
    if single_line.chars().count() <= max {
        single_line
    } else {
        format!("{}…", single_line.chars().take(max).collect::<String>())
    }
}

/// IR ページの一括操作の下に並べる「IR送信キュー」。
/// 対象 provider が無いときは一括操作側の案内だけを表示し、この節は出さない。
pub(super) fn build_ir_queue_section(
    ui: &mut egui::Ui,
    ir: &IrConfig,
    ir_manual: &mut IrManualUi,
    text: Localizer,
) {
    let Some(target) = primary_target(ir) else {
        ir_manual.queue_hidden();
        return;
    };
    SettingsSection::new(SettingsPage::Ir, tr!(text, "ir-queue-title"))
        .id_salt("settings_ir_queue")
        .show(ui, |ui| build_ir_queue(ui, target, ir_manual, text));
}

fn build_ir_queue(
    ui: &mut egui::Ui,
    target: IrBulkTarget,
    ir_manual: &mut IrManualUi,
    text: Localizer,
) {
    let snapshot = ir_manual.handle.snapshot();
    let busy = snapshot.running.is_some();
    let desired = IrQueueQuery {
        provider_key: target.provider_key.clone(),
        account_id: target.account_id.clone(),
        status: ir_manual.queue_status,
        page: ir_manual.queue_page,
    };
    if ir_manual.queue_requested.as_ref() != Some(&desired) {
        ir_manual.handle.request(IrManualRequest::LoadQueue(desired.clone()));
        ir_manual.queue_requested = Some(desired.clone());
    }
    let view = match &snapshot.queue {
        IrQueueState::Ready(view) => Some(view),
        _ => None,
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!(text, "ir-queue-filter"));
        let mut status = ir_manual.queue_status;
        egui::ComboBox::from_id_salt("ir_queue_filter")
            .selected_text(status_filter_label(status, text))
            .show_ui(ui, |ui| {
                for option in [
                    IrQueueStatusFilter::Failed,
                    IrQueueStatusFilter::Pending,
                    IrQueueStatusFilter::Sending,
                    IrQueueStatusFilter::Unfinished,
                ] {
                    ui.selectable_value(&mut status, option, status_filter_label(option, text));
                }
            });
        if status != ir_manual.queue_status {
            ir_manual.queue_status = status;
            ir_manual.queue_page = 0;
        }
        if ui.button(tr!(text, "ir-queue-refresh")).clicked() {
            ir_manual.handle.request(IrManualRequest::LoadQueue(desired.clone()));
        }
        let failed = view.map_or(0, |view| view.counts.failed);
        if ui
            .add_enabled(!busy && failed > 0, egui::Button::new(tr!(text, "ir-queue-resend-all")))
            .clicked()
        {
            ir_manual.confirm = Some(IrConfirm::ResendFailed);
        }
    });
    if let Some(view) = view {
        ui.small(tr!(
            text,
            "ir-queue-counts",
            "failed" => view.counts.failed,
            "pending" => view.counts.pending,
            "sending" => view.counts.sending,
        ));
    }
    match &snapshot.queue {
        IrQueueState::Idle | IrQueueState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!(text, "ir-queue-loading"));
            });
            ui.ctx().request_repaint_after(REPAINT_INTERVAL);
        }
        IrQueueState::Failed(error) => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                tr!(text, "ir-queue-load-failed", "error" => error.clone()),
            );
        }
        IrQueueState::Ready(view) => {
            if view.rows.is_empty() {
                ui.label(tr!(text, "ir-queue-empty"));
            }
            let pages = view.total.div_ceil(IR_QUEUE_PAGE_SIZE).max(1);
            let mut resend = None;
            egui::ScrollArea::both().auto_shrink([false, false]).max_height(360.0).show(ui, |ui| {
                egui::Grid::new("ir_queue_grid").striped(true).show(ui, |ui| {
                    for header in [
                        "ir-queue-col-kind",
                        "ir-queue-col-chart",
                        "ir-queue-col-status",
                        "ir-queue-col-attempts",
                        "ir-queue-col-next",
                        "ir-queue-col-error",
                        "ir-queue-col-updated",
                    ] {
                        ui.strong(text.text(header));
                    }
                    ui.label("");
                    ui.end_row();
                    for row in &view.rows {
                        let job = &row.job;
                        ui.label(job_kind_label(job.kind, text));
                        let chart = row.title.clone().unwrap_or_else(|| {
                            hash_to_hex(&job.chart_sha256).chars().take(12).collect()
                        });
                        ui.label(truncate_chars(&chart, 36)).on_hover_text(chart);
                        ui.label(job_status_label(&job.status, text));
                        ui.label(job.attempt_count.to_string());
                        ui.label(if job.status == "failed" {
                            format_optional_timestamp(Some(job.next_attempt_at))
                        } else {
                            "-".to_string()
                        });
                        let error = job.last_error.trim();
                        if error.is_empty() {
                            ui.label("-");
                        } else {
                            ui.label(truncate_chars(error, 48)).on_hover_text(error);
                        }
                        ui.label(format_optional_timestamp(Some(job.updated_at)));
                        if job.status == "failed"
                            && ui
                                .add_enabled(!busy, egui::Button::new(tr!(text, "ir-queue-resend")))
                                .clicked()
                        {
                            resend = Some(IrManualOp::ResendJob {
                                provider_key: job.provider.clone(),
                                account_id: job.account_id.clone(),
                                kind: job.kind,
                                local_score_id: job.local_score_id,
                            });
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(op) = resend {
                ir_manual.handle.request(IrManualRequest::Run(op));
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        ir_manual.queue_page > 0,
                        egui::Button::new(tr!(text, "ir-queue-prev")),
                    )
                    .clicked()
                {
                    ir_manual.queue_page -= 1;
                }
                ui.label(tr!(
                    text,
                    "ir-queue-page",
                    "page" => ir_manual.queue_page + 1,
                    "pages" => pages,
                    "total" => view.total,
                ));
                if ui
                    .add_enabled(
                        ir_manual.queue_page + 1 < pages,
                        egui::Button::new(tr!(text, "ir-queue-next")),
                    )
                    .clicked()
                {
                    ir_manual.queue_page += 1;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logged_in(mut provider: IrProviderConfig, key: &str) -> IrConfig {
        provider.provider_key = key.to_string();
        provider.enabled = true;
        provider.account_id = "account-1".to_string();
        IrConfig {
            primary_provider: key.to_string(),
            providers: vec![provider],
            ..IrConfig::default()
        }
    }

    #[test]
    fn bulk_target_follows_the_primary_provider_and_its_capabilities() {
        let bmz = logged_in(IrProviderConfig::bmz_ir(), "bmz-test");
        let target = primary_target(&bmz).unwrap();
        assert_eq!(target.account_id, "account-1");
        assert!(target.can_upload && target.can_download);

        let rian = logged_in(IrProviderConfig::rian_ir(), "rian-test");
        let target = primary_target(&rian).unwrap();
        assert!(!target.can_upload && !target.can_download);

        let bms_ir = logged_in(IrProviderConfig::bms_ir(), "bms-test");
        let target = primary_target(&bms_ir).unwrap();
        assert!(!target.can_upload && target.can_download);

        // 未ログイン（アカウント未確定）や無効のproviderは対象にしない。
        let mut anonymous = logged_in(IrProviderConfig::bmz_ir(), "bmz-test");
        anonymous.providers[0].account_id.clear();
        assert!(primary_target(&anonymous).is_none());
        let mut disabled = logged_in(IrProviderConfig::bmz_ir(), "bmz-test");
        disabled.providers[0].enabled = false;
        assert!(primary_target(&disabled).is_none());
        assert!(primary_target(&IrConfig::default()).is_none());
    }

    #[test]
    fn short_formatters_keep_one_line_and_compact_units() {
        assert_eq!(format_duration_short(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration_short(Duration::from_secs(125)), "2m 5s");
        assert_eq!(format_duration_short(Duration::from_secs(3_900)), "1h 5m");
        assert_eq!(truncate_chars("a\nb", 10), "a b");
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
    }

    #[test]
    fn bulk_section_and_queue_section_render_without_a_provider() {
        let ctx = egui::Context::default();
        let text = Localizer::new(AppLocale::En);
        let mut ui_state = IrManualUi::new(Default::default());
        ui_state.confirm = Some(IrConfirm::Upload);
        let ir = IrConfig::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            build_ir_bulk_operations(ui, &ir, &mut ui_state, text);
            build_ir_queue_section(ui, &ir, &mut ui_state, text);
            build_ir_confirm_modal(ui.ctx(), &ir, &mut ui_state, text);
        });
        // providerが無ければ確認モーダルは閉じ、要求も積まない。
        assert!(ui_state.confirm.is_none());
        assert!(ui_state.handle.drain_requests().is_empty());
    }
}
