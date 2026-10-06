#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LuaRuntimeFlagProbe {
    pub(super) id: i32,
    pub(super) table: String,
    pub(super) field: String,
    pub(super) initial: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum LuaRuntimeScalar {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LuaRuntimeCallbackKind {
    Draw,
    Value,
    Timer(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LuaRuntimeCallbackSpec {
    pub(super) path: String,
    pub(super) kind: LuaRuntimeCallbackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LuaAudioActionKindProbe {
    Play,
    Loop,
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LuaAudioActionProbe {
    pub(super) action: LuaAudioActionKindProbe,
    pub(super) path: String,
    pub(super) volume: f64,
}

/// beatoraja fast/slow 判定カウント ref (graph 比率推論用)
pub(super) const FAST_SLOW_FAST_REFS: [i32; 6] = [410, 412, 414, 416, 418, 421];
pub(super) const FAST_SLOW_SLOW_REFS: [i32; 6] = [411, 413, 415, 417, 419, 422];

pub(super) fn main_state_judge_ref(index: i32) -> Option<i32> {
    match index {
        0 => Some(110),
        1 => Some(111),
        2 => Some(112),
        3 => Some(113),
        4 => Some(114),
        5 => Some(420),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertReport {
    pub warnings: Vec<String>,
}

pub(super) struct LuaRuntimeCallback {
    pub(super) path: String,
    pub(super) kind: LuaRuntimeCallbackKind,
    pub(super) function: Option<Function>,
}

/// A Lua-only sidecar that owns the runtime VM and every callback function.
///
/// The VM is intentionally not cloneable. Its callbacks are obtained by a second
/// load after inference has completed, so inference can never mutate runtime
/// closure state, module state, or the Lua random-number generator.
pub struct LuaSkinRuntime {
    pub(super) lua: Lua,
    pub(super) main_state_dispatch: Table,
    pub(super) callbacks: Vec<LuaRuntimeCallback>,
    pub(super) instruction_budget: LuaInstructionBudget,
    pub(super) skin_path: PathBuf,
    pub(super) failed_callbacks: BTreeSet<usize>,
    pub(super) failure_log_count: usize,
    pub(super) pending_frame_start: bool,
    pub(super) pending_timer_frame_start: bool,
    pub(super) custom_timers: Arc<Mutex<LuaCustomTimers>>,
    pub(super) custom_timer_callbacks: Vec<(i32, usize)>,
    pub(super) result_panel_bindings: Arc<LuaResultPanelBindings>,
}

/// The same named panel state recognized by the declarative draw/action bridge.
/// Retain one rooted closure per shared upvalue; never clone or fold its state.
#[derive(Default)]
pub(super) struct LuaResultPanelBindings {
    global: bool,
    locals: Vec<(Function, i32)>,
}

impl LuaResultPanelBindings {
    pub(super) fn new(lua: &Lua, callbacks: &[LuaRuntimeCallback]) -> Self {
        let global = lua
            .globals()
            .raw_get::<Value>("Expand_op")
            .ok()
            .and_then(lua_result_panel_value)
            .is_some();
        let mut seen = BTreeSet::new();
        let locals = callbacks
            .iter()
            .filter_map(|callback| {
                let function = callback.function.as_ref()?;
                let (index, _, identity) = lua_result_mode_binding(lua, function)?;
                seen.insert(identity).then(|| (function.clone(), index))
            })
            .collect();
        Self { global, locals }
    }

    fn bind<'a>(&'a self, lua: &'a Lua, panel: Option<i32>) -> ResultPanelGuard<'a> {
        let mut guard = ResultPanelGuard { lua, global: None, locals: Vec::new() };
        let Some(panel) = panel.filter(|panel| (0..=2).contains(panel)) else {
            return guard;
        };
        if self.global
            && let Ok(previous) = lua.globals().raw_get::<Value>("Expand_op")
        {
            guard.global = Some(previous);
            let _ = lua.globals().raw_set("Expand_op", panel);
        }
        let mode = match panel {
            1 => 1,
            2 => 0,
            _ => 2,
        };
        for (function, index) in &self.locals {
            if let Some((_, previous)) = lua_result_mode_upvalue(lua, function) {
                guard.locals.push((function, *index, previous));
                set_lua_integer_upvalue(lua, function, *index, mode);
            }
        }
        guard
    }
}

struct ResultPanelGuard<'a> {
    lua: &'a Lua,
    global: Option<Value>,
    locals: Vec<(&'a Function, i32, i32)>,
}

impl Drop for ResultPanelGuard<'_> {
    fn drop(&mut self) {
        if let Some(previous) = &self.global {
            let _ = self.lua.globals().raw_set("Expand_op", previous.clone());
        }
        for (function, index, previous) in &self.locals {
            set_lua_integer_upvalue(self.lua, function, *index, *previous);
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct LuaCustomTimers {
    pub(super) values: BTreeMap<i32, Option<i64>>,
    pub(super) active: BTreeSet<i32>,
}

impl fmt::Debug for LuaSkinRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LuaSkinRuntime")
            .field("skin_path", &self.skin_path)
            .field("callback_count", &self.callbacks.len())
            .field("failed_callbacks", &self.failed_callbacks)
            .finish_non_exhaustive()
    }
}

impl LuaSkinRuntime {
    /// Begin one render frame, independently of the skin's playback clock.
    /// Every callback until the next call shares the same aggregate budget.
    pub fn begin_frame(&mut self) {
        self.pending_frame_start = true;
        self.pending_timer_frame_start = true;
    }

    /// Update active timers once per frame, including timers with no visible destination.
    /// Values are scene-relative start times in microseconds; None means OFF.
    pub fn advance_custom_timers(
        &mut self,
        state: &dyn LuaMainState,
    ) -> BTreeMap<i32, Option<i64>> {
        if std::mem::take(&mut self.pending_timer_frame_start) {
            for index in 0..self.custom_timer_callbacks.len() {
                let (id, callback_id) = self.custom_timer_callbacks[index];
                self.begin_runtime_callback();
                let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                    self.evaluate_callback_inner(
                        callback_id,
                        LuaRuntimeCallbackKind::Timer(id),
                        Some(state),
                    )
                }));
                let value = match result {
                    Ok(Ok(LuaRuntimeEvaluatedValue::Nil)) => Some(0),
                    Ok(Ok(LuaRuntimeEvaluatedValue::Integer(value))) => Some(value),
                    Ok(Ok(LuaRuntimeEvaluatedValue::Number(value)))
                        if value.is_finite()
                            && value >= i64::MIN as f64
                            && value < -(i64::MIN as f64) =>
                    {
                        Some(value as i64)
                    }
                    other => {
                        let error = match other {
                            Ok(Err(error)) => error.to_string(),
                            Err(_) => "panic while executing Lua timer".to_string(),
                            _ => "Lua timer must return a finite timestamp or nil".to_string(),
                        };
                        self.log_callback_failure_once(callback_id, &error);
                        None
                    }
                }
                .filter(|value| *value != i64::from(TIMER_OFF_VALUE) && *value != i64::MIN);
                if let Ok(mut timers) = self.custom_timers.lock() {
                    timers.values.insert(id, value);
                }
            }
        }
        self.custom_timers.lock().map(|timers| timers.values.clone()).unwrap_or_default()
    }

    pub fn callback_count(&self) -> usize {
        self.callbacks.len()
    }

    pub fn callback_path(&self, callback_id: usize) -> Option<&str> {
        self.callbacks.get(callback_id).map(|callback| callback.path.as_str())
    }

    /// Number of callback failures that produced a diagnostic. Repeated failures
    /// of the same callback remain log-once and do not increase this value.
    pub fn failure_log_count(&self) -> usize {
        self.failure_log_count
    }

    pub fn evaluate_draw(&mut self, callback_id: usize, state: &dyn LuaMainState) -> bool {
        self.evaluate_draw_inner(callback_id, Some(state))
    }

    /// Evaluate in a live `LuaRuntimeStateScope`, preserving per-call budgets.
    pub fn evaluate_draw_in_scope(&mut self, callback_id: usize) -> bool {
        self.evaluate_draw_inner(callback_id, None)
    }

    fn evaluate_draw_inner(
        &mut self,
        callback_id: usize,
        state: Option<&dyn LuaMainState>,
    ) -> bool {
        self.begin_runtime_callback();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.evaluate_callback_inner(callback_id, LuaRuntimeCallbackKind::Draw, state)
        }));
        match result {
            Ok(Ok(LuaRuntimeEvaluatedValue::Boolean(value))) => value,
            // LuaJ's `toboolean()` treats nil as false. This also lets a skin
            // keep a load-time-unavailable draw callback on the runtime path
            // without producing diagnostics for rows where it stays absent.
            Ok(Ok(LuaRuntimeEvaluatedValue::Nil)) => false,
            Ok(Ok(value)) => {
                self.log_callback_failure_once(
                    callback_id,
                    &format!("Lua draw callback returned {}, expected boolean", value.type_name()),
                );
                false
            }
            Ok(Err(error)) => {
                self.log_callback_failure_once(callback_id, &error.to_string());
                false
            }
            Err(_) => {
                self.log_callback_failure_once(callback_id, "panic while executing Lua callback");
                false
            }
        }
    }

    pub fn evaluate_number(&mut self, callback_id: usize, state: &dyn LuaMainState) -> Option<f64> {
        self.evaluate_number_inner(callback_id, Some(state))
    }

    pub fn evaluate_number_in_scope(&mut self, callback_id: usize) -> Option<f64> {
        self.evaluate_number_inner(callback_id, None)
    }

    fn evaluate_number_inner(
        &mut self,
        callback_id: usize,
        state: Option<&dyn LuaMainState>,
    ) -> Option<f64> {
        self.begin_runtime_callback();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.evaluate_callback_inner(callback_id, LuaRuntimeCallbackKind::Value, state)
        }));
        match result {
            Ok(Ok(LuaRuntimeEvaluatedValue::Integer(value))) => Some(value as f64),
            Ok(Ok(LuaRuntimeEvaluatedValue::Number(value))) if value.is_finite() => Some(value),
            Ok(Ok(LuaRuntimeEvaluatedValue::Number(value))) => {
                self.log_callback_failure_once(
                    callback_id,
                    &format!("Lua value callback returned non-finite number ({value})"),
                );
                None
            }
            Ok(Ok(value)) => {
                self.log_callback_failure_once(
                    callback_id,
                    &format!("Lua value callback returned {}, expected number", value.type_name()),
                );
                None
            }
            Ok(Err(error)) => {
                self.log_callback_failure_once(callback_id, &error.to_string());
                None
            }
            Err(_) => {
                self.log_callback_failure_once(callback_id, "panic while executing Lua callback");
                None
            }
        }
    }

    pub fn evaluate_text(
        &mut self,
        callback_id: usize,
        state: &dyn LuaMainState,
    ) -> Option<String> {
        self.evaluate_text_inner(callback_id, Some(state))
    }

    pub fn evaluate_text_in_scope(&mut self, callback_id: usize) -> Option<String> {
        self.evaluate_text_inner(callback_id, None)
    }

    fn evaluate_text_inner(
        &mut self,
        callback_id: usize,
        state: Option<&dyn LuaMainState>,
    ) -> Option<String> {
        self.begin_runtime_callback();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.evaluate_callback_inner(callback_id, LuaRuntimeCallbackKind::Value, state)
        }));
        match result {
            Ok(Ok(value)) => value.into_text().or_else(|| {
                self.log_callback_failure_once(
                    callback_id,
                    "Lua text callback returned unsupported value",
                );
                None
            }),
            Ok(Err(error)) => {
                self.log_callback_failure_once(callback_id, &error.to_string());
                None
            }
            Err(_) => {
                self.log_callback_failure_once(callback_id, "panic while executing Lua callback");
                None
            }
        }
    }

    fn begin_runtime_callback(&mut self) {
        let new_frame = std::mem::take(&mut self.pending_frame_start);
        self.instruction_budget.begin_runtime_callback(new_frame);
    }

    fn evaluate_callback_inner(
        &self,
        callback_id: usize,
        expected_kind: LuaRuntimeCallbackKind,
        state: Option<&dyn LuaMainState>,
    ) -> mlua::Result<LuaRuntimeEvaluatedValue> {
        let callback = self.callbacks.get(callback_id).ok_or_else(|| {
            mlua::Error::runtime(format!("unknown Lua callback ID {callback_id}"))
        })?;
        if callback.kind != expected_kind {
            return Err(mlua::Error::runtime(format!(
                "Lua callback kind mismatch at {}: expected {expected_kind:?}, registered {:?}",
                callback.path, callback.kind
            )));
        }
        let function = callback.function.as_ref().ok_or_else(|| {
            mlua::Error::runtime(format!("Lua callback was not registered at {}", callback.path))
        })?;
        if let Some(state) = state {
            return self.state_scope().with_state(state, || {
                function.call::<Value>(()).and_then(LuaRuntimeEvaluatedValue::from_lua)
            })?;
        }
        // An unbound call must fail safely, never read the load-time provider.
        let _: Function = self.main_state_dispatch.raw_get(1)?;
        function.call::<Value>(()).and_then(LuaRuntimeEvaluatedValue::from_lua)
    }

    /// Independent handle so the runtime can remain behind an adapter while a
    /// borrowed provider is installed for a synchronous group of callbacks.
    pub fn state_scope(&self) -> LuaRuntimeStateScope {
        LuaRuntimeStateScope {
            lua: self.lua.clone(),
            dispatch: self.main_state_dispatch.clone(),
            custom_timers: self.custom_timers.clone(),
            result_panel_bindings: self.result_panel_bindings.clone(),
        }
    }

    fn log_callback_failure_once(&mut self, callback_id: usize, error: &str) {
        if !self.failed_callbacks.insert(callback_id) {
            return;
        }
        self.failure_log_count = self.failure_log_count.saturating_add(1);
        let path = self.callback_path(callback_id).unwrap_or("<unknown>");
        tracing::warn!(
            skin = %self.skin_path.display(),
            callback_id,
            field_path = path,
            classification = "ERROR",
            error,
            "Lua callback failed; using safe fallback value"
        );
    }
}

pub struct LuaRuntimeStateScope {
    lua: Lua,
    dispatch: Table,
    custom_timers: Arc<Mutex<LuaCustomTimers>>,
    result_panel_bindings: Arc<LuaResultPanelBindings>,
}

impl LuaRuntimeStateScope {
    /// The provider cannot escape this call. Nested providers restore the outer
    /// dispatcher on success, error, or panic before mlua ends the borrowed scope.
    pub fn with_state<R>(
        &self,
        state: &dyn LuaMainState,
        run: impl FnOnce() -> R,
    ) -> mlua::Result<R> {
        // Covers both individual callbacks and the renderer's shared scope.
        // Restore on nesting/unwind without resetting any other closure state.
        let _panel = self.result_panel_bindings.bind(&self.lua, state.result_panel());
        self.lua.scope(|scope| {
            let dispatch = scope.create_function(
                |lua, (operation, argument, timer_value): (u8, Value, Option<i64>)| {
                    let id = || <i32 as mlua::FromLua>::from_lua(argument.clone(), lua);
                    Ok(match operation {
                        0 => {
                            Value::Boolean(lua_boolean_property(lua, &argument)?.is_some_and(
                                |property| state.option(property.id) ^ property.negated,
                            ))
                        }
                        1 => Value::Integer(
                            lua_property_id(lua, &argument, PropertyFamily::Integer)?
                                .map_or(0, |id| state.number(id)),
                        ),
                        2 => Value::Integer(state.exscore()),
                        3 | 4 => Value::Number(
                            lua_property_id(lua, &argument, PropertyFamily::Float)?
                                .map_or(0.0, |id| state.float(id)),
                        ),
                        5 => Value::String(
                            lua.create_string(
                                lua_property_id(lua, &argument, PropertyFamily::String)?
                                    .map_or_else(String::new, |id| state.text(id)),
                            )?,
                        ),
                        6 => {
                            let id = id()?;
                            let custom = self
                                .custom_timers
                                .lock()
                                .map_err(|_| mlua::Error::runtime("custom timer lock poisoned"))?
                                .values
                                .get(&id)
                                .copied();
                            Value::Integer(
                                custom
                                    .unwrap_or_else(|| state.timer_start_us(id))
                                    .unwrap_or(i64::from(TIMER_OFF_VALUE)),
                            )
                        }
                        7 => Value::Integer(i64::from(state.event_index(id()?))),
                        8 => Value::Integer(i64::from(state.gauge_type())),
                        9 => Value::Integer(state.time_us_i64()),
                        10 => Value::Integer(state.judge(id()?)),
                        11 => create_main_state_offset_table(lua, state.offset(id()?))?,
                        12 => Value::Integer(state.total_play_counts_in_session()),
                        13 => Value::Integer(state.total_play_notes_in_session()),
                        14 => Value::Integer(state.score_date_sec_time()),
                        15 => {
                            let id = id()?;
                            if !(10_000..=19_999).contains(&id) {
                                return Err(mlua::Error::runtime(
                                    "only custom timers are writable",
                                ));
                            }
                            let value = timer_value.filter(|value| {
                                *value != i64::from(TIMER_OFF_VALUE) && *value != i64::MIN
                            });
                            let mut timers = self
                                .custom_timers
                                .lock()
                                .map_err(|_| mlua::Error::runtime("custom timer lock poisoned"))?;
                            // beatoraja ignores writes to active (function-backed) timers.
                            if !timers.active.contains(&id) {
                                timers.values.insert(id, value);
                            }
                            Value::Boolean(true)
                        }
                        _ => return Err(mlua::Error::runtime("unknown main_state operation")),
                    })
                },
            )?;
            let previous = self.dispatch.raw_get(1)?;
            self.dispatch.raw_set(1, dispatch)?;
            let _guard = RuntimeDispatchGuard { slot: &self.dispatch, previous };
            Ok(run())
        })
    }
}

struct RuntimeDispatchGuard<'a> {
    slot: &'a Table,
    previous: Value,
}

impl Drop for RuntimeDispatchGuard<'_> {
    fn drop(&mut self) {
        let _ = self.slot.raw_set(1, self.previous.clone());
    }
}

/// Install stable accessors before the clean VM executes the skin so even
/// `local number = main_state.number` follows the current callback's state.
pub(super) fn install_runtime_main_state_dispatch(lua: &Lua) -> mlua::Result<Table> {
    let main_state: Table = lua.globals().get("bmz_main_state")?;
    // Private numeric slot avoids resolving a named registry key for every
    // main_state access. The borrowed dispatcher is cleared before its Lua
    // scope expires; access outside a callback uses the load-time provider.
    let dispatch_slot = lua.create_table_with_capacity(1, 0)?;
    for (operation, field) in [
        "option",
        "number",
        "exscore",
        "float",
        "float_number",
        "text",
        "timer",
        "event_index",
        "gauge_type",
        "time",
        "judge",
        "offset",
        "total_play_counts_in_session",
        "total_play_notes_in_session",
        "score_date_sec_time",
    ]
    .into_iter()
    .enumerate()
    {
        let original: Function =
            main_state.get(if field == "float" { "float_number" } else { field })?;
        let dispatch_slot = dispatch_slot.clone();
        main_state.set(
            field,
            lua.create_function(move |_, argument: Value| {
                match dispatch_slot.raw_get::<Value>(1)? {
                    Value::Function(dispatch) => {
                        dispatch.call::<Value>((operation as u8, argument))
                    }
                    _ => original.call::<Value>(argument),
                }
            })?,
        )?;
    }
    let timer_dispatch = dispatch_slot.clone();
    main_state.set(
        "set_timer",
        lua.create_function(move |_, (id, value): (i32, i64)| {
            match timer_dispatch.raw_get::<Value>(1)? {
                Value::Function(dispatch) => dispatch.call::<Value>((15_u8, id, value)),
                _ => Ok(Value::Boolean(true)),
            }
        })?,
    )?;
    install_main_state_read_helpers(lua, &main_state)?;
    Ok(dispatch_slot)
}

enum LuaRuntimeEvaluatedValue {
    Nil,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
}

impl LuaRuntimeEvaluatedValue {
    fn from_lua(value: Value) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(Self::Nil),
            Value::Boolean(value) => Ok(Self::Boolean(value)),
            Value::Integer(value) => Ok(Self::Integer(value)),
            Value::Number(value) => Ok(Self::Number(value)),
            Value::String(value) => Ok(Self::String(value.to_string_lossy())),
            value => Err(mlua::Error::runtime(format!(
                "Lua callback returned unsupported {} value",
                value.type_name()
            ))),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Boolean(_) => "boolean",
            Self::Integer(_) | Self::Number(_) => "number",
            Self::String(_) => "string",
        }
    }

    fn into_text(self) -> Option<String> {
        match self {
            Self::Nil => Some("nil".to_string()),
            Self::Boolean(value) => Some(value.to_string()),
            Self::Integer(value) => Some(value.to_string()),
            Self::Number(value) if value.is_finite() => Some(value.to_string()),
            Self::String(value) => Some(value),
            Self::Number(_) => None,
        }
    }
}
use super::*;
