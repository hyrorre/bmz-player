use super::*;
use crate::config::download_sources::HttpDownloadPreset;

pub(super) fn build_download_source_settings(
    ui: &mut egui::Ui,
    config: &mut crate::config::app_config::ChartDownloadsConfig,
    text: Localizer,
) {
    let before = config.http_source;
    egui::ComboBox::from_id_salt("http_download_source")
        .selected_text(config.http_source.name())
        .show_ui(ui, |ui| {
            for preset in HttpDownloadPreset::ALL {
                ui.add_enabled_ui(preset.is_available(), |ui| {
                    ui.selectable_value(&mut config.http_source, preset, preset.name());
                });
            }
        });
    if config.http_source != before && config.http_source != HttpDownloadPreset::Custom {
        // Only an explicit source selection replaces the user's custom URL.
        config.http_api_url = config.http_source.api_url().into();
    }
    ui.label(tr!(text, "settings-downloads-source-help"));
    if config.http_source == HttpDownloadPreset::Wriggle {
        ui.colored_label(egui::Color32::YELLOW, tr!(text, "settings-downloads-wriggle-status"));
    }
    let effective = config.effective_http_api_url();
    let requires_md5 = effective.contains("{md5}") || effective.contains("%s");
    let requires_sha256 = effective.contains("{sha256}");
    ui.label(tr!(text, "settings-downloads-required-hashes", "hashes" => match (requires_md5, requires_sha256) { (true,true) => "MD5 + SHA256", (true,false) => "MD5", (false,true) => "SHA256", _ => "—" }));
    ui.add(
        egui::Slider::new(&mut config.http_parallelism, 1..=5)
            .text(tr!(text, "settings-downloads-parallelism")),
    );
}
