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
    pub(super) key: Option<RegistryKey>,
}

/// A Lua-only sidecar that owns the runtime VM and every callback registry key.
///
/// The VM is intentionally not cloneable. Its callbacks are obtained by a second
/// load after inference has completed, so inference can never mutate runtime
/// closure state, module state, or the Lua random-number generator.
pub struct LuaSkinRuntime {
    pub(super) lua: Lua,
    pub(super) callbacks: Vec<LuaRuntimeCallback>,
    pub(super) main_state_key: RegistryKey,
    pub(super) instruction_budget: LuaInstructionBudget,
    pub(super) skin_path: PathBuf,
    pub(super) failed_callbacks: BTreeSet<usize>,
    pub(super) failure_log_count: usize,
    pub(super) pending_frame_start: bool,
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
        state: &dyn LuaMainState,
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
        let key = callback.key.as_ref().ok_or_else(|| {
            mlua::Error::runtime(format!("Lua callback was not registered at {}", callback.path))
        })?;
        let function: Function = self.lua.registry_value(key)?;
        let main_state: Table = self.lua.registry_value(&self.main_state_key)?;

        self.lua.scope(|scope| {
            const FIELDS: &[&str] = &[
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
            ];
            let originals = FIELDS
                .iter()
                .map(|field| Ok((*field, main_state.get::<Value>(*field)?)))
                .collect::<mlua::Result<Vec<_>>>()?;

            main_state.set("option", scope.create_function(|_, id: i32| Ok(state.option(id)))?)?;
            main_state.set("number", scope.create_function(|_, id: i32| Ok(state.number(id)))?)?;
            main_state.set("exscore", scope.create_function(|_, ()| Ok(state.exscore()))?)?;
            let float = scope.create_function(|_, id: i32| Ok(state.float(id)))?;
            main_state.set("float", float.clone())?;
            main_state.set("float_number", float)?;
            main_state.set("text", scope.create_function(|_, id: i32| Ok(state.text(id)))?)?;
            main_state.set(
                "timer",
                scope
                    .create_function(|_, id: i32| Ok(state.timer(id).unwrap_or(TIMER_OFF_VALUE)))?,
            )?;
            main_state.set(
                "event_index",
                scope.create_function(|_, id: i32| Ok(state.event_index(id)))?,
            )?;
            main_state.set("gauge_type", scope.create_function(|_, ()| Ok(state.gauge_type()))?)?;
            main_state.set("time", scope.create_function(|_, ()| Ok(state.time_us()))?)?;
            main_state
                .set("judge", scope.create_function(|_, index: i32| Ok(state.judge(index)))?)?;
            main_state.set(
                "offset",
                scope.create_function(|lua, id: i32| {
                    create_main_state_offset_table(lua, state.offset(id))
                })?,
            )?;

            let result = function.call::<Value>(()).and_then(LuaRuntimeEvaluatedValue::from_lua);

            // Scoped functions borrow the current frame snapshot. Restore the
            // persistent load-time stubs before the scope invalidates them.
            for (field, value) in originals {
                main_state.set(field, value)?;
            }
            result
        })
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
