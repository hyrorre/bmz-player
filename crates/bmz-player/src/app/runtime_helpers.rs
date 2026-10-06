use super::*;

pub(super) fn surface_size_for_window(window: &Window) -> SurfaceSize {
    let size = window.inner_size();
    SurfaceSize { width: size.width, height: size.height }
}

pub(super) fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn now_unix_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis()).unwrap_or(0)
}

pub(super) fn next_screenshot_path(config_dir: &str, data_dir: &Path) -> PathBuf {
    let dir = screenshot_dir(config_dir, data_dir);
    let stamp = now_unix_millis();
    for index in 0..1000 {
        let file_name = if index == 0 {
            format!("bmz-screenshot-{stamp}.png")
        } else {
            format!("bmz-screenshot-{stamp}-{index}.png")
        };
        let path = dir.join(file_name);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("bmz-screenshot-{stamp}-overflow.png"))
}

/// 左上オーバーレイ文字列を決める。
///
/// 撮影フレーム (`hide_toast`) ではトーストを隠し、連続撮影時の写り込みを防ぐ。
pub(super) fn resolve_left_overlay_text(
    hide_toast: bool,
    toast: Option<(&str, Duration)>,
    fallback: &str,
) -> String {
    if !hide_toast
        && let Some((message, age)) = toast
        && age < LEFT_OVERLAY_TOAST_DURATION
        && !message.is_empty()
    {
        return message.to_string();
    }
    fallback.to_string()
}

pub(super) fn pack_scan_progress(progress: ScanProgress) -> u64 {
    (u64::from(progress.done) << 32) | u64::from(progress.total)
}

pub(super) fn unpack_scan_progress(packed: u64) -> ScanProgress {
    ScanProgress { done: (packed >> 32) as u32, total: packed as u32 }
}

pub(super) fn screenshot_dir(config_dir: &str, data_dir: &Path) -> PathBuf {
    let trimmed = config_dir.trim();
    let path = if trimmed.is_empty() {
        PathBuf::from(crate::config::app_config::default_screenshot_dir())
    } else {
        PathBuf::from(trimmed)
    };
    if path.is_absolute() {
        return path;
    }
    if let Some(relative) = screenshot_dir_legacy_data_relative(&path) {
        data_dir.join(relative)
    } else {
        data_dir.join(path)
    }
}

pub(super) fn screenshot_dir_legacy_data_relative(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    match components.next()? {
        std::path::Component::Normal(part) if part == std::ffi::OsStr::new("data") => {
            Some(components.as_path().to_path_buf())
        }
        _ => None,
    }
}

pub(super) fn deferred_boot_action(
    boot_chart_id: Option<i64>,
    options: &AppOptions,
) -> Option<DeferredBoot> {
    if let Some(chart_id) = boot_chart_id {
        if options.boot_practice {
            return Some(DeferredBoot::Practice {
                chart_id,
                start_time_ms: options.practice_start_ms,
                end_time_ms: options.practice_end_ms,
            });
        }
        return Some(DeferredBoot::Chart {
            chart_id,
            replay_slot: options.boot_replay_slot,
            skip_decide: options.skip_decide,
            score_save_disabled: options.viewer_play,
            start_time_us: options.boot_start_time_us,
            bms_random_seed: options.boot_bms_random_seed,
        });
    }
    if let Some(path) = options.boot_replay_file.clone() {
        return Some(DeferredBoot::ReplayFile { path });
    }
    if let Some(course_id) = options.boot_course_replay_id {
        return Some(DeferredBoot::CourseReplay { course_id });
    }
    options.boot_course_id.map(|course_id| DeferredBoot::Course { course_id })
}

pub(super) fn resolve_boot_chart_id(
    library_db: &crate::storage::library_db::LibraryDatabase,
    options: &AppOptions,
) -> Option<i64> {
    if let Some(path) = options.boot_play_path.as_deref() {
        return lookup_boot_chart_id(library_db, path);
    }
    if options.boot_play_sample {
        return library_db.chart_id_by_title(SAMPLE_PLAYABLE_TITLE).ok().flatten();
    }
    None
}

pub(super) fn lookup_boot_chart_id(
    library_db: &crate::storage::library_db::LibraryDatabase,
    path: &str,
) -> Option<i64> {
    let path_obj = Path::new(path);
    if !crate::chart_source::ChartLocator::parse(path_obj)
        .is_ok_and(|locator| locator.container_path().is_file())
    {
        tracing::warn!(path, "boot chart path not found; starting normally");
        return None;
    }
    match library_db.chart_id_by_chart_file_path(path_obj) {
        Ok(Some(chart_id)) => Some(chart_id),
        Ok(None) => {
            tracing::warn!(path, "boot chart path is not in library; starting normally");
            None
        }
        Err(error) => {
            tracing::error!(%error, path, "failed to resolve boot chart path; starting normally");
            None
        }
    }
}

pub(super) fn log_startup_options(options: &AppOptions) {
    if options.lua_skin_runtime_mode == bmz_skin::LuaSkinRuntimeMode::Compat {
        tracing::info!(arg = LUA_SKIN_RUNTIME_ARG, "Lua skin runtime compatibility mode enabled");
    }
    if let Some(path) = &options.boot_play_path {
        tracing::info!(boot_play_path = %path, "boot chart path specified");
    }
    if options.boot_result_sample {
        tracing::info!(arg = BOOT_RESULT_SAMPLE_ARG, "debug result boot enabled");
    }
    if options.autoplay_on_start {
        tracing::info!(arg = AUTOPLAY_ON_START_ARG, "autoplay enabled for started charts");
    }
    if options.battle_on_start {
        tracing::info!(arg = VIEWER_BATTLE_ARG, "battle mode enabled");
    }
    if let Some(frames) = options.smoke_exit_after_frames {
        tracing::info!(arg = SMOKE_EXIT_AFTER_FRAMES_ARG, frames, "smoke auto-exit enabled");
    }
    if let Some(frames) = options.smoke_exit_after_play_frames {
        tracing::info!(
            arg = SMOKE_EXIT_AFTER_PLAY_FRAMES_ARG,
            frames,
            "smoke play-frame auto-exit enabled"
        );
    }
    if let Some(frames) = options.smoke_exit_after_result_frames {
        tracing::info!(
            arg = SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG,
            frames,
            "smoke result-frame auto-exit enabled"
        );
    }
    if options.smoke_exit_on_result {
        tracing::info!(arg = SMOKE_EXIT_ON_RESULT_ARG, "smoke auto-exit on result enabled");
    }
    if options.boot_practice {
        tracing::info!("practice mode enabled for boot chart");
    }
    if let Some(path) = &options.smoke_screenshot_path {
        tracing::info!(arg = SMOKE_SCREENSHOT_ARG, path, "smoke screenshot enabled");
    }
}
