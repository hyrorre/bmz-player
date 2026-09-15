use super::*;

pub(super) fn build_update_dialog(
    ctx: &egui::Context,
    dialog: UpdateDialog<'_>,
    text: Localizer,
) -> Option<UpdateDialogAction> {
    let mut action = None;
    egui::Window::new(tr!(text, "update-title"))
        .id(egui::Id::new("update_dialog"))
        .collapsible(false)
        .resizable(false)
        .default_width(440.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| match dialog {
            UpdateDialog::Available(candidate) => {
                ui.heading(tr!(
                    text,
                    "update-available",
                    "version" => candidate.version.as_str()
                ));
                ui.label(tr!(
                    text,
                    "update-current-version",
                    "version" => current_version()
                ));
                if let Some(published_at) = candidate.published_at.as_deref() {
                    ui.label(tr!(text, "update-published-at", "date" => published_at));
                }
                if let Some(asset) = candidate.asset.as_ref() {
                    ui.label(tr!(text, "update-asset", "asset" => asset.name.as_str()));
                    ui.label(update_asset_kind_label(asset.kind, text));
                } else {
                    ui.label(tr!(text, "update-no-compatible-asset"));
                }
                if let Some(body) = release_body_excerpt(&candidate.body) {
                    ui.separator();
                    ui.label(body);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let can_update = candidate.asset.is_some();
                    if ui
                        .add_enabled(can_update, egui::Button::new(tr!(text, "update-button")))
                        .clicked()
                    {
                        action = Some(UpdateDialogAction::Update);
                    }
                    if ui.button(tr!(text, "update-not-now")).clicked() {
                        action = Some(UpdateDialogAction::NotNow);
                    }
                    if ui.button(tr!(text, "update-skip-release")).clicked() {
                        action = Some(UpdateDialogAction::SkipRelease);
                    }
                });
                if ui.button(tr!(text, "update-open-release-page")).clicked() {
                    action = Some(UpdateDialogAction::OpenReleasePage);
                }
            }
            UpdateDialog::Downloading(candidate, progress) => {
                ui.heading(tr!(
                    text,
                    "update-downloading",
                    "version" => candidate.version.as_str()
                ));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(if progress.extracting.load(std::sync::atomic::Ordering::Relaxed) {
                        tr!(text, "update-extracting")
                    } else {
                        tr!(text, "update-fetching-asset")
                    });
                });
                let received = progress.received.load(std::sync::atomic::Ordering::Relaxed);
                let total = progress.total.load(std::sync::atomic::Ordering::Relaxed);
                if total > 0 {
                    ui.add(
                        egui::ProgressBar::new((received as f64 / total as f64).min(1.0) as f32)
                            .show_percentage(),
                    );
                }
                if ui.button(tr!(text, "common-cancel")).clicked() {
                    action = Some(UpdateDialogAction::Cancel);
                }
                if let Some(asset) = candidate.asset.as_ref() {
                    ui.label(tr!(text, "update-asset", "asset" => asset.name.as_str()));
                }
            }
            UpdateDialog::Ready(candidate) => {
                ui.heading(tr!(text, "update-available", "version" => candidate.version.as_str()));
                ui.label(tr!(text, "update-ready"));
                if ui.button(tr!(text, "update-install-restart")).clicked() {
                    action = Some(UpdateDialogAction::Install);
                }
                if ui.button(tr!(text, "common-cancel")).clicked() {
                    action = Some(UpdateDialogAction::Cancel);
                }
            }
            UpdateDialog::Preparing(candidate) => {
                ui.label(tr!(text, "update-available", "version" => candidate.version.as_str()));
                ui.spinner();
                ui.label(tr!(text, "update-preparing"));
            }
            UpdateDialog::Error { message, candidate } => {
                ui.heading(tr!(text, "update-check-failed"));
                ui.colored_label(egui::Color32::LIGHT_RED, message);
                ui.separator();
                ui.horizontal(|ui| {
                    if candidate.is_some() && ui.button(tr!(text, "update-retry")).clicked() {
                        action = Some(UpdateDialogAction::Update);
                    }
                    if ui.button(tr!(text, "common-close")).clicked() {
                        action = Some(UpdateDialogAction::NotNow);
                    }
                    if candidate.is_some()
                        && ui.button(tr!(text, "update-open-release-page")).clicked()
                    {
                        action = Some(UpdateDialogAction::OpenReleasePage);
                    }
                });
            }
            UpdateDialog::UpToDate => {
                ui.heading(tr!(text, "update-up-to-date"));
                ui.label(tr!(
                    text,
                    "update-current-version",
                    "version" => current_version()
                ));
                if ui.button(tr!(text, "common-close")).clicked() {
                    action = Some(UpdateDialogAction::NotNow);
                }
            }
        });
    action
}

pub(super) fn release_body_excerpt(body: &str) -> Option<String> {
    let mut lines =
        body.lines().map(str::trim).filter(|line| !line.is_empty()).take(6).collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let mut text = lines.join("\n");
    const MAX_LEN: usize = 480;
    if text.len() > MAX_LEN {
        text = text.chars().take(MAX_LEN).collect();
        text.push_str("...");
    } else if body.lines().filter(|line| !line.trim().is_empty()).count() > lines.len() {
        text.push_str("\n...");
    }
    lines.clear();
    Some(text)
}

pub(super) fn update_asset_kind_label(kind: UpdateAssetKind, text: Localizer) -> String {
    match kind {
        UpdateAssetKind::WindowsInstaller => tr!(text, "update-kind-windows-installer"),
        UpdateAssetKind::WindowsPortable => tr!(text, "update-kind-windows-portable"),
        UpdateAssetKind::MacosAppZip if crate::update::sparkle::available() => {
            tr!(text, "update-kind-macos-automatic")
        }
        UpdateAssetKind::MacosAppZip => tr!(text, "update-kind-macos-manual"),
        UpdateAssetKind::Other => tr!(text, "update-kind-manual"),
    }
}
