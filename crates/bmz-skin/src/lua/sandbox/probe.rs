use super::*;

#[derive(Debug, Clone)]
pub(super) struct MainStateProbe {
    pub(super) mode: MainStateProbeMode,
    pub(super) inferring: bool,
    pub(super) number_calls: Vec<i32>,
    pub(super) number_values: BTreeMap<i32, i32>,
    pub(super) option_calls: Vec<i32>,
    pub(super) option_values: BTreeMap<i32, bool>,
    pub(super) timer_calls: Vec<i32>,
    pub(super) timer_values: BTreeMap<i32, i32>,
    pub(super) event_index_calls: Vec<i32>,
    pub(super) event_index_values: BTreeMap<i32, i32>,
    pub(super) offset_values: BTreeMap<i32, LuaSkinOffsetValue>,
    pub(super) gauge_type_calls: usize,
    pub(super) gauge_type_value: i32,
    pub(super) float_number_calls: Vec<i32>,
    pub(super) float_number_values: BTreeMap<i32, f64>,
    pub(super) text_calls: Vec<i32>,
    pub(super) text_values: BTreeMap<i32, String>,
    pub(super) os_clock_calls: usize,
    pub(super) os_clock_value: Option<f64>,
    pub(super) time_value_us: i32,
    pub(super) next_dynamic_timer_id: i32,
    pub(super) dynamic_timers: Vec<(i32, String)>,
    pub(super) dynamic_timer_ids_by_observe: BTreeMap<String, i32>,
    pub(super) fixed_delay_timers: Vec<(i32, i32, i32)>,
    pub(super) unsupported_dynamic_timers: Vec<i32>,
    pub(super) load_time_constant_dynamic_timers: Vec<i32>,
    pub(super) next_runtime_flag_id: i32,
    pub(super) runtime_flags: Vec<LuaRuntimeFlagProbe>,
    pub(super) next_runtime_event_id: i32,
    pub(super) runtime_events: Vec<(i32, Vec<i32>)>,
    pub(super) runtime_event_ids_by_flags: BTreeMap<Vec<i32>, i32>,
    pub(super) capture_audio_actions: bool,
    pub(super) audio_actions: Vec<LuaAudioActionProbe>,
    pub(super) keylogger_destination_occurrences: BTreeMap<String, usize>,
    pub(super) gauge_lead_glow_occurrences: BTreeMap<String, usize>,
    pub(super) gauge_value_destination_occurrences: BTreeMap<String, usize>,
    pub(super) gauge_value_overlay_mode: Option<&'static str>,
    pub(super) result_panel_default: Option<i32>,
    pub(super) runtime_mode: LuaSkinRuntimeMode,
    pub(super) runtime_callbacks: Vec<LuaRuntimeCallbackSpec>,
    pub(super) load_dependencies: Option<Arc<Mutex<SkinLoadDependencies>>>,
}

impl Default for MainStateProbe {
    fn default() -> Self {
        Self {
            mode: MainStateProbeMode::default(),
            inferring: false,
            number_calls: Vec::new(),
            number_values: BTreeMap::new(),
            option_calls: Vec::new(),
            option_values: BTreeMap::new(),
            timer_calls: Vec::new(),
            timer_values: BTreeMap::new(),
            event_index_calls: Vec::new(),
            event_index_values: BTreeMap::new(),
            offset_values: BTreeMap::new(),
            gauge_type_calls: 0,
            gauge_type_value: 0,
            float_number_calls: Vec::new(),
            float_number_values: BTreeMap::new(),
            text_calls: Vec::new(),
            text_values: BTreeMap::new(),
            os_clock_calls: 0,
            os_clock_value: None,
            time_value_us: 1_000_000,
            next_dynamic_timer_id: SKIN_DYNAMIC_TIMER_BASE,
            dynamic_timers: Vec::new(),
            dynamic_timer_ids_by_observe: BTreeMap::new(),
            fixed_delay_timers: Vec::new(),
            unsupported_dynamic_timers: Vec::new(),
            load_time_constant_dynamic_timers: Vec::new(),
            next_runtime_flag_id: 0,
            runtime_flags: Vec::new(),
            next_runtime_event_id: SKIN_EVENT_RUNTIME_BASE,
            runtime_events: Vec::new(),
            runtime_event_ids_by_flags: BTreeMap::new(),
            capture_audio_actions: true,
            audio_actions: Vec::new(),
            keylogger_destination_occurrences: BTreeMap::new(),
            gauge_lead_glow_occurrences: BTreeMap::new(),
            gauge_value_destination_occurrences: BTreeMap::new(),
            gauge_value_overlay_mode: None,
            result_panel_default: None,
            runtime_mode: LuaSkinRuntimeMode::Auto,
            runtime_callbacks: Vec::new(),
            load_dependencies: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) enum MainStateProbeMode {
    #[default]
    RuntimeStub,
    SymbolicNumbers {
        base_value: i32,
    },
    RecordNumbers {
        default_value: i32,
    },
}

impl MainStateProbe {
    pub(super) fn record_audio_action(
        &mut self,
        action: LuaAudioActionKindProbe,
        path: String,
        volume: f64,
    ) {
        if self.capture_audio_actions && !path.is_empty() && volume.is_finite() {
            self.audio_actions.push(LuaAudioActionProbe { action, path, volume });
        }
    }

    pub(super) fn take_audio_actions(&mut self) -> Vec<LuaAudioActionProbe> {
        std::mem::take(&mut self.audio_actions)
    }

    pub(super) fn clear_aux_calls(&mut self) {
        self.float_number_calls.clear();
        self.float_number_values.clear();
        self.text_calls.clear();
        self.text_values.clear();
        self.os_clock_calls = 0;
        self.os_clock_value = None;
        self.event_index_calls.clear();
        self.event_index_values.clear();
    }

    pub(super) fn begin_number_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::SymbolicNumbers { base_value: default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    pub(super) fn begin_number_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    pub(super) fn begin_number_call_recording_with_option_value(
        &mut self,
        default_value: i32,
        option_id: i32,
        option_value: bool,
    ) {
        self.begin_number_call_recording(default_value);
        self.option_values.insert(option_id, option_value);
    }

    pub(super) fn begin_number_recording_with_value(&mut self, ref_id: i32, value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.number_values.insert(ref_id, value);
    }

    pub(super) fn begin_number_recording_with_values(&mut self, values: BTreeMap<i32, i32>) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values = values;
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    pub(super) fn begin_number_recording_with_values_and_options(
        &mut self,
        values: BTreeMap<i32, i32>,
        options: BTreeMap<i32, bool>,
    ) {
        self.begin_number_recording_with_values(values);
        self.option_values = options;
    }

    pub(super) fn begin_text_recording_with_values(&mut self, values: BTreeMap<i32, String>) {
        self.begin_number_recording_with_values(BTreeMap::new());
        self.text_calls.clear();
        self.text_values = values;
    }

    pub(super) fn begin_number_timer_recording_with_values(
        &mut self,
        values: BTreeMap<i32, i32>,
        mut timer_values: BTreeMap<i32, i32>,
    ) {
        self.begin_number_recording_with_values(values);
        timer_values.entry(i32::MIN).or_insert(i32::MIN);
        self.timer_values = timer_values;
    }

    pub(super) fn begin_option_call_recording(&mut self, default_value: bool) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.option_values.insert(i32::MIN, default_value);
    }

    pub(super) fn begin_option_recording_with_value(&mut self, option_id: i32, value: bool) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.option_values.insert(option_id, value);
    }

    pub(super) fn begin_timer_option_call_recording(&mut self) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.option_values.insert(i32::MIN, true);
        self.timer_values.insert(i32::MIN, i32::MIN);
    }

    pub(super) fn begin_timer_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.timer_values.insert(i32::MIN, default_value);
    }

    pub(super) fn begin_timer_recording_with_values(
        &mut self,
        mut timer_values: BTreeMap<i32, i32>,
    ) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        timer_values.entry(i32::MIN).or_insert(i32::MIN);
        self.timer_values = timer_values;
    }

    pub(super) fn begin_timer_event_recording_with_values(
        &mut self,
        timer_values: BTreeMap<i32, i32>,
        event_id: i32,
        event_value: i32,
    ) {
        self.begin_timer_recording_with_values(timer_values);
        self.event_index_values.insert(event_id, event_value);
    }

    pub(super) fn begin_timer_option_recording_with_values(
        &mut self,
        timer_id: i32,
        timer_value: i32,
        option_id: i32,
        option_value: bool,
    ) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.timer_values.insert(timer_id, timer_value);
        self.option_values.insert(option_id, option_value);
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
    }

    pub(super) fn begin_timer_options_recording_with_values(
        &mut self,
        timer_values: BTreeMap<i32, i32>,
        option_values: BTreeMap<i32, bool>,
    ) {
        self.begin_timer_recording_with_values(timer_values);
        self.option_values = option_values;
    }

    pub(super) fn begin_gauge_type_call_recording(&mut self, value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = value;
    }

    pub(super) fn begin_gauge_type_recording_with_value(&mut self, value: i32) {
        self.begin_gauge_type_call_recording(value);
    }

    pub(super) fn begin_event_index_call_recording(&mut self, default_value: i32) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.clear_aux_calls();
    }

    pub(super) fn begin_event_index_recording_with_value(&mut self, event_id: i32, value: i32) {
        self.begin_event_index_call_recording(0);
        self.event_index_values.insert(event_id, value);
    }

    pub(super) fn begin_event_index_options_recording_with_values(
        &mut self,
        event_id: i32,
        event_value: i32,
        option_values: BTreeMap<i32, bool>,
        default_option_value: bool,
    ) {
        self.begin_event_index_recording_with_value(event_id, event_value);
        self.option_values.insert(i32::MIN, default_option_value);
        self.option_values.extend(option_values);
    }

    pub(super) fn begin_os_clock_recording(&mut self, value: f64) {
        self.mode = MainStateProbeMode::RecordNumbers { default_value: 0 };
        self.number_calls.clear();
        self.number_values.clear();
        self.option_calls.clear();
        self.option_values.clear();
        self.timer_calls.clear();
        self.timer_values.clear();
        self.event_index_calls.clear();
        self.event_index_values.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.float_number_calls.clear();
        self.float_number_values.clear();
        self.text_calls.clear();
        self.os_clock_calls = 0;
        self.os_clock_value = Some(value);
    }

    pub(super) fn begin_os_clock_options_recording(
        &mut self,
        value: f64,
        option_values: &[(i32, bool)],
        default_option_value: bool,
    ) {
        self.begin_os_clock_recording(value);
        self.option_values.insert(i32::MIN, default_option_value);
        for &(option_id, option_value) in option_values {
            self.option_values.insert(option_id, option_value);
        }
    }

    pub(super) fn end_recording(&mut self) {
        self.mode = MainStateProbeMode::RuntimeStub;
        self.number_values.clear();
        self.option_values.clear();
        self.timer_values.clear();
        self.event_index_values.clear();
        self.event_index_calls.clear();
        self.gauge_type_calls = 0;
        self.gauge_type_value = 0;
        self.os_clock_value = None;
        self.text_values.clear();
    }

    pub(super) fn number(&mut self, ref_id: i32) -> i32 {
        match self.mode {
            MainStateProbeMode::RuntimeStub => {
                let value = self
                    .number_values
                    .get(&ref_id)
                    .copied()
                    .unwrap_or_else(|| lua_runtime_stub_number(ref_id));
                self.record_load_time_number_dependency(ref_id, value);
                value
            }
            MainStateProbeMode::SymbolicNumbers { base_value } => {
                self.number_calls.push(ref_id);
                self.number_values.get(&ref_id).copied().unwrap_or(base_value + ref_id)
            }
            MainStateProbeMode::RecordNumbers { default_value } => {
                self.number_calls.push(ref_id);
                self.number_values.get(&ref_id).copied().unwrap_or(default_value)
            }
        }
    }

    pub(super) fn judge(&mut self, index: i32) -> i32 {
        main_state_judge_ref(index).map(|ref_id| self.number(ref_id)).unwrap_or(0)
    }

    pub(super) fn option(&mut self, option_id: i32) -> bool {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            let value = self.option_values.get(&option_id).copied().unwrap_or(false);
            self.record_load_time_option_dependency(option_id, value);
            return value;
        }
        self.option_calls.push(option_id);
        self.option_values
            .get(&option_id)
            .copied()
            .or_else(|| self.option_values.get(&i32::MIN).copied())
            .unwrap_or(false)
    }

    pub(super) fn record_load_time_number_dependency(&self, ref_id: i32, value: i32) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.number_values.insert(ref_id, value);
        }
    }

    pub(super) fn record_load_time_option_dependency(&self, option_id: i32, value: bool) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.option_values.insert(option_id, value);
        }
    }

    pub(super) fn record_load_time_event_index_dependency(&self, event_id: i32, value: i32) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.event_index_values.insert(event_id, value);
        }
    }

    pub(super) fn record_load_time_text_dependency(&self, ref_id: i32, value: &str) {
        if let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.text_values.insert(ref_id, value.to_string());
        }
    }

    pub(super) fn offset(&self, offset_id: i32) -> LuaSkinOffsetValue {
        let value = self.offset_values.get(&offset_id).copied().unwrap_or_default();
        if matches!(self.mode, MainStateProbeMode::RuntimeStub)
            && let Some(dependencies) = &self.load_dependencies
            && let Ok(mut dependencies) = dependencies.lock()
        {
            dependencies.offset_id_values.insert(offset_id, value);
        }
        value
    }

    pub(super) fn timer(&mut self, timer_id: i32) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return i32::MIN;
        }
        self.timer_calls.push(timer_id);
        self.timer_values
            .get(&timer_id)
            .copied()
            .or_else(|| self.timer_values.get(&i32::MIN).copied())
            .unwrap_or(i32::MIN)
    }

    pub(super) fn gauge_type(&mut self) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 0;
        }
        self.gauge_type_calls += 1;
        self.gauge_type_value
    }

    pub(super) fn float_number(&mut self, ref_id: i32) -> f64 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 0.0;
        }
        self.float_number_calls.push(ref_id);
        self.float_number_values.get(&ref_id).copied().unwrap_or(0.0)
    }

    pub(super) fn volume_number(&mut self, ref_id: i32) -> f64 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return 1.0;
        }
        f64::from(self.number(ref_id)) / 100.0
    }

    pub(super) fn text(&mut self, ref_id: i32) -> String {
        if ref_id == 1010 {
            return format!("bmz-player {}", env!("CARGO_PKG_VERSION"));
        }
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            if (1001..=1003).contains(&ref_id) {
                if let Some(value) = self.text_values.get(&ref_id).cloned() {
                    self.record_load_time_text_dependency(ref_id, &value);
                    return value;
                }
                return format!(
                    "{LUA_TEXT_REF_SENTINEL_PREFIX}{ref_id}{LUA_TEXT_REF_SENTINEL_SUFFIX}"
                );
            }
            let value = self.text_values.get(&ref_id).cloned().unwrap_or_default();
            self.record_load_time_text_dependency(ref_id, &value);
            return value;
        }
        self.text_calls.push(ref_id);
        self.text_values.get(&ref_id).cloned().unwrap_or_else(|| format!("Text{ref_id}"))
    }

    pub(super) fn event_index(&mut self, event_id: i32) -> i32 {
        match self.mode {
            MainStateProbeMode::RuntimeStub => {
                let value = self.event_index_values.get(&event_id).copied().unwrap_or_default();
                self.record_load_time_event_index_dependency(event_id, value);
                value
            }
            MainStateProbeMode::SymbolicNumbers { base_value } => {
                self.event_index_calls.push(event_id);
                self.event_index_values.get(&event_id).copied().unwrap_or(base_value + event_id)
            }
            MainStateProbeMode::RecordNumbers { default_value } => {
                self.event_index_calls.push(event_id);
                self.event_index_values.get(&event_id).copied().unwrap_or(default_value)
            }
        }
    }

    pub(super) fn time(&mut self) -> i32 {
        if matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            return lua_load_now_micros();
        }
        let value = self.time_value_us;
        self.time_value_us = self.time_value_us.saturating_add(1_000);
        value
    }

    pub(super) fn begin_draw_probe(
        &mut self,
        numbers: BTreeMap<i32, i32>,
        floats: BTreeMap<i32, f64>,
    ) {
        self.begin_number_recording_with_values(numbers);
        self.float_number_values = floats;
    }

    pub(super) fn os_clock(&mut self) -> f64 {
        if let Some(value) = self.os_clock_value {
            self.os_clock_calls += 1;
            return value;
        }
        if !matches!(self.mode, MainStateProbeMode::RuntimeStub) {
            self.os_clock_calls += 1;
            return 0.0;
        }
        lua_os_clock_seconds()
    }
}
