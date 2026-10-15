use super::*;

pub(super) fn should_bypass_analog_scratch_bounce(
    event: &crate::input::gamepad::GamepadButtonEvent,
    play_binding: Option<&LaneBinding>,
) -> bool {
    if !event.synthesized_analog_axis {
        return false;
    }
    let Some(play_binding) = play_binding else { return false };
    let control = PhysicalControl::GamepadButton(event.name.clone());
    play_binding
        .resolve_entry(event.device_id, &control)
        .is_some_and(|binding| matches!(binding.lane, Lane::Scratch | Lane::Scratch2))
}

pub(super) fn should_play_select_bgm_on_enter(
    select_preview_playing: bool,
    system_sound_load_pending: bool,
) -> bool {
    !select_preview_playing && !system_sound_load_pending
}

pub(super) fn take_select_enter(
    pending: &mut bool,
    in_select: bool,
    loading: bool,
    ready: bool,
    viewer_waiting: bool,
) -> bool {
    if !in_select || loading || !ready {
        return false;
    }
    std::mem::take(pending) && !viewer_waiting
}

pub(super) fn should_shuffle_system_sound_sets_on_scene_enter(
    previous: Option<AppSceneKind>,
    next: AppSceneKind,
) -> bool {
    next == AppSceneKind::Select && previous.is_some_and(|scene| scene != AppSceneKind::Select)
}

/// `head` の後ろに [`SoundType::RESULT_BGMS`] を連結した配列を作る。`M` は合計長。
const fn with_result_bgms<const N: usize, const M: usize>(
    head: [crate::system_sound::SoundType; N],
) -> [crate::system_sound::SoundType; M] {
    use crate::system_sound::SoundType;
    assert!(N + SoundType::RESULT_BGMS.len() == M);
    let mut sounds = [SoundType::Select; M];
    let mut index = 0;
    while index < N {
        sounds[index] = head[index];
        index += 1;
    }
    let mut index = 0;
    while index < SoundType::RESULT_BGMS.len() {
        sounds[N + index] = SoundType::RESULT_BGMS[index];
        index += 1;
    }
    sounds
}

pub(super) fn system_bgm_stop_targets_on_scene_enter(
    scene_kind: AppSceneKind,
) -> &'static [crate::system_sound::SoundType] {
    use crate::system_sound::SoundType;
    static PLAY: [SoundType; 9] = with_result_bgms([SoundType::Select, SoundType::SelectEnter]);
    static NON_PLAY: [SoundType; 10] =
        with_result_bgms([SoundType::Select, SoundType::SelectEnter, SoundType::Decide]);
    match scene_kind {
        AppSceneKind::Play => &PLAY,
        AppSceneKind::Select | AppSceneKind::Decide | AppSceneKind::Result => &NON_PLAY,
    }
}

pub(super) fn select_preview_fade_name(fade: SelectPreviewFade) -> &'static str {
    match fade {
        SelectPreviewFade::Silent => "silent",
        SelectPreviewFade::FadingIn { .. } => "fading_in",
        SelectPreviewFade::Playing => "playing",
        SelectPreviewFade::FadingOut { .. } => "fading_out",
    }
}

pub(super) fn select_preview_key_after_delay(
    key: Option<String>,
    current_source: Option<&str>,
    elapsed: Duration,
    delay: Duration,
) -> Option<String> {
    if elapsed >= delay || key.as_deref() == current_source { key } else { None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AudioOutputIssueCause {
    StreamError,
    CallbackLockContention,
    CommandContention,
    GeneratedPreviewCpuPressure,
    CallbackDeadlineExceeded,
    MixClipping,
    LateSchedule,
    Unknown,
}

impl AudioOutputIssueCause {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::StreamError => "stream_error",
            Self::CallbackLockContention => "callback_lock_contention",
            Self::CommandContention => "command_contention",
            Self::GeneratedPreviewCpuPressure => "generated_preview_cpu_pressure",
            Self::CallbackDeadlineExceeded => "callback_deadline_exceeded",
            Self::MixClipping => "mix_clipping",
            Self::LateSchedule => "late_schedule",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AudioOutputIssueMetrics {
    pub(super) stream_errors: u64,
    pub(super) source_lock_misses: u64,
    pub(super) engine_lock_misses: u64,
    pub(super) command_drops: u64,
    pub(super) command_engine_lock_misses: u64,
    pub(super) callback_over_budget: bool,
    pub(super) clipped_samples: u64,
    pub(super) generated_preview_loading: bool,
    pub(super) late_schedule: bool,
}

pub(super) fn classify_audio_output_issue(
    metrics: AudioOutputIssueMetrics,
) -> AudioOutputIssueCause {
    if metrics.stream_errors != 0 {
        AudioOutputIssueCause::StreamError
    } else if metrics.source_lock_misses != 0 || metrics.engine_lock_misses != 0 {
        AudioOutputIssueCause::CallbackLockContention
    } else if metrics.command_drops != 0 || metrics.command_engine_lock_misses != 0 {
        AudioOutputIssueCause::CommandContention
    } else if metrics.callback_over_budget && metrics.generated_preview_loading {
        AudioOutputIssueCause::GeneratedPreviewCpuPressure
    } else if metrics.callback_over_budget {
        AudioOutputIssueCause::CallbackDeadlineExceeded
    } else if metrics.late_schedule {
        AudioOutputIssueCause::LateSchedule
    } else if metrics.clipped_samples != 0 {
        AudioOutputIssueCause::MixClipping
    } else {
        AudioOutputIssueCause::Unknown
    }
}

/// Suspect late schedule from one reporting interval only.
///
/// Threshold is ~2 ms (`sample_rate / 500`, at least one frame). The maximum must
/// be the peak late distance inside that interval across all sources, not a
/// lifetime max — otherwise a single old spike keeps classifying later tiny delays
/// as `late_schedule`. The cumulative late-frame delta is not required: it can
/// shrink to zero when sources are added or removed within the interval.
pub(super) fn late_schedule_from_interval(interval_max_late_frames: u64, sample_rate: u32) -> bool {
    let threshold = (u64::from(sample_rate) / 500).max(1);
    interval_max_late_frames >= threshold
}

pub(super) fn select_preview_normalization_gain(enabled: bool, analyzed_gain: f32) -> f32 {
    if enabled && analyzed_gain.is_finite() { analyzed_gain.clamp(0.0, 1.0) } else { 1.0 }
}

pub(super) fn should_use_generated_preview(
    preview_file: &str,
    explicit_preview_missing: bool,
) -> bool {
    preview_file.is_empty() || explicit_preview_missing
}

pub(super) fn result_exit_audio_gain(elapsed: Duration, fadeout: Duration) -> f32 {
    let audio_fade = result_exit_audio_fade_duration(fadeout);
    if audio_fade.is_zero() {
        0.0
    } else {
        (1.0 - elapsed.as_secs_f32() / audio_fade.as_secs_f32()).clamp(0.0, 1.0)
    }
}

pub(super) fn result_exit_audio_fade_duration(fadeout: Duration) -> Duration {
    fadeout.min(RESULT_EXIT_AUDIO_FADE)
}

pub(super) fn duration_to_frames(duration: Duration, sample_rate: u32) -> u32 {
    if duration.is_zero() || sample_rate == 0 {
        return 0;
    }
    let frames = duration.as_secs_f64() * f64::from(sample_rate);
    frames.round().clamp(1.0, f64::from(u32::MAX)) as u32
}

pub(super) fn decide_bgm_fade_out_frames(chart_zero_time: TimeUs, sample_rate: u32) -> u32 {
    let ready_lead_us = chart_zero_time.0.saturating_neg().max(0) as u64;
    duration_to_frames(Duration::from_micros(ready_lead_us), sample_rate)
}

pub(super) fn result_exit_system_sounds() -> &'static [crate::system_sound::SoundType] {
    use crate::system_sound::SoundType;
    static SOUNDS: [SoundType; 23] = with_result_bgms([
        SoundType::ResultEnterClear,
        SoundType::ResultEnterFail,
        SoundType::ResultEnterA,
        SoundType::ResultEnterAA,
        SoundType::ResultEnterAAA,
        SoundType::CourseEnterClear,
        SoundType::CourseEnterFail,
        SoundType::ResultClear,
        SoundType::ResultFail,
        SoundType::ResultA,
        SoundType::ResultAA,
        SoundType::ResultAAA,
        SoundType::ResultClose,
        SoundType::CourseClear,
        SoundType::CourseFail,
        SoundType::CourseClose,
    ]);
    &SOUNDS
}

pub(super) fn result_entry_sound_for_clear(
    clear: bmz_core::clear::ClearType,
) -> crate::system_sound::SoundType {
    use crate::system_sound::SoundType;
    if matches!(clear, bmz_core::clear::ClearType::Failed) {
        SoundType::ResultFail
    } else {
        SoundType::ResultClear
    }
}

pub(super) fn result_entry_sound_for_result(
    clear: bmz_core::clear::ClearType,
    ex_score: u32,
    total_notes: u32,
    has_sound: impl Fn(crate::system_sound::SoundType) -> bool,
) -> crate::system_sound::SoundType {
    use crate::system_sound::SoundType;
    if clear == bmz_core::clear::ClearType::Failed {
        return if has_sound(SoundType::ResultBgmFail) {
            SoundType::ResultBgmFail
        } else {
            SoundType::ResultFail
        };
    }
    if clear != bmz_core::clear::ClearType::NoPlay {
        // Result表示と同じrank optionを使い、丸めたスコア率や自己ベストを参照しない。
        let state = bmz_render::skin::SkinDrawState {
            ex_score,
            total_notes,
            result_failed: Some(false),
            ..Default::default()
        };
        let rank_sounds = [
            (300, SoundType::ResultBgmAAA, SoundType::ResultAAA),
            (301, SoundType::ResultBgmAA, SoundType::ResultAA),
            (302, SoundType::ResultBgmA, SoundType::ResultA),
        ]
        .into_iter()
        .find_map(|(option, bgm, se)| {
            bmz_render::skin::lua_main_state_option(option, &[], &state).then_some((bgm, se))
        });
        if let Some((bgm, _)) = rank_sounds.filter(|(bgm, _)| has_sound(*bgm)) {
            return bgm;
        }
        if has_sound(SoundType::ResultBgmClear) {
            return SoundType::ResultBgmClear;
        }
        if let Some((_, se)) = rank_sounds.filter(|(_, se)| has_sound(*se)) {
            return se;
        }
    }
    result_entry_sound_for_clear(clear)
}

/// 明示的な入場SEだけを独立して選ぶ。互換SEは呼び出し側でBGM優先にする。
pub(super) fn result_explicit_enter_sound(
    clear: bmz_core::clear::ClearType,
    ex_score: u32,
    total_notes: u32,
    has_sound: impl Fn(crate::system_sound::SoundType) -> bool,
) -> Option<crate::system_sound::SoundType> {
    use crate::system_sound::SoundType;
    let selected = result_entry_sound_for_result(clear, ex_score, total_notes, |sound| {
        let enter = match sound {
            SoundType::ResultBgmClear => SoundType::ResultEnterClear,
            SoundType::ResultBgmFail => SoundType::ResultEnterFail,
            SoundType::ResultBgmA => SoundType::ResultEnterA,
            SoundType::ResultBgmAA => SoundType::ResultEnterAA,
            SoundType::ResultBgmAAA => SoundType::ResultEnterAAA,
            _ => return false,
        };
        has_sound(enter)
    });
    let enter = match selected {
        SoundType::ResultBgmA => SoundType::ResultEnterA,
        SoundType::ResultBgmAA => SoundType::ResultEnterAA,
        SoundType::ResultBgmAAA => SoundType::ResultEnterAAA,
        SoundType::ResultBgmFail | SoundType::ResultFail => SoundType::ResultEnterFail,
        _ => SoundType::ResultEnterClear,
    };
    has_sound(enter).then_some(enter)
}

pub(super) fn result_entry_sounds(
    clear: bmz_core::clear::ClearType,
    ex_score: u32,
    total_notes: u32,
    has_sound: impl Fn(crate::system_sound::SoundType) -> bool,
    is_bgm: impl Fn(crate::system_sound::SoundType) -> bool,
) -> Vec<crate::system_sound::SoundType> {
    let sound = result_entry_sound_for_result(clear, ex_score, total_notes, &has_sound);
    let enter = result_explicit_enter_sound(clear, ex_score, total_notes, has_sound);
    let mut sounds = enter.into_iter().collect::<Vec<_>>();
    if enter.is_none() || is_bgm(sound) {
        sounds.push(sound);
    }
    sounds
}

pub(super) fn course_result_entry_sounds(
    clear: bmz_core::clear::ClearType,
    has_sound: impl Fn(crate::system_sound::SoundType) -> bool,
) -> Vec<crate::system_sound::SoundType> {
    use crate::system_sound::SoundType;
    let preferred = course_result_entry_sound_for_clear(clear);
    let (enter, bgm) = if preferred == SoundType::CourseFail {
        (SoundType::CourseEnterFail, SoundType::CourseBgmFail)
    } else {
        (SoundType::CourseEnterClear, SoundType::CourseBgmClear)
    };
    let se = if has_sound(enter) {
        enter
    } else if has_sound(preferred) {
        preferred
    } else {
        result_entry_sound_for_clear(clear)
    };
    let mut sounds = vec![se];
    if has_sound(bgm) {
        sounds.push(bgm);
    }
    sounds
}

pub(super) fn should_apply_system_sound_load(
    scene: AppSceneKind,
    set_already_applied: bool,
) -> bool {
    scene == AppSceneKind::Select || !set_already_applied
}

pub(super) fn should_apply_system_sound_result(
    scene: AppSceneKind,
    set_already_applied: bool,
    normalization_only: bool,
) -> bool {
    normalization_only || should_apply_system_sound_load(scene, set_already_applied)
}

pub(super) fn result_entry_clear_type_for_sound(
    finished: &FinishedPlaySession,
) -> bmz_core::clear::ClearType {
    finished.result.clear_type
}

pub(super) fn course_result_entry_sound_for_clear(
    clear: bmz_core::clear::ClearType,
) -> crate::system_sound::SoundType {
    use crate::system_sound::SoundType;
    if matches!(clear, bmz_core::clear::ClearType::Failed) {
        SoundType::CourseFail
    } else {
        SoundType::CourseClear
    }
}

pub(super) fn result_exit_sound_for_context(
    is_course_result: bool,
    course_close_available: bool,
) -> crate::system_sound::SoundType {
    use crate::system_sound::SoundType;

    if is_course_result && course_close_available {
        SoundType::CourseClose
    } else {
        SoundType::ResultClose
    }
}

pub(super) fn should_route_settings_key_event(
    state: ElementState,
    repeat: bool,
    settings_editing: bool,
) -> bool {
    state == ElementState::Pressed && (settings_editing || !repeat)
}

pub(super) fn settings_browse_move_control(
    control: &str,
    bindings: &SettingsBindings,
    select_bindings: &SelectKeyBindings,
) -> Option<SelectMove> {
    match control {
        "ArrowUp" | "DPadUp" | "ScratchUp" => Some(SelectMove::Previous),
        "ArrowDown" | "DPadDown" | "ScratchDown" => Some(SelectMove::Next),
        _ if select_bindings.is_select_scratch_up(control) => Some(SelectMove::Previous),
        _ if select_bindings.is_select_scratch_down(control) => Some(SelectMove::Next),
        _ if bindings.is_increase(control) => Some(SelectMove::Next),
        _ if bindings.is_decrease(control) => Some(SelectMove::Previous),
        _ => None,
    }
}

pub(super) fn settings_edit_direction_from_analog_scroll(mov: i32) -> i32 {
    mov.signum()
}

pub(super) fn settings_edit_direction_from_mouse_wheel(delta: MouseScrollDelta) -> i32 {
    mouse_wheel_y(delta).signum() as i32
}

pub(super) fn system_sound_catalog_from_boot(
    boot: &BootstrappedApp,
) -> crate::system_sound::SoundSetCatalog {
    let cfg = &boot.profile_config.system_sound;
    let bgm_candidates = if cfg.bgm_dir.is_empty() {
        Vec::new()
    } else {
        crate::system_sound::scan_bgm_sound_sets(Path::new(&cfg.bgm_dir))
    };
    let se_candidates = if cfg.se_dir.is_empty() {
        Vec::new()
    } else {
        crate::system_sound::scan_sound_sets(
            Path::new(&cfg.se_dir),
            crate::system_sound::SoundType::ResultClear.file_name(),
        )
    };
    let default_dir = if cfg.default_sound_dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(&cfg.default_sound_dir))
    };
    crate::system_sound::SoundSetCatalog {
        bgm_roots: bgm_candidates,
        se_dirs: se_candidates,
        default_dir,
    }
}

pub(super) fn system_sound_selection_from_catalog(
    catalog: &crate::system_sound::SoundSetCatalog,
) -> crate::system_sound::SoundSetSelection {
    let selection = catalog.select_random();
    tracing::info!(
        bgm_dir = ?selection.bgm_dir,
        bgm_variant_dir = ?selection.bgm_variant_dir,
        se_dir = ?selection.se_dir,
        "selected system sound sets"
    );
    selection
}

pub(super) fn system_sound_volume_from_mix(
    mix: &crate::config::profile_config::AudioMixConfig,
    is_bgm: bool,
) -> f32 {
    let unit = if is_bgm { mix.system_bgm_volume } else { mix.system_se_volume };
    let volume = crate::config::play::volume_unit_to_f32(mix.master_volume)
        * crate::config::play::volume_unit_to_f32(unit);
    volume.clamp(0.0, 1.0)
}
