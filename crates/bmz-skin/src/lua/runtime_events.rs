//! Runtime execution of Lua `act` functions and `customEvents`.
//!
//! beatoraja runs a Lua `act` through `SkinLuaAccessor.loadEvent` when clicked,
//! updates `customEvents` once per frame after the custom timers
//! (`Skin.updateCustomObjects`), and `main_state.event_exec(id)` runs only the
//! custom events `1000..=1999` (`MainState.executeEvent`). Effects that leave
//! the VM are queued as [`SkinLuaEffect`] for the app.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};

use super::*;

/// Effects kept until the app drains them after a frame.
pub(super) const LUA_RUNTIME_EFFECT_LIMIT: usize = 256;
/// beatoraja has no limit and overflows its stack on recursive `event_exec`.
pub(super) const LUA_EVENT_NESTING_LIMIT: u32 = 16;
/// `SkinProperty.EVENT_CUSTOM_BEGIN..=EVENT_CUSTOM_END`.
pub(super) const LUA_CUSTOM_EVENT_IDS: std::ops::RangeInclusive<i32> = 1000..=1999;

#[derive(Debug, Clone)]
pub(super) struct LuaEventCallback {
    pub(super) path: String,
    pub(super) function: Function,
}

#[derive(Debug, Clone)]
pub(super) enum LuaCustomEventAction {
    Function(LuaEventCallback),
    /// A built-in event ID, executed by the app like a clicked `act`.
    Builtin(i32),
}

#[derive(Debug, Clone)]
pub(super) enum LuaCustomEventCondition {
    Function(LuaEventCallback),
    /// A boolean property ID; negative IDs are negated like beatoraja's factory.
    Option(i32),
}

#[derive(Debug, Clone)]
pub(super) struct LuaCustomEvent {
    pub(super) action: Option<LuaCustomEventAction>,
    pub(super) condition: Option<LuaCustomEventCondition>,
    pub(super) min_interval_ms: i64,
    /// False for events already converted to declarative skin audio. They are
    /// not updated here, but `event_exec` still runs their action.
    pub(super) observed: bool,
    pub(super) last_execute_us: Option<i64>,
}

#[derive(Debug)]
pub(super) struct LuaRuntimeEvents {
    skin_path: PathBuf,
    path_context: Option<SkinPathContext>,
    events: Mutex<BTreeMap<i32, LuaCustomEvent>>,
    effects: Mutex<Vec<SkinLuaEffect>>,
    effects_overflowed: AtomicBool,
    depth: AtomicU32,
    failed: Mutex<BTreeSet<String>>,
    failure_count: AtomicUsize,
}

pub(super) struct LuaEventGuard<'a> {
    depth: &'a AtomicU32,
}

impl Drop for LuaEventGuard<'_> {
    fn drop(&mut self) {
        self.depth.fetch_sub(1, Ordering::Relaxed);
    }
}

impl LuaRuntimeEvents {
    pub(super) fn new(
        skin_path: &Path,
        path_context: Option<SkinPathContext>,
        events: BTreeMap<i32, LuaCustomEvent>,
    ) -> Self {
        Self {
            skin_path: skin_path.to_path_buf(),
            path_context,
            events: Mutex::new(events),
            effects: Mutex::new(Vec::new()),
            effects_overflowed: AtomicBool::new(false),
            depth: AtomicU32::new(0),
            failed: Mutex::new(BTreeSet::new()),
            failure_count: AtomicUsize::new(0),
        }
    }

    pub(super) fn has_observed_events(&self) -> bool {
        self.events.lock().is_ok_and(|events| events.values().any(|event| event.observed))
    }

    /// Observed custom events in ID order, like beatoraja's update loop.
    pub(super) fn observed_ids(&self) -> Vec<i32> {
        self.events
            .lock()
            .map(|events| {
                events.iter().filter(|(_, event)| event.observed).map(|(id, _)| *id).collect()
            })
            .unwrap_or_default()
    }

    /// Condition of an observed event when its minimum interval has elapsed.
    pub(super) fn due_condition(&self, id: i32, now_us: i64) -> Option<LuaCustomEventCondition> {
        let events = self.events.lock().ok()?;
        let event = events.get(&id)?;
        let due = event
            .last_execute_us
            .is_none_or(|last| now_us.saturating_sub(last) / 1_000 >= event.min_interval_ms);
        due.then(|| event.condition.clone()).flatten()
    }

    /// Marks one level of `act` / event execution. None at the nesting limit.
    pub(super) fn enter(&self) -> Option<LuaEventGuard<'_>> {
        let depth = self.depth.fetch_add(1, Ordering::Relaxed);
        let guard = LuaEventGuard { depth: &self.depth };
        (depth < LUA_EVENT_NESTING_LIMIT).then_some(guard)
    }

    /// Whether an `act` or event action is running (effects are accepted only then).
    pub(super) fn in_event(&self) -> bool {
        self.depth.load(Ordering::Relaxed) > 0
    }

    pub(super) fn push_effect(&self, effect: SkinLuaEffect) {
        let Ok(mut effects) = self.effects.lock() else { return };
        if effects.len() < LUA_RUNTIME_EFFECT_LIMIT {
            effects.push(effect);
        } else if !self.effects_overflowed.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                skin = %self.skin_path.display(),
                limit = LUA_RUNTIME_EFFECT_LIMIT,
                "Lua skin requested too many effects in one frame; dropping the rest"
            );
        }
    }

    pub(super) fn take_effects(&self) -> Vec<SkinLuaEffect> {
        self.effects.lock().map(|mut effects| std::mem::take(&mut *effects)).unwrap_or_default()
    }

    /// The existing file inside the skin's allowed roots, as an absolute path
    /// the app can decode without the path context. None when missing.
    pub(super) fn resolve_audio_path(&self, requested: &str) -> Option<String> {
        let Some(path_context) = &self.path_context else {
            return Some(requested.to_string());
        };
        match path_context.resolve_file(requested) {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(_) => {
                self.log_failure_once(
                    &format!("audio:{requested}"),
                    "skin audio file not found; the sound is skipped",
                );
                None
            }
        }
    }

    pub(super) fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::Relaxed)
    }

    pub(super) fn log_failure_once(&self, path: &str, error: &str) {
        if !self.failed.lock().is_ok_and(|mut failed| failed.insert(path.to_string())) {
            return;
        }
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            skin = %self.skin_path.display(),
            field_path = path,
            classification = "ERROR",
            error,
            "Lua event failed; continuing like beatoraja"
        );
    }

    /// Calls an `act` / event function. Errors are logged once and swallowed,
    /// like beatoraja's `loadEvent` wrapper.
    pub(super) fn call(&self, callback: &LuaEventCallback, arg1: i32, arg2: i32) {
        let Some(_guard) = self.enter() else {
            self.log_failure_once(&callback.path, "Lua event nesting limit reached");
            return;
        };
        if let Err(error) = callback.function.call::<()>((arg1, arg2)) {
            self.log_failure_once(&callback.path, &error.to_string());
        }
    }

    /// beatoraja `MainState.executeEvent(id, arg1, arg2)`: only custom events
    /// run. Must be called with a bound `main_state` dispatcher.
    pub(super) fn execute(&self, id: i32, arg1: i32, arg2: i32, now_us: i64) {
        if !LUA_CUSTOM_EVENT_IDS.contains(&id) {
            return;
        }
        let Some(action) = self
            .events
            .lock()
            .ok()
            .and_then(|events| events.get(&id).map(|event| event.action.clone()))
        else {
            return;
        };
        match action {
            Some(LuaCustomEventAction::Function(callback)) => self.call(&callback, arg1, arg2),
            Some(LuaCustomEventAction::Builtin(event)) => {
                self.push_effect(SkinLuaEffect::Event { id: event, arg1, arg2 });
            }
            None => {}
        }
        if let Ok(mut events) = self.events.lock()
            && let Some(event) = events.get_mut(&id)
        {
            event.last_execute_us = Some(now_us);
        }
    }
}

/// LuaJ `toint()`: numbers truncate, numeric strings parse, anything else is 0.
pub(super) fn lua_to_int(value: &Value) -> i32 {
    fn truncate(value: f64) -> i32 {
        if value.is_finite() {
            value.trunc().clamp(i32::MIN as f64, i32::MAX as f64) as i32
        } else {
            0
        }
    }
    match value {
        Value::Integer(value) => *value as i32,
        Value::Number(value) => truncate(*value),
        Value::String(text) => text.to_string_lossy().trim().parse::<f64>().map_or(0, truncate),
        _ => 0,
    }
}

/// LuaJ `tofloat()`: numbers and numeric strings, anything else is 0.
pub(super) fn lua_to_float(value: &Value) -> f64 {
    let value = match value {
        Value::Integer(value) => *value as f64,
        Value::Number(value) => *value,
        Value::String(text) => text.to_string_lossy().trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    };
    if value.is_finite() { value } else { 0.0 }
}

/// Reads `customEvents` from the clean runtime VM. Later definitions of an ID
/// replace earlier ones, like beatoraja's `IntMap.put`.
pub(super) fn lua_custom_events_from_root(
    root: &Value,
    declarative_ids: &BTreeSet<i32>,
) -> mlua::Result<BTreeMap<i32, LuaCustomEvent>> {
    let mut events = BTreeMap::new();
    let Value::Table(root) = root else { return Ok(events) };
    let Ok(definitions) = root.get::<Table>("customEvents") else { return Ok(events) };
    for (index, definition) in definitions.sequence_values::<Value>().enumerate() {
        let Value::Table(definition) = definition? else { continue };
        let id = match definition.get::<Value>("id")? {
            value @ (Value::Integer(_) | Value::Number(_) | Value::String(_)) => lua_to_int(&value),
            _ => continue,
        };
        let path = format!("$.customEvents[{}]", index + 1);
        let callback = |field: &str, function: Function| LuaEventCallback {
            path: format!("{path}.{field}"),
            function,
        };
        let action = match definition.get::<Value>("action")? {
            Value::Function(function) => {
                Some(LuaCustomEventAction::Function(callback("action", function)))
            }
            value @ (Value::Integer(_) | Value::Number(_)) => {
                Some(LuaCustomEventAction::Builtin(lua_to_int(&value)))
            }
            _ => None,
        };
        let condition = match definition.get::<Value>("condition")? {
            Value::Function(function) => {
                Some(LuaCustomEventCondition::Function(callback("condition", function)))
            }
            value @ (Value::Integer(_) | Value::Number(_)) => {
                Some(LuaCustomEventCondition::Option(lua_to_int(&value)))
            }
            _ => None,
        };
        let min_interval_ms = i64::from(lua_to_int(&definition.get::<Value>("minInterval")?));
        events.insert(
            id,
            LuaCustomEvent {
                action,
                condition,
                min_interval_ms,
                observed: !declarative_ids.contains(&id),
                last_execute_us: None,
            },
        );
    }
    Ok(events)
}

/// Whether the skin root has `customEvents` that need the runtime VM.
pub(super) fn lua_root_has_runtime_custom_events(
    root: &Value,
    declarative_ids: &BTreeSet<i32>,
) -> bool {
    lua_custom_events_from_root(root, declarative_ids).is_ok_and(|events| {
        events
            .values()
            .any(|event| event.observed && (event.action.is_some() || event.condition.is_some()))
    })
}
