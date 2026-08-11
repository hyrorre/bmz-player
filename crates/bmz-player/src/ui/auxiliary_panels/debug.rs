use super::*;

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub(super) fn log_level_color(level: TracingLogLevel) -> egui::Color32 {
    match level {
        TracingLogLevel::Trace => egui::Color32::GRAY,
        TracingLogLevel::Debug => egui::Color32::LIGHT_BLUE,
        TracingLogLevel::Info => egui::Color32::LIGHT_GREEN,
        TracingLogLevel::Warn => egui::Color32::YELLOW,
        TracingLogLevel::Error => egui::Color32::LIGHT_RED,
    }
}

#[allow(dead_code)]
pub(super) fn localized_log_message(entry: &LogEntry, text: Localizer) -> String {
    if entry.message.is_empty() { tr!(text, "debug-log-no-message") } else { entry.message.clone() }
}

#[allow(dead_code)]
pub(super) fn format_log_entry(entry: &LogEntry, text: Localizer) -> String {
    format!("[{}] {} {}", entry.level.as_str(), entry.target, localized_log_message(entry, text))
}

/// FPS / フレーム時間 / シーン / 解像度を表示する軽量デバッグパネル。
pub(super) fn build_debug_panel(
    ctx: &egui::Context,
    open: &mut bool,
    info: &DebugInfo,
    text: Localizer,
) {
    localized_sized_panel_window(
        "debug_panel",
        tr!(text, "debug-title"),
        ctx,
        open,
        620.0,
        500.0,
        egui::pos2(16.0, 140.0),
    )
    .show(ctx, |ui| {
        scrollable_window_content(ui, |ui| {
            let dt = ctx.input(|i| i.stable_dt);
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
                ui.label(info.maximum_frame_latency.map_or_else(
                    || tr!(text, "debug-uninitialized"),
                    |latency| latency.to_string(),
                ));
                ui.end_row();
            });
        });
    });
}
