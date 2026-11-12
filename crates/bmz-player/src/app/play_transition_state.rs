use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultSkinSlot {
    Normal,
    Course,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum PlayEntryPresentation {
    #[default]
    Normal,
    ViewerSeek,
}

impl PlayEntryPresentation {
    pub(super) const fn is_seamless(self) -> bool {
        matches!(self, Self::ViewerSeek)
    }

    pub(super) const fn shows_ready_presentation(self) -> bool {
        !self.is_seamless()
    }
}

pub(super) const fn play_entry_elapsed_time(
    presentation: PlayEntryPresentation,
    continued_elapsed: TimeUs,
) -> TimeUs {
    if presentation.is_seamless() { continued_elapsed } else { TimeUs(0) }
}

pub(super) fn play_ready_skin_elapsed_time(
    presentation: PlayEntryPresentation,
    elapsed: Option<TimeUs>,
    viewer_offset: TimeUs,
) -> Option<TimeUs> {
    elapsed.map(|elapsed| {
        if presentation.is_seamless() {
            // 常設パーツも READY を参照するため、無効化せず導入演出を通過させる。
            TimeUs(viewer_offset.0.max(100_000_000).saturating_add(elapsed.0))
        } else {
            elapsed
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultSkinClickAction {
    SetPanel(i32),
    SelectIrScope(crate::screens::result_ir::ResultRankingTab),
    ToggleIrScope,
    ToggleFavoriteSong,
    ToggleFavoriteChart,
    SaveReplay(u8),
    ResetDailyStatistics,
}

#[derive(Debug, Clone)]
pub(super) struct TableBreadcrumb {
    pub(super) name: String,
    pub(super) symbol: String,
}

pub(super) fn table_breadcrumb_from_record(table: &DifficultyTableRecord) -> TableBreadcrumb {
    TableBreadcrumb { name: table.name.clone(), symbol: table.symbol.clone() }
}

pub(super) enum DecideLaunch {
    Play,
    Practice(Box<PracticeSession>),
}

impl DecideLaunch {
    pub(super) fn practice(practice: PracticeSession) -> Self {
        Self::Practice(Box::new(practice))
    }

    pub(super) fn into_practice_session(self) -> Option<PracticeSession> {
        match self {
            Self::Play => None,
            Self::Practice(practice) => Some(*practice),
        }
    }
}

pub(super) struct DecideTransition {
    pub(super) chart_id: i64,
    pub(super) options: PlayStartOptions,
    pub(super) launch: DecideLaunch,
    pub(super) started_at: Instant,
    /// The decide exit started (skip, cancel or scene end); `exit` says how it runs.
    pub(super) fadeout_started_at: Option<Instant>,
    pub(super) cancel: bool,
    pub(super) exit: DecideExit,
    pub(super) snapshot: RenderSnapshot,
    pub(super) title_override: Option<DecideTitleOverride>,
}

/// How a started decide exit is drawn and how long it waits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum DecideExit {
    /// Timer 2 for the skin's `fadeout` (cancel, scene end, and skip on JSON / Lua).
    #[default]
    Fadeout,
    /// LR2 skip: timer 3 for `#CLOSE`, or `#FADEOUT`
    /// when the skin has no `#CLOSE`.
    Lr2Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecideTitleOverride {
    pub(super) title: String,
    pub(super) subtitle: String,
}

impl DecideTransition {
    pub(super) fn snapshot_for_render(&self) -> RenderSnapshot {
        let mut snapshot = self.snapshot.clone();
        if let Some(title_override) = &self.title_override {
            snapshot.title.clone_from(&title_override.title);
            snapshot.subtitle.clone_from(&title_override.subtitle);
        }
        snapshot
    }
}

pub(super) struct PendingPlayStart {
    pub(super) chart_id: i64,
    pub(super) options: PlayStartOptions,
    /// 変換済み譜面の静的 skin 値を placeholder snapshot へ反映済みか。
    pub(super) prepared_chart_applied: bool,
    pub(super) play_config_key_mode: KeyMode,
    pub(super) lane: PendingPlayLaneState,
    pub(super) lane_actions: Vec<PlayLaneAction>,
    pub(super) visual_input: PendingPlayVisualInput,
}

impl PendingPlayStart {
    pub(super) fn from_snapshot(
        chart_id: i64,
        options: PlayStartOptions,
        snapshot: &RenderSnapshot,
        profile: &ProfileConfig,
        key_mode: KeyMode,
        play_config_key_mode: KeyMode,
        gamepad_slots: crate::input::gamepad::GamepadSlotMap,
    ) -> Self {
        let playback_rate_percent = if options.playback_rate_percent == 0 {
            100
        } else {
            bmz_audio::clock::clamp_playback_rate_percent(options.playback_rate_percent)
        };
        let binding = crate::config::play::lane_binding_for_chart_with_slots(
            &profile.input,
            key_mode,
            gamepad_slots,
        );
        let mode_config = profile.play_mode_config(play_config_key_mode);
        let hs_fix = options.hs_fix;
        Self {
            chart_id,
            options,
            prepared_chart_applied: false,
            play_config_key_mode,
            lane: PendingPlayLaneState::from_snapshot(
                snapshot,
                &mode_config,
                hs_fix,
                playback_rate_percent,
            ),
            lane_actions: Vec::new(),
            visual_input: PendingPlayVisualInput::new(key_mode, binding, snapshot.autoplay),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingPlayLaneState {
    pub(super) hispeed: f32,
    pub(super) hispeed_mode: HispeedMode,
    pub(super) base_hispeed_mode: HispeedMode,
    pub(super) floating_policy: FloatingPolicy,
    pub(super) normal_hispeed_level: u8,
    pub(super) target_green_number: u32,
    pub(super) lane_cover: f32,
    pub(super) lift: f32,
    pub(super) hidden_cover: f32,
    pub(super) sudden_enabled: bool,
    pub(super) lift_enabled: bool,
    pub(super) hidden_enabled: bool,
    pub(super) lane_cover_visible: bool,
    pub(super) lane_target: PlayLaneTarget,
    pub(super) lane_cover_changing: bool,
    pub(super) hsfix_base_bpm: f32,
    /// Classic + HS-FIXでBPM150基準のHS値を実効倍率へ変換する係数。
    /// HS CONFIG = LR2。
    pub(super) lr2_hispeed: bool,
    pub(super) classic_hsfix_scale: f32,
    pub(super) hispeed_auto_adjust: bool,
    pub(super) playback_rate_percent: u16,
}

impl PendingPlayLaneState {
    pub(super) fn from_snapshot(
        snapshot: &RenderSnapshot,
        mode_config: &PlayModeConfig,
        hs_fix: HsFixOption,
        playback_rate_percent: u16,
    ) -> Self {
        let sudden_enabled = mode_config.lane_effect.sudden_enabled();
        let hidden_enabled = mode_config.lane_effect.hidden_enabled();
        let base_hispeed_mode = match mode_config.base_hispeed {
            BaseHispeedConfig::Normal => HispeedMode::Normal,
            BaseHispeedConfig::Classic | BaseHispeedConfig::Lr2 => HispeedMode::Classic,
        };
        let hsfix_base_bpm = hsfix_base_bpm_from_snapshot(snapshot, hs_fix);
        let lr2_hispeed = mode_config.base_hispeed == BaseHispeedConfig::Lr2;
        Self {
            hispeed: snapshot.hispeed,
            hispeed_mode: if snapshot.hispeed_mode_index == 0 {
                base_hispeed_mode
            } else {
                HispeedMode::Floating
            },
            base_hispeed_mode,
            floating_policy: match mode_config.floating_policy {
                FloatingPolicyConfig::Disabled => FloatingPolicy::Disabled,
                FloatingPolicyConfig::Toggle => FloatingPolicy::Toggle,
                FloatingPolicyConfig::Locked => FloatingPolicy::Locked,
            },
            normal_hispeed_level: crate::config::play::normalize_normal_hispeed_level(
                mode_config.normal_hispeed_level,
            ),
            target_green_number: mode_config.target_green_number.max(1),
            lane_cover: crate::config::play::clamp_lane_cover_for_lift(
                crate::config::play::lane_unit_to_f32(mode_config.sudden),
                snapshot.lift,
            ),
            lift: snapshot.lift,
            hidden_cover: if hidden_enabled {
                crate::config::play::lane_unit_to_f32(mode_config.hidden)
            } else {
                0.0
            },
            sudden_enabled,
            lift_enabled: mode_config.lift_enabled,
            hidden_enabled,
            lane_cover_visible: sudden_enabled,
            lane_target: PlayLaneTarget::Lift,
            lane_cover_changing: snapshot.lane_cover_changing,
            hsfix_base_bpm,
            lr2_hispeed,
            classic_hsfix_scale: crate::screens::play_snapshot::classic_hsfix_scale_for_option(
                lr2_hispeed,
                hs_fix,
                f64::from(hsfix_base_bpm),
                playback_rate_percent,
            ),
            hispeed_auto_adjust: mode_config.hispeed_auto_adjust,
            playback_rate_percent,
        }
    }

    /// skin表示とprofile保存に使うHS値。LR2 + HS-FIXではBPM150基準の値を返す。
    pub(super) fn display_hispeed(self) -> f32 {
        if self.hispeed_mode == HispeedMode::Classic {
            crate::config::play::classic_display_hispeed(self.hispeed, self.classic_hsfix_scale)
        } else {
            self.hispeed
        }
    }

    pub(super) fn active_lane_cover(self) -> f32 {
        if self.sudden_enabled && self.lane_cover_visible {
            crate::config::play::clamp_lane_cover_for_lift(self.lane_cover, self.lift)
        } else {
            0.0
        }
    }

    pub(super) fn refresh_floating_hispeed(&mut self, now_bpm: f32, speed_locked: bool) {
        if self.hispeed_mode != HispeedMode::Floating || speed_locked {
            return;
        }
        let visible =
            crate::config::play::visible_lane_fraction(self.active_lane_cover(), self.lift);
        let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
            f64::from(now_bpm),
            self.playback_rate_percent,
        );
        self.hispeed =
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                self.target_green_number.max(1) as f32,
                visible,
                now_bpm,
                1.0,
            ));
    }

    pub(super) fn refresh_floating_hispeed_without_lane_effects(
        &mut self,
        now_bpm: f32,
        speed_locked: bool,
    ) {
        if self.hispeed_mode != HispeedMode::Floating || speed_locked {
            return;
        }
        let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
            f64::from(now_bpm),
            self.playback_rate_percent,
        );
        self.hispeed =
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                self.target_green_number.max(1) as f32,
                1.0,
                now_bpm,
                1.0,
            ));
    }

    pub(super) fn refresh_cover_hispeed(&mut self, now_bpm: f32, speed_locked: bool) {
        let target_bpm = if self.hispeed_auto_adjust { now_bpm } else { self.hsfix_base_bpm };
        self.refresh_floating_hispeed(target_bpm, speed_locked);
    }

    pub(super) fn sync_chart_bpm(&mut self, snapshot: &RenderSnapshot, hs_fix: HsFixOption) {
        // Classicの表示HSを保ったまま、譜面の基準BPMで実効倍率を求め直す。
        let display_hispeed = self.display_hispeed();
        self.hsfix_base_bpm = hsfix_base_bpm_from_snapshot(snapshot, hs_fix);
        self.classic_hsfix_scale = crate::screens::play_snapshot::classic_hsfix_scale_for_option(
            self.lr2_hispeed,
            hs_fix,
            f64::from(self.hsfix_base_bpm),
            self.playback_rate_percent,
        );
        if self.hispeed_mode == HispeedMode::Classic {
            self.hispeed = crate::config::play::classic_effective_hispeed(
                display_hispeed,
                self.classic_hsfix_scale,
            );
        }
    }

    pub(super) fn current_green_number(self, now_bpm: f32) -> u32 {
        let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
            f64::from(now_bpm),
            self.playback_rate_percent,
        ) as f32;
        let duration = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
            now_bpm,
            self.hispeed,
            self.active_lane_cover(),
            self.lift,
            1.0,
        );
        green_number_from_display_duration(duration)
    }

    pub(super) fn current_full_lane_green_number(self, now_bpm: f32) -> u32 {
        let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
            f64::from(now_bpm),
            self.playback_rate_percent,
        ) as f32;
        let duration = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
            now_bpm,
            self.hispeed,
            0.0,
            0.0,
            1.0,
        );
        green_number_from_display_duration(duration)
    }

    pub(super) fn refresh_normal_hispeed(&mut self, now_bpm: f32, speed_locked: bool) {
        if self.hispeed_mode != HispeedMode::Normal || speed_locked {
            return;
        }
        let now_bpm = crate::screens::play_snapshot::effective_bpm_for_playback_rate(
            f64::from(now_bpm),
            self.playback_rate_percent,
        );
        self.hispeed =
            clamp_hispeed(crate::screens::play_snapshot::hispeed_for_green_number_values(
                crate::config::play::normal_hispeed_green_number(self.normal_hispeed_level) as f32,
                1.0,
                now_bpm,
                1.0,
            ));
    }

    pub(super) fn apply_to_snapshot(self, snapshot: &mut RenderSnapshot) {
        snapshot.hispeed = self.hispeed;
        snapshot.hispeed_display = Some(self.display_hispeed());
        snapshot.hispeed_mode_index = match self.hispeed_mode {
            HispeedMode::Normal | HispeedMode::Classic => 0,
            HispeedMode::Floating => 1,
        };
        snapshot.base_hispeed_index = match self.base_hispeed_mode {
            HispeedMode::Normal => 1,
            HispeedMode::Classic | HispeedMode::Floating => 0,
        };
        snapshot.normal_hispeed_level = self.normal_hispeed_level;
        snapshot.hispeed_config_index = match (self.base_hispeed_mode, self.floating_policy) {
            (_, FloatingPolicy::Locked) => 2,
            _ if self.lr2_hispeed => 5,
            (HispeedMode::Normal, FloatingPolicy::Disabled) => 0,
            (_, FloatingPolicy::Disabled) => 1,
            (HispeedMode::Normal, FloatingPolicy::Toggle) => 3,
            (_, FloatingPolicy::Toggle) => 4,
        };
        snapshot.target_green_number = self.target_green_number;
        snapshot.lift = self.lift;
        snapshot.lane_cover = self.active_lane_cover();
        snapshot.hidden_cover = self.hidden_cover;
        snapshot.lanecover_enabled = self.sudden_enabled;
        snapshot.lift_enabled = self.lift_enabled;
        snapshot.hidden_enabled = self.hidden_enabled;
        snapshot.lane_cover_changing = self.lane_cover_changing;
        snapshot.note_display_duration_ms =
            crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
                crate::screens::play_snapshot::effective_bpm_for_playback_rate(
                    f64::from(snapshot.now_bpm),
                    self.playback_rate_percent,
                ) as f32,
                self.hispeed,
                snapshot.lane_cover,
                self.lift,
                1.0,
            )
            .round()
            .clamp(0.0, i32::MAX as f32) as i32;
    }
}

fn hsfix_base_bpm_from_snapshot(snapshot: &RenderSnapshot, hs_fix: HsFixOption) -> f32 {
    match hs_fix {
        HsFixOption::Off | HsFixOption::StartBpm => snapshot.now_bpm,
        HsFixOption::MaxBpm => snapshot.max_bpm,
        HsFixOption::MainBpm => snapshot.main_bpm,
        HsFixOption::MinBpm => snapshot.min_bpm,
    }
}

/// プレイ中の補助操作 (E1/E2 + 鍵盤・SCR、Start2回押し等) の入力 resolver。
///
/// 現在のプレイセッションの設定対象モード (`play_config_key_mode`) と、開始時に解決した
/// コントローラー割り当てで構築する。選曲用の設定は参照しない。
#[derive(Debug, Clone)]
pub(super) struct PlayOptionInput {
    pub(super) key_mode: KeyMode,
    pub(super) binding: LaneBinding,
    pub(super) scratch_binding: LaneBinding,
    actions: super::input_resolve::ActionTable,
    gamepad_slots: crate::input::gamepad::GamepadSlotMap,
}

impl PlayOptionInput {
    pub(super) fn new(
        key_mode: KeyMode,
        binding: LaneBinding,
        profile_input: &ProfileInputConfig,
        gamepad_slots: crate::input::gamepad::GamepadSlotMap,
    ) -> Self {
        let scratch_binding =
            crate::config::play_input::lane_binding_for_play_option_scratch_with_slots(
                profile_input,
                key_mode,
                gamepad_slots,
            )
            .unwrap_or_else(|_| LaneBinding { entries: Vec::new() });
        let actions = super::input_resolve::ActionTable::from_config(profile_input);
        Self { key_mode, binding, scratch_binding, actions, gamepad_slots }
    }

    /// テスト用: binding を直接与える。E1〜E4 は既定 profile の設定を使う。
    #[cfg(test)]
    pub(super) fn from_bindings_for_test(
        key_mode: KeyMode,
        binding: LaneBinding,
        scratch_binding: LaneBinding,
    ) -> Self {
        let profile_input = crate::config::play_input::default_profile_input();
        Self {
            key_mode,
            binding,
            scratch_binding,
            actions: super::input_resolve::ActionTable::from_config(&profile_input),
            gamepad_slots: Default::default(),
        }
    }

    pub(super) fn resolve_entry(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
    ) -> Option<bmz_gameplay::input::binding::BindingResolution> {
        self.binding
            .resolve_entry(device, control)
            .or_else(|| self.scratch_binding.resolve_entry(device, control))
    }

    pub(super) fn resolves_lane(&self, device: DeviceId, control: &PhysicalControl) -> bool {
        self.resolve_entry(device, control).is_some()
    }

    fn slot(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
    ) -> Option<super::input_resolve::PlayerSide> {
        super::input_resolve::controller_slot(device, control, self.gamepad_slots)
    }

    /// E1〜E4 等の action か。コントローラーは入力元のスロットで個別設定・共通設定を選ぶ。
    pub(super) fn is_action(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
        action: InputActionConfig,
    ) -> bool {
        let slot = self.slot(device, control);
        self.actions.has_action(control, slot, action)
            || (action == InputActionConfig::E1
                && self.actions.has_action(control, slot, InputActionConfig::E4))
    }

    /// 共通 resolver 用の解決結果。レーンは現在のプレイモードのものを返す。
    pub(super) fn resolve_control(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
    ) -> super::input_resolve::ResolvedControl {
        super::input_resolve::ResolvedControl::for_play(
            self.slot(device, control),
            self.key_mode,
            self.resolve_entry(device, control),
            &self.actions,
            control,
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingPlayVisualInput {
    pub(super) key_mode: KeyMode,
    pub(super) binding: LaneBinding,
    pub(super) suppress_human_input: bool,
    pub(super) lane_keyon_started_at: [Option<TimeUs>; LANE_COUNT],
    pub(super) lane_keyoff_started_at: [Option<TimeUs>; LANE_COUNT],
    pub(super) lane_scratch_direction: [Option<ScratchDirection>; LANE_COUNT],
    pub(super) lane_scratch_angle_delta_ms: [i64; LANE_COUNT],
    pub(super) scratch_angle_last_render_at: Option<TimeUs>,
}

impl PendingPlayVisualInput {
    pub(super) fn new(key_mode: KeyMode, binding: LaneBinding, suppress_human_input: bool) -> Self {
        Self {
            key_mode,
            binding,
            suppress_human_input,
            lane_keyon_started_at: [None; LANE_COUNT],
            lane_keyoff_started_at: [None; LANE_COUNT],
            lane_scratch_direction: [None; LANE_COUNT],
            lane_scratch_angle_delta_ms: [0; LANE_COUNT],
            scratch_angle_last_render_at: None,
        }
    }

    pub(super) fn apply_event(&mut self, event: &DeviceInputEvent, visual_now: TimeUs) {
        if self.suppress_human_input {
            return;
        }
        let Some(binding) = self.binding.resolve_entry(event.device, &event.control) else {
            return;
        };
        let lane = binding.lane.index();
        match event.kind {
            InputKind::Press => {
                self.lane_keyon_started_at[lane] = Some(visual_now);
                self.lane_keyoff_started_at[lane] = None;
                self.lane_scratch_direction[lane] = binding.scratch_direction;
            }
            InputKind::Release => {
                if self.lane_keyon_started_at[lane].is_some() {
                    self.lane_keyon_started_at[lane] = None;
                    self.lane_keyoff_started_at[lane] = Some(visual_now);
                }
                self.lane_scratch_direction[lane] = None;
            }
        }
    }

    pub(super) fn advance(&mut self, visual_now: TimeUs) {
        let Some(last_render_at) = self.scratch_angle_last_render_at.replace(visual_now) else {
            return;
        };
        let delta_ms = ((visual_now.0 - last_render_at.0) / 1_000).max(0);
        if delta_ms == 0 {
            return;
        }
        for lane in [Lane::Scratch, Lane::Scratch2] {
            let lane_index = lane.index();
            if self.lane_keyon_started_at[lane_index].is_none() {
                continue;
            }
            let sign =
                match self.lane_scratch_direction[lane_index].unwrap_or(ScratchDirection::Down) {
                    ScratchDirection::Up => 1,
                    ScratchDirection::Down => -1,
                };
            self.lane_scratch_angle_delta_ms[lane_index] =
                (self.lane_scratch_angle_delta_ms[lane_index] + sign * delta_ms.saturating_mul(2))
                    .rem_euclid(2_160);
        }
    }

    pub(super) fn apply_to_session(self, session: &mut bmz_gameplay::session::GameSession) {
        session.lane_keyon_started_at = self.lane_keyon_started_at;
        session.lane_keyoff_started_at = self.lane_keyoff_started_at;
        session.lane_scratch_direction = self.lane_scratch_direction;
        session.lane_scratch_angle_delta_ms = self.lane_scratch_angle_delta_ms;
        session.scratch_angle_last_render_at = self.scratch_angle_last_render_at;
    }
}
