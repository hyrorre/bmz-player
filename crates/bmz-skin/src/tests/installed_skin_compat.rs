//! Snapshot and comparison harness for locally installed Lua skins.
//!
//! The test is ignored by default because third-party skins are not part of
//! the repository. It never writes below the skin root; snapshots and reports
//! go to the explicitly requested output path.
//!
//! ```text
//! BMZ_INSTALLED_SKIN_ROOT=$PWD/data/skins \
//! BMZ_SKIN_COMPAT_OUT=$PWD/.local/lua-api-compat/after.json \
//! BMZ_SKIN_COMPAT_BASELINE=$PWD/.local/lua-api-compat/before.json \
//! cargo test -p bmz-skin --lib installed_lua_skins -- --ignored --nocapture
//! ```
//!
//! Use absolute paths: the test runs in `crates/bmz-skin`.
//! `BMZ_SKIN_COMPAT_FILTER` limits entries to relative paths containing the
//! given text. `BMZ_SKIN_COMPAT_OPTIONS` / `BMZ_SKIN_COMPAT_FILES` select
//! non-default skin options and files (JSON objects). Several baseline runs
//! can be given separated by `;`. Without a baseline only the snapshot is
//! written.
//!
//! Runtime callbacks run in the renderer's destination order (see
//! `renderer_evaluation_order`), so `callback_results` of skins that share
//! globals between callbacks are stable between runs. Snapshots written
//! before that change evaluated in callback-ID order. Their format is the
//! same, but such skins may differ on frame 0 against them; take a new
//! baseline from the base commit.

use super::*;
use serde_json::{Value as JsonValue, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const FRAMES: i64 = 3;

/// Deterministic, non-trivial live state for runtime callback evaluation.
struct CompatProbeState {
    frame: i64,
}

impl LuaMainState for CompatProbeState {
    fn screen_size(&self) -> [u32; 2] {
        [1920, 1080]
    }

    fn result_panel(&self) -> Option<i32> {
        Some(1)
    }

    fn option(&self, id: i32) -> bool {
        (i64::from(id) + self.frame).rem_euclid(2) == 0
    }

    fn number(&self, id: i32) -> i64 {
        (i64::from(id) * 37 + self.frame * 11).rem_euclid(1000)
    }

    fn float(&self, id: i32) -> f64 {
        f64::from(id.rem_euclid(100)) / 100.0
    }

    fn text(&self, id: i32) -> String {
        format!("text{id}")
    }

    fn timer(&self, id: i32) -> Option<i32> {
        (id % 3 == 0).then_some(500 * self.frame as i32)
    }

    fn event_index(&self, id: i32) -> i32 {
        id.rem_euclid(4)
    }

    fn gauge_type(&self) -> i32 {
        2
    }

    fn time_us(&self) -> i32 {
        self.time_us_i64() as i32
    }

    fn time_us_i64(&self) -> i64 {
        10_000_000 + self.frame * 16_667
    }

    fn total_play_counts_in_session(&self) -> i64 {
        4
    }

    fn total_play_notes_in_session(&self) -> i64 {
        5_678
    }

    fn score_date_sec_time(&self) -> i64 {
        1_700_000_000
    }

    fn offset(&self, id: i32) -> LuaSkinOffsetValue {
        LuaSkinOffsetValue { x: id % 7, y: id % 5, w: 0, h: 0, r: 0, a: 0 }
    }
}

fn fnv1a(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn hash_json(value: &JsonValue) -> String {
    fnv1a(&serde_json::to_string(value).unwrap())
}

struct PathNormalizer {
    replacements: Vec<(String, &'static str)>,
}

impl PathNormalizer {
    fn new(root: &Path) -> Self {
        let mut variants = BTreeSet::new();
        let mut push = |path: &Path| {
            let text = path.to_string_lossy().to_string();
            for candidate in [text.clone(), text.trim_start_matches(r"\\?\").to_string()] {
                variants.insert(candidate.clone());
                variants.insert(candidate.replace('\\', "/"));
            }
        };
        push(root);
        if let Ok(canonical) = fs::canonicalize(root) {
            push(&canonical);
        }
        let mut replacements = variants
            .into_iter()
            .filter(|v| !v.is_empty())
            .map(|v| (v, "<SKINS>"))
            .collect::<Vec<_>>();
        // Longest first so the `\\?\` form is replaced before its suffix.
        replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.0.len()));
        Self { replacements }
    }

    fn text(&self, text: &str) -> String {
        let mut output = text.to_string();
        for (from, to) in &self.replacements {
            if output.contains(from.as_str()) {
                output = output.replace(from.as_str(), to);
            }
        }
        self.truncated_chunk_names(&output)
    }

    /// Lua shortens long chunk names to `[string "C:\...\data\skin..."]`.
    fn truncated_chunk_names(&self, text: &str) -> String {
        const OPEN: &str = "[string \"";
        const CLOSE: &str = "...\"]";
        let mut output = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find(OPEN) {
            let (before, tail) = rest.split_at(start + OPEN.len());
            output.push_str(before);
            match tail.find(CLOSE) {
                Some(end)
                    if end > 0
                        && self
                            .replacements
                            .iter()
                            .any(|(root, _)| root.starts_with(&tail[..end])) =>
                {
                    output.push_str("<SKINS>");
                    rest = &tail[end..];
                }
                _ => rest = tail,
            }
        }
        output.push_str(rest);
        output
    }

    fn json(&self, value: JsonValue) -> JsonValue {
        match value {
            JsonValue::String(text) => JsonValue::String(self.text(&text)),
            JsonValue::Array(values) => {
                JsonValue::Array(values.into_iter().map(|value| self.json(value)).collect())
            }
            JsonValue::Object(map) => JsonValue::Object(
                map.into_iter().map(|(k, v)| (self.text(&k), self.json(v))).collect(),
            ),
            value => value,
        }
    }
}

fn collect_entries(root: &Path, filter: Option<&str>) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin"))
            {
                entries.push(path);
            }
        }
    }
    entries.retain(|path| {
        let relative = relative_name(root, path);
        filter.is_none_or(|filter| relative.contains(filter))
    });
    entries.sort();
    entries
}

fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn error_text(normalizer: &PathNormalizer, error: &anyhow::Error) -> String {
    normalizer.text(&format!("{error:#}"))
}

fn snapshot_header(normalizer: &PathNormalizer, context: &SkinPathContext) -> JsonValue {
    match load_lua_skin_header_value_with_path_context(context) {
        Ok(loaded) => {
            let mut value = normalizer.json(loaded.value);
            // A mixed Lua table keeps its sequence part as "1", "2", ... keys.
            // The app's SkinDocument ignores them, and Luxe Flat / mz-select
            // fill them in `pairs` order while loading the header.
            if let JsonValue::Object(map) = &mut value {
                map.retain(|key, _| key.parse::<i64>().is_err());
            }
            let names = |key: &str, field: &str| -> Vec<String> {
                value
                    .get(key)
                    .and_then(JsonValue::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| match item.get(field) {
                                Some(JsonValue::String(text)) => text.clone(),
                                Some(other) => other.to_string(),
                                None => String::new(),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let properties = value
                .get("property")
                .and_then(JsonValue::as_array)
                .map(|properties| {
                    properties
                        .iter()
                        .map(|property| {
                            let items = property
                                .get("item")
                                .and_then(JsonValue::as_array)
                                .map(|items| {
                                    items
                                        .iter()
                                        .map(|item| {
                                            format!(
                                                "{}({})",
                                                item.get("name")
                                                    .and_then(JsonValue::as_str)
                                                    .unwrap_or(""),
                                                item.get("op")
                                                    .map(JsonValue::to_string)
                                                    .unwrap_or_default()
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("|")
                                })
                                .unwrap_or_default();
                            format!(
                                "{}=def:{}:{items}",
                                property.get("name").and_then(JsonValue::as_str).unwrap_or(""),
                                property.get("def").map(JsonValue::to_string).unwrap_or_default()
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "ok": true,
                "type": value.get("type"),
                "name": value.get("name"),
                "hash": hash_json(&value),
                "properties": properties,
                "filepaths": names("filepath", "name"),
                "offsets": names("offset", "name"),
                "warnings": loaded.warnings.iter().map(|w| normalizer.text(&w.message)).collect::<Vec<_>>(),
            })
        }
        Err(error) => json!({ "ok": false, "error": error_text(normalizer, &error) }),
    }
}

fn callback_id(value: Option<&JsonValue>, prefix: &str) -> Option<usize> {
    value?.as_str()?.trim().strip_prefix(prefix)?.parse().ok()
}

/// Callback IDs in the order the renderer reaches them within a frame.
///
/// Callback IDs follow the loader's field order, not the renderer's: by ID, a
/// `value` callback that sets a global can run after the `draw` that reads it
/// (and before the loader fixed its order, that changed between runs).
/// Destinations are walked in document order:
/// a Lua number `value` runs before its destination's `draw`
/// (`SkinNumber.prepare`) and a Lua `text` after it (`SkinText.prepare`).
/// Each callback runs once per frame, at its first use. Callbacks that no
/// destination reaches follow in field path order.
fn renderer_evaluation_order(document: &JsonValue, paths: &[String]) -> Vec<usize> {
    let by_destination_id = |section: &str| -> BTreeMap<&str, usize> {
        document[section]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?;
                Some((id, callback_id(item.get("value_expr"), lua::LUA_VALUE_CALLBACK_PREFIX)?))
            })
            .collect()
    };
    let (values, texts) = (by_destination_id("value"), by_destination_id("text"));
    let mut seen = vec![false; paths.len()];
    let mut order = Vec::with_capacity(paths.len());
    for destination in document["destination"].as_array().into_iter().flatten() {
        let id = destination.get("id").and_then(JsonValue::as_str);
        let draw = callback_id(destination.get("draw"), lua::LUA_DRAW_CALLBACK_PREFIX);
        for callback in [
            id.and_then(|id| values.get(id).copied()),
            draw,
            id.and_then(|id| texts.get(id).copied()),
        ]
        .into_iter()
        .flatten()
        {
            if seen.get(callback) == Some(&false) {
                seen[callback] = true;
                order.push(callback);
            }
        }
    }
    let mut rest = (0..paths.len()).filter(|&callback| !seen[callback]).collect::<Vec<_>>();
    rest.sort_by_key(|&callback| &paths[callback]);
    order.extend(rest);
    order
}

fn evaluate_runtime(
    runtime: &mut LuaSkinRuntime,
    paths: &[String],
    order: &[usize],
) -> (Vec<String>, Vec<String>, usize) {
    let mut results = paths.iter().map(|path| (path.clone(), Vec::new())).collect::<Vec<_>>();
    let mut timers = Vec::new();
    for frame in 0..FRAMES {
        let state = CompatProbeState { frame };
        runtime.begin_frame();
        let values = runtime.advance_custom_timers(&state);
        // Custom events run after the timers each frame, as in the app.
        runtime.advance_custom_events(&state);
        let effects = runtime.take_effects();
        timers.push(format!("{values:?} effects={effects:?}"));
        for &callback_id in order {
            let (path, outputs) = &mut results[callback_id];
            if runtime.callback_path(callback_id) != Some(path.as_str()) {
                outputs.push("unregistered".to_string());
                continue;
            }
            let output = if path.ends_with(".act") || path.ends_with(".event") {
                // Click handlers run only when clicked.
                "act".to_string()
            } else if path.ends_with(".draw") {
                runtime.evaluate_draw(callback_id, &state).to_string()
            } else if path.ends_with(".timer") {
                "timer".to_string()
            } else if path.contains("$.text[") {
                format!("{:?}", runtime.evaluate_text(callback_id, &state))
            } else {
                format!("{:?}", runtime.evaluate_number(callback_id, &state))
            };
            outputs.push(output);
        }
    }
    (
        results
            .into_iter()
            .map(|(path, outputs)| format!("{path} => {}", outputs.join(" | ")))
            .collect(),
        timers,
        runtime.failure_log_count(),
    )
}

/// Replaces IDs that BMZ allocates while walking Lua tables.
///
/// The IDs follow the conversion order, so they shift whenever inference
/// compiles or adds a callback (and differed between loads while the loader
/// walked tables in `pairs` order). Runtime callbacks become their field path,
/// and generated dynamic timers become their observed expression.
fn canonical_generated_ids(value: JsonValue, callback_paths: &[String]) -> JsonValue {
    let dynamic_timers = value
        .get("dynamicTimer")
        .and_then(JsonValue::as_array)
        .map(|timers| {
            timers
                .iter()
                .filter_map(|timer| {
                    Some((timer.get("id")?.as_i64()?, timer.get("observe")?.as_str()?.to_string()))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    fn walk(
        value: JsonValue,
        key: Option<&str>,
        callbacks: &[String],
        timers: &BTreeMap<i64, String>,
    ) -> JsonValue {
        match value {
            JsonValue::String(text) => {
                for prefix in [lua::LUA_DRAW_CALLBACK_PREFIX, lua::LUA_VALUE_CALLBACK_PREFIX] {
                    if let Some(path) = text
                        .strip_prefix(prefix)
                        .and_then(|id| id.parse::<usize>().ok())
                        .and_then(|id| callbacks.get(id))
                    {
                        return JsonValue::String(format!("{prefix}{path}"));
                    }
                }
                JsonValue::String(text)
            }
            JsonValue::Number(number)
                if key == Some("act")
                    && let Some(path) = number
                        .as_i64()
                        .and_then(|id| i32::try_from(id).ok())
                        .and_then(bmz_skin_document::lua_act_callback_id)
                        .and_then(|id| callbacks.get(id)) =>
            {
                JsonValue::String(format!("lua_act:{path}"))
            }
            JsonValue::Number(number)
                if matches!(key, Some("timer" | "id"))
                    && number.as_i64().is_some_and(|id| timers.contains_key(&id)) =>
            {
                JsonValue::String(format!("dynamic:{}", timers[&number.as_i64().unwrap()]))
            }
            JsonValue::Array(values) => JsonValue::Array(
                values.into_iter().map(|value| walk(value, key, callbacks, timers)).collect(),
            ),
            JsonValue::Object(map) => JsonValue::Object(
                map.into_iter()
                    .map(|(child, value)| {
                        let value = walk(value, Some(&child), callbacks, timers);
                        (child, value)
                    })
                    .collect(),
            ),
            value => value,
        }
    }
    let mut value = walk(value, None, callback_paths, &dynamic_timers);
    if let Some(JsonValue::Array(timers)) = value.get_mut("dynamicTimer") {
        timers.sort_by_key(|timer| timer.to_string());
    }
    value
}

/// Snapshot variants: the neutral initial load, a representative scene load
/// (song bar, difficulty, Result clear and scores) and the same scene in
/// `compat` mode, which keeps every supported function field in the runtime VM.
const DOCUMENT_VARIANTS: &[&str] = &["document", "document_scene", "document_compat"];

fn variant_load_state(variant: &str) -> LuaLoadRuntimeState {
    let mut state = LuaLoadRuntimeState { screen_size: [1920, 1080], ..Default::default() };
    if variant != "document" {
        state.option_values = BTreeMap::from([(2, true), (90, true), (153, true)]);
        state.number_values =
            BTreeMap::from([(71, 1500), (107, 85), (121, 1450), (150, 1400), (110, 900)]);
        state.text_values =
            BTreeMap::from([(10, "Title".to_string()), (12, "Full Title".to_string())]);
    }
    if variant == "document_compat" {
        state.runtime_mode = LuaSkinRuntimeMode::Compat;
    }
    state
}

/// Optional skin selections applied to every entry, as a JSON object of
/// `{"option or filepath name": "selected item"}` (combine with the filter).
fn configured_selections(variable: &str) -> BTreeMap<String, String> {
    std::env::var(variable)
        .ok()
        .map(|json| {
            serde_json::from_str(&json).unwrap_or_else(|error| panic!("{variable}: {error}"))
        })
        .unwrap_or_default()
}

fn snapshot_document(
    normalizer: &PathNormalizer,
    context: &SkinPathContext,
    runtime_state: &LuaLoadRuntimeState,
) -> JsonValue {
    let loaded = match lua::load_lua_skin_value_with_path_context(
        context,
        &configured_selections("BMZ_SKIN_COMPAT_OPTIONS"),
        &configured_selections("BMZ_SKIN_COMPAT_FILES"),
        runtime_state,
        &BTreeMap::new(),
    ) {
        Ok(loaded) => loaded,
        Err(error) => return json!({ "ok": false, "error": error_text(normalizer, &error) }),
    };
    let LoadedLuaSkinValue {
        value,
        mut lua_runtime,
        runtime_callback_paths,
        warnings,
        dependencies,
        ..
    } = loaded;
    let normalized = normalize_lua_skin_document(value);
    let parsed: Result<SkinDocument, _> = serde_path_to_error::deserialize(normalized.clone());
    let evaluation_order = renderer_evaluation_order(&normalized, &runtime_callback_paths);
    let value = canonical_generated_ids(normalizer.json(normalized), &runtime_callback_paths);
    let mut sections = BTreeMap::new();
    let mut elements = BTreeMap::new();
    let mut counts = BTreeMap::new();
    if let JsonValue::Object(map) = &value {
        for (key, entry) in map {
            sections.insert(key.clone(), hash_json(entry));
            if let JsonValue::Array(items) = entry {
                counts.insert(key.clone(), items.len());
                elements.insert(key.clone(), items.iter().map(hash_json).collect::<Vec<_>>());
            }
        }
    }
    let (fonts, sources, parse_error) = match &parsed {
        Ok(document) => (
            document
                .font
                .iter()
                .map(|font| {
                    let fallback = font
                        .fallback
                        .iter()
                        .map(|fallback| {
                            format!("{}#{}", normalizer.text(&fallback.path), fallback.font_type)
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "{}|{}|{}|{fallback}",
                        font.id,
                        normalizer.text(&font.path),
                        font.font_type
                    )
                })
                .collect::<Vec<_>>(),
            document
                .source
                .iter()
                .map(|source| format!("{}|{}", source.id, normalizer.text(&source.path)))
                .collect::<Vec<_>>(),
            None,
        ),
        Err(error) => (Vec::new(), Vec::new(), Some(normalizer.text(&error.to_string()))),
    };
    let (mut callback_results, custom_timers, failure_log_count) = match lua_runtime.as_mut() {
        Some(runtime) => evaluate_runtime(runtime, &runtime_callback_paths, &evaluation_order),
        None => (Vec::new(), Vec::new(), 0),
    };
    // Report by field path; the IDs shift between code versions.
    callback_results.sort();
    let mut runtime_callback_paths = runtime_callback_paths;
    runtime_callback_paths.sort();
    json!({
        "ok": parse_error.is_none(),
        "error": parse_error,
        "hash": hash_json(&value),
        "sections": sections,
        "elements": elements,
        "counts": counts,
        "fonts": fonts,
        "sources": sources,
        "warnings": warnings.iter().map(|w| normalizer.text(&w.message)).collect::<Vec<_>>(),
        "runtime_callbacks": runtime_callback_paths,
        "callback_results": callback_results,
        "custom_timers": custom_timers,
        "failure_log_count": failure_log_count,
        "dependencies": {
            "opaque": dependencies.opaque,
            "screen_size": dependencies.screen_size,
            "number_values": dependencies.number_values.keys().collect::<Vec<_>>(),
            "option_values": dependencies.option_values.keys().collect::<Vec<_>>(),
            "text_values": dependencies.text_values.keys().collect::<Vec<_>>(),
            "random_file_paths": dependencies.random_file_paths.keys().collect::<Vec<_>>(),
            "loaded_file_count": dependencies.loaded_files.len(),
        },
    })
}

fn snapshot_entry(root: &Path, normalizer: &PathNormalizer, entry: &Path) -> JsonValue {
    let context = match SkinPathContext::new(entry, [root.to_path_buf()]) {
        Ok(context) => context,
        Err(error) => return json!({ "context_error": error_text(normalizer, &error) }),
    };
    let run = || {
        catch_unwind(AssertUnwindSafe(|| {
            let mut snapshot = json!({ "header": snapshot_header(normalizer, &context) });
            for variant in DOCUMENT_VARIANTS {
                snapshot[*variant] =
                    snapshot_document(normalizer, &context, &variant_load_state(variant));
            }
            snapshot
        }))
        .unwrap_or_else(|_| json!({ "panic": true }))
    };
    let first = run();
    let second = run();
    let mut unstable = Vec::new();
    collect_differences("", &first, &second, &mut unstable);
    let mut snapshot = first;
    snapshot["unstable"] = json!(unstable);
    snapshot
}

fn collect_differences(path: &str, left: &JsonValue, right: &JsonValue, out: &mut Vec<String>) {
    match (left, right) {
        (JsonValue::Object(left), JsonValue::Object(right)) => {
            for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
                let child = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(l), Some(r)) => collect_differences(&child, l, r, out),
                    _ => out.push(child),
                }
            }
        }
        (JsonValue::Array(l), JsonValue::Array(r)) if l.len() == r.len() => {
            for (index, (l, r)) in l.iter().zip(r).enumerate() {
                collect_differences(&format!("{path}[{index}]"), l, r, out);
            }
        }
        _ if left != right => out.push(path.to_string()),
        _ => {}
    }
}

fn string_list(value: Option<&JsonValue>) -> BTreeSet<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().map_or_else(|| item.to_string(), str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn is_unstable(entry: &JsonValue, field: &str) -> bool {
    entry["unstable"].as_array().is_some_and(|paths| {
        paths.iter().filter_map(JsonValue::as_str).any(|path| path.starts_with(field))
    })
}

/// Hard regressions fail the test; soft changes are reported for review.
fn compare_entry(
    name: &str,
    before: &JsonValue,
    after: &JsonValue,
    regressions: &mut Vec<String>,
    changes: &mut Vec<String>,
) {
    for stage in std::iter::once("header").chain(DOCUMENT_VARIANTS.iter().copied()) {
        let was_ok = before[stage]["ok"].as_bool().unwrap_or(false);
        let is_ok = after[stage]["ok"].as_bool().unwrap_or(false);
        if was_ok && !is_ok {
            regressions.push(format!("{name}: {stage} load regressed: {}", after[stage]["error"]));
        } else if !was_ok && is_ok {
            changes.push(format!("{name}: {stage} now loads (was: {})", before[stage]["error"]));
        } else if !was_ok && before[stage]["error"] != after[stage]["error"] {
            changes.push(format!(
                "{name}: {stage} error changed: {} -> {}",
                before[stage]["error"], after[stage]["error"]
            ));
        }
    }
    if after.get("panic").is_some() && before.get("panic").is_none() {
        regressions.push(format!("{name}: loader panicked"));
    }
    let lost_properties = string_list(before["header"].get("properties"))
        .difference(&string_list(after["header"].get("properties")))
        .cloned()
        .collect::<Vec<_>>();
    if !lost_properties.is_empty() {
        regressions.push(format!("{name}: header options lost or changed: {lost_properties:?}"));
    }
    for list in ["filepaths", "offsets"] {
        let lost = string_list(before["header"].get(list))
            .difference(&string_list(after["header"].get(list)))
            .cloned()
            .collect::<Vec<_>>();
        if !lost.is_empty() {
            regressions.push(format!("{name}: header {list} lost: {lost:?}"));
        }
    }
    for variant in DOCUMENT_VARIANTS {
        compare_document(name, variant, before, after, regressions, changes);
    }
}

fn compare_document(
    name: &str,
    variant: &str,
    before: &JsonValue,
    after: &JsonValue,
    regressions: &mut Vec<String>,
    changes: &mut Vec<String>,
) {
    let (before_doc, after_doc) = (&before[variant], &after[variant]);
    if !before_doc["ok"].as_bool().unwrap_or(false) || !after_doc["ok"].as_bool().unwrap_or(false) {
        return;
    }
    let unstable = |field: &str| is_unstable(before, &format!(".{variant}.{field}"));
    let label = format!("{name} [{variant}]");
    if let Some(counts) = before_doc["counts"].as_object() {
        for (key, count) in counts {
            let previous = count.as_u64().unwrap_or(0);
            let current = after_doc["counts"][key].as_u64().unwrap_or(0);
            if current < previous && !unstable(&format!("counts.{key}")) {
                regressions.push(format!("{label}: {key} count decreased {previous} -> {current}"));
            }
        }
    }
    for list in ["fonts", "sources"] {
        let lost = string_list(before_doc.get(list))
            .difference(&string_list(after_doc.get(list)))
            .cloned()
            .collect::<Vec<_>>();
        if !lost.is_empty() && !unstable(list) {
            regressions.push(format!("{label}: {list} lost: {lost:?}"));
        }
    }
    // A callback that is now compiled is not a regression by itself; its
    // object stays in the document and is reviewed with the content diff.
    let lost_callbacks = string_list(before_doc.get("runtime_callbacks"))
        .difference(&string_list(after_doc.get("runtime_callbacks")))
        .cloned()
        .collect::<Vec<_>>();
    let new_callbacks = string_list(after_doc.get("runtime_callbacks"))
        .difference(&string_list(before_doc.get("runtime_callbacks")))
        .cloned()
        .collect::<Vec<_>>();
    if !unstable("runtime_callbacks") && (!lost_callbacks.is_empty() || !new_callbacks.is_empty()) {
        changes.push(format!(
            "{label}: runtime callbacks -{lost_callbacks:?} +{new_callbacks:?} (REVIEW)"
        ));
    }
    let new_warnings = string_list(after_doc.get("warnings"))
        .difference(&string_list(before_doc.get("warnings")))
        .cloned()
        .collect::<Vec<_>>();
    if !new_warnings.is_empty() && !unstable("warnings") {
        let (errors, others): (Vec<_>, Vec<_>) =
            new_warnings.into_iter().partition(|warning| warning.starts_with("ERROR"));
        if !errors.is_empty() {
            regressions.push(format!("{label}: new error warnings: {errors:?}"));
        }
        if !others.is_empty() {
            changes.push(format!("{label}: new warnings: {others:?}"));
        }
    }
    let resolved = string_list(before_doc.get("warnings"))
        .difference(&string_list(after_doc.get("warnings")))
        .cloned()
        .collect::<Vec<_>>();
    if !resolved.is_empty() && !unstable("warnings") {
        changes.push(format!("{label}: warnings no longer emitted: {resolved:?}"));
    }
    let failures_before = before_doc["failure_log_count"].as_u64().unwrap_or(0);
    let failures_after = after_doc["failure_log_count"].as_u64().unwrap_or(0);
    if failures_after > failures_before && !unstable("failure_log_count") {
        regressions.push(format!(
            "{label}: runtime callback failures {failures_before} -> {failures_after}"
        ));
    } else if failures_after < failures_before {
        changes.push(format!(
            "{label}: runtime callback failures {failures_before} -> {failures_after}"
        ));
    }
    if before_doc["hash"] != after_doc["hash"]
        && let (Some(left), Some(right)) =
            (before_doc["sections"].as_object(), after_doc["sections"].as_object())
    {
        let mut content = Vec::new();
        let mut order_only = Vec::new();
        let mut unstable_sections = Vec::new();
        for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
            if left.get(key) == right.get(key) {
                continue;
            }
            let elements =
                (before_doc["elements"][key].as_array(), after_doc["elements"][key].as_array());
            if unstable(&format!("sections.{key}")) && !matches!(elements, (Some(_), Some(_))) {
                unstable_sections.push(key.to_string());
                continue;
            }
            match elements {
                (Some(l), Some(r)) => {
                    let sorted = |items: &[JsonValue]| {
                        let mut items = items.iter().map(JsonValue::to_string).collect::<Vec<_>>();
                        items.sort();
                        items
                    };
                    let (l_sorted, r_sorted) = (sorted(l), sorted(r));
                    if l_sorted == r_sorted {
                        if !unstable(&format!("elements.{key}")) {
                            order_only.push(key.to_string());
                        }
                        continue;
                    }
                    let changed = (0..l.len().max(r.len()))
                        .filter(|&i| {
                            l.get(i) != r.get(i) && !unstable(&format!("elements.{key}[{i}]"))
                        })
                        .take(12)
                        .collect::<Vec<_>>();
                    if changed.is_empty() {
                        unstable_sections.push(key.to_string());
                        continue;
                    }
                    let removed = l_sorted.iter().filter(|item| !r_sorted.contains(item)).count();
                    let added = r_sorted.iter().filter(|item| !l_sorted.contains(item)).count();
                    content.push(format!("{key}{changed:?} (-{removed} +{added})"));
                }
                _ => content.push(key.to_string()),
            }
        }
        if !content.is_empty() {
            changes.push(format!("{label}: document content changed in {content:?}"));
        }
        if !order_only.is_empty() {
            changes.push(format!("{label}: document order changed in {order_only:?}"));
        }
        if !unstable_sections.is_empty() && content.is_empty() {
            changes.push(format!("{label}: unstable sections differ {unstable_sections:?}"));
        }
    }
    let callbacks_before = string_list(before_doc.get("callback_results"));
    let callbacks_after = string_list(after_doc.get("callback_results"));
    if callbacks_before != callbacks_after && !unstable("callback_results") {
        let changed = callbacks_after.difference(&callbacks_before).take(12).collect::<Vec<_>>();
        changes.push(format!("{label}: runtime callback results changed: {changed:?}"));
    }
}

#[test]
#[ignore = "requires locally installed skins via BMZ_INSTALLED_SKIN_ROOT"]
fn installed_lua_skins_match_compatibility_baseline() {
    let root = PathBuf::from(
        std::env::var("BMZ_INSTALLED_SKIN_ROOT")
            .expect("set BMZ_INSTALLED_SKIN_ROOT to the skin library root"),
    );
    let root = fs::canonicalize(&root).unwrap_or(root);
    let output =
        PathBuf::from(std::env::var("BMZ_SKIN_COMPAT_OUT").expect("set BMZ_SKIN_COMPAT_OUT"));
    assert!(!output.starts_with(&root), "snapshot output must not be written inside the skin root");
    let filter = std::env::var("BMZ_SKIN_COMPAT_FILTER").ok();
    let entries = collect_entries(&root, filter.as_deref());
    assert!(!entries.is_empty(), "no .luaskin entries below {}", root.display());

    let normalizer = PathNormalizer::new(&root);
    let next = AtomicUsize::new(0);
    let results = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(entry) = entries.get(index) else { break };
                    let started = std::time::Instant::now();
                    let snapshot = snapshot_entry(&root, &normalizer, entry);
                    let name = relative_name(&root, entry);
                    eprintln!("snapshot {name} ({} ms)", started.elapsed().as_millis());
                    results.lock().unwrap().insert(name, snapshot);
                }
            });
        }
    });
    let results = results.into_inner().unwrap();
    let snapshot = json!({ "entry_count": results.len(), "entries": results });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&output, serde_json::to_string_pretty(&snapshot).unwrap()).unwrap();
    eprintln!("wrote {} entries to {}", results_len(&snapshot), output.display());

    let Ok(baseline_paths) = std::env::var("BMZ_SKIN_COMPAT_BASELINE") else { return };
    // Several baseline runs may be given separated by `;`. Fields that already
    // differ between those runs (clock, random choices, hash-seed dependent
    // inference) are treated as unstable instead of as changes.
    let mut runs = baseline_paths
        .split(';')
        .filter(|path| !path.is_empty())
        .map(|path| serde_json::from_str::<JsonValue>(&fs::read_to_string(path).unwrap()).unwrap());
    let mut baseline = runs.next().expect("BMZ_SKIN_COMPAT_BASELINE is empty");
    for run in runs {
        let Some(entries) = baseline["entries"].as_object_mut() else { break };
        for (name, entry) in entries.iter_mut() {
            let mut differing = Vec::new();
            collect_differences("", entry, &run["entries"][name], &mut differing);
            if let Some(unstable) = entry["unstable"].as_array_mut() {
                unstable.extend(differing.into_iter().map(JsonValue::String));
            }
        }
    }
    let mut regressions = Vec::new();
    let mut changes = Vec::new();
    let before = baseline["entries"].as_object().unwrap();
    let after = snapshot["entries"].as_object().unwrap();
    for (name, previous) in before {
        if filter.as_deref().is_some_and(|filter| !name.contains(filter)) {
            continue;
        }
        match after.get(name) {
            Some(current) => compare_entry(name, previous, current, &mut regressions, &mut changes),
            None => regressions.push(format!("{name}: entry missing from current snapshot")),
        }
    }
    for name in after.keys().filter(|name| !before.contains_key(*name)) {
        changes.push(format!("{name}: new entry"));
    }
    let (noise, changes): (Vec<_>, Vec<_>) =
        changes.into_iter().partition(|change| change.contains("unstable sections differ"));
    let report = format!(
        "regressions ({}):\n{}\n\nchanges ({}):\n{}\n\nunstable in the baseline runs ({}):\n{}\n",
        regressions.len(),
        regressions.join("\n"),
        changes.len(),
        changes.join("\n"),
        noise.len(),
        noise.join("\n")
    );
    let report_path = output.with_extension("report.txt");
    fs::write(&report_path, &report).unwrap();
    eprintln!("{report}");
    assert!(regressions.is_empty(), "installed skin regressions; see {}", report_path.display());
}

fn results_len(snapshot: &JsonValue) -> usize {
    snapshot["entries"].as_object().map_or(0, |entries| entries.len())
}
