use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum DebugLogFilter {
    All,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl DebugLogFilter {
    const ALL: [Self; 6] =
        [Self::All, Self::Error, Self::Warn, Self::Info, Self::Debug, Self::Trace];

    fn label(self, text: Localizer) -> String {
        match self {
            Self::All => tr!(text, "debug-log-filter-all"),
            Self::Error => tr!(text, "debug-log-filter-error"),
            Self::Warn => tr!(text, "debug-log-filter-warn"),
            Self::Info => tr!(text, "debug-log-filter-info"),
            Self::Debug => tr!(text, "debug-log-filter-debug"),
            Self::Trace => tr!(text, "debug-log-filter-trace"),
        }
    }

    const fn minimum_level(self) -> Option<TracingLogLevel> {
        match self {
            Self::All => None,
            Self::Error => Some(TracingLogLevel::Error),
            Self::Warn => Some(TracingLogLevel::Warn),
            Self::Info => Some(TracingLogLevel::Info),
            Self::Debug => Some(TracingLogLevel::Debug),
            Self::Trace => Some(TracingLogLevel::Trace),
        }
    }

    pub(super) fn allows(self, level: TracingLogLevel) -> bool {
        self.minimum_level().is_none_or(|minimum| level >= minimum)
    }
}

pub(super) fn log_level_color(level: TracingLogLevel) -> egui::Color32 {
    match level {
        TracingLogLevel::Trace => egui::Color32::GRAY,
        TracingLogLevel::Debug => egui::Color32::LIGHT_BLUE,
        TracingLogLevel::Info => egui::Color32::LIGHT_GREEN,
        TracingLogLevel::Warn => egui::Color32::YELLOW,
        TracingLogLevel::Error => egui::Color32::LIGHT_RED,
    }
}

pub(super) fn localized_log_message(entry: &LogEntry, text: Localizer) -> String {
    if entry.message.is_empty() { tr!(text, "debug-log-no-message") } else { entry.message.clone() }
}

pub(super) fn format_log_entry(entry: &LogEntry, text: Localizer) -> String {
    format!("[{}] {} {}", entry.level.as_str(), entry.target, localized_log_message(entry, text))
}

/// FPS / フレーム時間 / シーン / 解像度 / tracing ログを表示するデバッグパネル。
pub(super) fn build_debug_page(
    ui: &mut egui::Ui,
    info: &DebugInfo,
    log_buffer: &LogBuffer,
    debug_log_filter: &mut DebugLogFilter,
    debug_log_autoscroll: &mut bool,
    text: Localizer,
) {
    let dt = ui.ctx().input(|i| i.stable_dt);
    egui::Grid::new("debug_grid").num_columns(2).show(ui, |ui| {
        ui.label("FPS");
        ui.label(info.current_fps.to_string());
        ui.end_row();
        ui.label(tr!(text, "debug-frame-time"));
        ui.label(format!("{:.2} ms", dt * 1000.0));
        ui.end_row();
        ui.label(tr!(text, "debug-scene"));
        ui.label(info.scene);
        ui.end_row();
        ui.label(tr!(text, "debug-resolution"));
        ui.label(format!("{} x {}", info.width, info.height));
        ui.end_row();
        ui.label(tr!(text, "debug-present-mode"));
        ui.label(
            info.effective_present_mode
                .map_or_else(|| tr!(text, "debug-uninitialized"), ToString::to_string),
        );
        ui.end_row();
        ui.label(tr!(text, "debug-max-frame-latency"));
        ui.label(
            info.maximum_frame_latency
                .map_or_else(|| tr!(text, "debug-uninitialized"), |latency| latency.to_string()),
        );
        ui.end_row();
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(tr!(text, "debug-log"));
        egui::ComboBox::from_id_salt("debug_log_filter")
            .selected_text(debug_log_filter.label(text))
            .show_ui(ui, |ui| {
                for filter in DebugLogFilter::ALL {
                    ui.selectable_value(debug_log_filter, filter, filter.label(text));
                }
            });
        ui.checkbox(debug_log_autoscroll, tr!(text, "debug-log-autoscroll"));
    });

    let entries = log_buffer.snapshot();
    let visible_entries =
        entries.iter().filter(|entry| debug_log_filter.allows(entry.level)).collect::<Vec<_>>();
    let mut copy_requested = false;
    let mut clear_requested = false;
    ui.horizontal(|ui| {
        ui.small(tr!(
            text,
            "debug-log-count",
            "visible" => visible_entries.len(),
            "total" => entries.len()
        ));
        if ui.button(tr!(text, "common-copy")).clicked() {
            copy_requested = true;
        }
        if ui.button(tr!(text, "debug-log-clear")).clicked() {
            clear_requested = true;
        }
    });

    egui::ScrollArea::vertical()
        .id_salt("debug_log_scroll")
        .max_height(ui.available_height().max(120.0))
        .auto_shrink([false, false])
        .stick_to_bottom(*debug_log_autoscroll)
        .show(ui, |ui| {
            if visible_entries.is_empty() {
                ui.weak(tr!(text, "debug-log-empty"));
            }
            for entry in visible_entries {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(log_level_color(entry.level), entry.level.as_str());
                    ui.weak(format!("{}:", entry.target));
                    ui.label(localized_log_message(entry, text));
                });
            }
        });

    if copy_requested {
        let text = entries
            .iter()
            .filter(|entry| debug_log_filter.allows(entry.level))
            .map(|entry| format_log_entry(entry, text))
            .collect::<Vec<_>>()
            .join("\n");
        ui.ctx().copy_text(text);
    }
    if clear_requested {
        log_buffer.clear();
    }
}
