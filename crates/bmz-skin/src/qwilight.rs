//! Qwilight play-skin adapter. Independently implements the YAML/Lua asset contract;
//! no Qwilight application source or third-party skin assets are included.
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use bmz_skin_document::{SKIN_REF_BMZ_KEY_MODE, SkinDocument};
use mlua::{ChunkMode, Function, Lua, MultiValue, Table, Value};
use serde_json::{Value as Json, json};
use serde_yaml_ng::Value as Yaml;

use crate::{
    LoadedSkinDocument, LuaLoadRuntimeState, LuaSkinOffsetValue, SkinLoadDependencies,
    SkinLoadWarning, SkinPathContext,
};

mod customize;
mod layout;
mod objects;
mod resources;
mod syntax;
#[cfg(test)]
mod tests;
mod visibility;

pub use customize::{
    QWILIGHT_LANE_COVER_DIR, QWILIGHT_LANE_COVER_PROPERTY, is_qwilight_lane_cover_image_path,
};

/// Qwilight reads `configure-0` to `configure-15`.
const CONFIGURE_COUNT: usize = 16;
/// BMZ UI locale code and Qwilight LCID. Qwilight uses 1033 for every other language.
const LOCALES: [(&str, &str); 3] = [("ja", "1041"), ("ko", "1042"), ("en", "1033")];

pub fn is_qwilight_play_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("yaml") || v.eq_ignore_ascii_case("yml"))
        && !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('@'))
}

struct Definition {
    yaml: Yaml,
}
impl Definition {
    fn read(context: &SkinPathContext) -> Result<Self> {
        ensure!(
            is_qwilight_play_skin_path(context.entry_file()),
            "Qwilight themes are not play skins"
        );
        let path = context.policy().resolve(context.entry_file())?;
        ensure!(fs::metadata(&path)?.len() <= 2 * 1024 * 1024, "Qwilight YAML exceeds size limit");
        let yaml: Yaml = serde_yaml_ng::from_str(&fs::read_to_string(path)?)?;
        for section in ["format", "lambda", "frame", "point"] {
            ensure!(yaml[section].is_mapping(), "Qwilight play skin is missing {section}");
        }
        Ok(Self { yaml })
    }
    fn text(&self, section: &str, key: &str) -> Option<String> {
        match &self.yaml[section][key] {
            Yaml::String(v) => Some(v.clone()),
            Yaml::Number(v) => Some(v.to_string()),
            Yaml::Bool(v) => Some(v.to_string()),
            // Perfect inherits the full-combo layout/timing when omitted.
            _ => match (section, key) {
                ("frame", "yell!-frame" | "yell!-framerate")
                | (
                    "point",
                    "yell!System" | "yell!Position0" | "yell!Position1" | "yell!Length"
                    | "yell!Height",
                ) => self.text(section, &key.replacen("yell!", "band!", 1)),
                ("lambda", "alt-yell!") => self.text(section, "alt-band!"),
                _ => None,
            },
        }
    }
    /// `setMainPosition` and similar capability flags; Qwilight defaults them to false.
    fn flag(&self, key: &str) -> bool {
        self.text("lambda", key).is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    }
    /// The saved option key. It stays language independent, as before localized labels.
    fn property_name(&self, i: usize) -> String {
        self.text("lambda", &format!("configure-comment-{i}-1041"))
            .or_else(|| self.text("lambda", &format!("configure-comment-{i}")))
            .unwrap_or_else(|| format!("Qwilight configure {i}"))
    }
    fn split(text: &str) -> Vec<String> {
        text.split(',').map(|s| s.trim().to_string()).collect()
    }
    /// The language-independent choices that BMZ saves and passes to Lua by index.
    fn choices(&self, i: usize) -> Vec<String> {
        self.text("lambda", &format!("configure-{i}")).map(|s| Self::split(&s)).unwrap_or_default()
    }
    /// Display text from `configure-comment-{i}-{LCID}` and `configure-{i}-{LCID}`.
    /// A localized list only labels the choices when it has the same length.
    fn property_labels(&self, i: usize) -> Json {
        let choices = self.choices(i);
        let mut name = serde_json::Map::new();
        let mut items: BTreeMap<String, serde_json::Map<String, Json>> = BTreeMap::new();
        for (locale, lcid) in LOCALES {
            let comment = self.text("lambda", &format!("configure-comment-{i}-{lcid}"));
            let comment = if locale == "en" {
                comment.or_else(|| self.text("lambda", &format!("configure-comment-{i}")))
            } else {
                comment
            };
            if let Some(comment) = comment {
                name.insert(locale.into(), json!(comment));
            }
            let Some(localized) = self.text("lambda", &format!("configure-{i}-{lcid}")) else {
                continue;
            };
            let localized = Self::split(&localized);
            if localized.len() != choices.len() {
                continue;
            }
            for (choice, label) in choices.iter().zip(localized) {
                items.entry(choice.clone()).or_default().insert(locale.into(), json!(label));
            }
        }
        json!({"name": name, "items": items})
    }
    /// `available[i]` hides an option whose `configure-available-{i}` is false.
    fn header(&self, available: &[bool; CONFIGURE_COUNT], lane_covers: &[String]) -> Json {
        let mut labels = serde_json::Map::new();
        let mut properties: Vec<_> = (0..CONFIGURE_COUNT)
            .filter_map(|i| {
                let choices = self.choices(i);
                (!choices.is_empty() && available[i]).then(|| {
                    let name = self.property_name(i);
                    labels.insert(name.clone(), self.property_labels(i));
                    json!({
                        "name": name, "def": choices[0],
                        "item": choices.iter().take(128).enumerate().map(|(j, name)| json!({
                            "name": name, "op": 30000 + i * 128 + j
                        })).collect::<Vec<_>>()
                    })
                })
            })
            .collect();
        visibility::properties(&mut properties, &mut labels);
        properties.push(customize::lane_cover_property(lane_covers));
        json!({"type": 0,
            // Qwilight's transitions belong to the separate @theme. Use BMZ's
            // default fade for play-only imports; don't start notes under black.
            "bmzPlayFadein": 500, "bmzPlayFadeout": 500, "bmzPlayClearCompletion": true,
            "loadstart": 500, "fadeout": 500, "close": 500,
            "name": self.text("format", "title-1041").or_else(|| self.text("format", "title")).unwrap_or_else(|| "Qwilight play skin".into()),
            "property": properties, "propertyLabels": labels,
            "offset": customize::adjustment_defs(self),
            "source": [], "image": [], "destination": [], "slider": [],
            "value": [], "text": [], "panel": [], "judge": [], "graph": [],
            "qwilightVisualizer": [], "sourceFrames": {}
        })
    }
}

/// Catalog header: lists every option without running the skin's Lua.
pub fn load_qwilight_play_header(context: &SkinPathContext) -> Result<SkinDocument> {
    let definition = Definition::read(context)?;
    let lane_covers = customize::lane_cover_names(context);
    Ok(serde_json::from_value(definition.header(&[true; CONFIGURE_COUNT], &lane_covers))?)
}

/// Settings header: runs the skin's Lua with the slot's options so that
/// `configure-available-*` hides options exactly as Qwilight's settings window.
pub fn load_qwilight_play_header_with_options(
    context: &SkinPathContext,
    options: &BTreeMap<String, String>,
) -> Result<SkinDocument> {
    let eval = Evaluator::new(Definition::read(context)?, context, options, &BTreeMap::new())?;
    let lane_covers = customize::lane_cover_names(context);
    Ok(serde_json::from_value(eval.definition.header(&eval.available, &lane_covers))?)
}

pub fn load_qwilight_play_skin(
    context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    state: &LuaLoadRuntimeState,
) -> Result<LoadedSkinDocument> {
    let definition = Definition::read(context)?;
    let mode = *state.number_values.get(&SKIN_REF_BMZ_KEY_MODE).unwrap_or(&7);
    ensure!([4, 5, 6, 7, 8, 9, 10, 14].contains(&mode), "unsupported Qwilight key mode {mode}");
    let mut eval = Evaluator::new(definition, context, options, &state.offset_values)?;
    let layout = layout::Layout::build(&mut eval, mode)?;
    let resources = resources::Resources::new(context, &eval)?;
    let lane_cover = customize::LaneCoverChoice::from_options(options);
    let mut builder = objects::Builder::new(eval, layout, resources, context, lane_cover)?;
    builder.build()?;
    let document =
        serde_json::from_value(builder.document).context("invalid converted Qwilight play skin")?;
    Ok(LoadedSkinDocument {
        document,
        lua_runtime: None,
        files: BTreeMap::new(),
        dependencies: SkinLoadDependencies { opaque: true, ..Default::default() },
        warnings: builder.warnings.into_iter().map(|message| SkinLoadWarning { message }).collect(),
    })
}

struct Evaluator {
    definition: Definition,
    lua: Lua,
    /// Selections as saved. Qwilight loads lambda/format values and materials with them.
    load_configures: Table,
    /// Hidden options read as their first choice, as Qwilight's per-play layout.
    play_configures: Table,
    play_phase: Cell<bool>,
    available: [bool; CONFIGURE_COUNT],
    hidden_pipelines: Vec<i32>,
    remaining_time: bool,
}
impl Evaluator {
    fn new(
        definition: Definition,
        context: &SkinPathContext,
        options: &BTreeMap<String, String>,
        offsets: &BTreeMap<String, LuaSkinOffsetValue>,
    ) -> Result<Self> {
        let lua = crate::lua::new_layout_lua()?;
        let selected: Vec<usize> = (0..CONFIGURE_COUNT)
            .map(|i| {
                let items = definition.choices(i);
                options
                    .get(&definition.property_name(i))
                    .and_then(|value| items.iter().position(|item| item == value))
                    .unwrap_or(0)
            })
            .collect();
        let load_configures = lua.create_sequence_from(selected.iter().copied())?;
        lua.globals().set("configures", load_configures.clone())?;
        customize::set_adjustment_globals(&lua, &definition, offsets)?;
        lua.globals().set("inputMapping", 0)?;
        lua.globals().set("judgmentStage", 100)?;
        let stem = context.entry_file().file_stem().context("skin has no name")?.to_string_lossy();
        let name = definition.text("format", "lua").unwrap_or_else(|| stem.into_owned());
        let path = context.entry_dir().join(name).with_extension("lua");
        if path.exists() {
            let path = context.policy().resolve(&path)?;
            ensure!(
                fs::metadata(&path)?.len() <= 2 * 1024 * 1024,
                "Qwilight Lua exceeds size limit"
            );
            let source = syntax::normalize_lua(&fs::read_to_string(&path)?);
            lua.load(source).set_name(path.to_string_lossy()).set_mode(ChunkMode::Text).exec()?;
        }
        let mut eval = Self {
            definition,
            lua,
            play_configures: load_configures.clone(),
            load_configures,
            play_phase: Cell::new(false),
            available: [true; CONFIGURE_COUNT],
            hidden_pipelines: visibility::hidden(options),
            remaining_time: options
                .get("Qwilight time display")
                .is_some_and(|value| value == "Remaining"),
        };
        for i in 0..CONFIGURE_COUNT {
            eval.available[i] = eval.configure_available(i);
        }
        let play: Vec<usize> = selected
            .iter()
            .enumerate()
            .map(|(i, &value)| if eval.available[i] { value } else { 0 })
            .collect();
        eval.play_configures = eval.lua.create_sequence_from(play)?;
        Ok(eval)
    }
    /// Anything but `true` hides the option. A Lua error stops Qwilight from
    /// loading the skin; BMZ keeps that option visible so it can be changed back.
    fn configure_available(&self, i: usize) -> bool {
        let Some(text) = self.definition.text("lambda", &format!("configure-available-{i}")) else {
            return true;
        };
        self.use_phase(false);
        match self.evaluate(&text) {
            Ok(Value::Boolean(value)) => value,
            Ok(Value::String(value)) => {
                value.to_str().is_ok_and(|v| v.trim().eq_ignore_ascii_case("true"))
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!(%error, configure = i, "Qwilight configure-available failed");
                true
            }
        }
    }
    /// Lambda/format values and material names are read when Qwilight loads the
    /// skin, while point/frame and `alt-*` values are evaluated for each play.
    fn use_phase(&self, play: bool) {
        if self.play_phase.replace(play) != play {
            let table = if play { &self.play_configures } else { &self.load_configures };
            let _ = self.lua.globals().set("configures", table.clone());
        }
    }
    fn section_phase(&self, section: &str, key: &str) {
        self.use_phase(match section {
            "point" | "frame" => true,
            "lambda" => key.starts_with("alt-"),
            _ => false,
        });
    }
    fn evaluate(&self, text: &str) -> Result<Value> {
        let text = text.trim();
        if let Ok(number) = text.parse::<f64>() {
            return Ok(Value::Number(number));
        }
        if let Some((name, arguments)) = text.split_once('(') {
            ensure!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "invalid Qwilight function name"
            );
            let arguments =
                arguments.strip_suffix(')').context("invalid Qwilight function call")?;
            let values: Result<Vec<Value>> = arguments
                .split(',')
                .filter(|arg| !arg.trim().is_empty())
                .map(|arg| {
                    if let Ok(value) = arg.trim().parse::<f64>() {
                        Ok(Value::Number(value))
                    } else {
                        Ok(Value::String(self.lua.create_string(arg.trim())?))
                    }
                })
                .collect();
            let function: Function = self
                .lua
                .globals()
                .get(name)
                .with_context(|| format!("Qwilight function {name}"))?;
            return function
                .call(MultiValue::from_vec(values?))
                .with_context(|| format!("Qwilight {text}"));
        }
        Ok(Value::String(self.lua.create_string(text)?))
    }
    fn number(&self, section: &str, key: &str, default: f64) -> Result<f64> {
        self.section_phase(section, key);
        match self.definition.text(section, key) {
            Some(value) => self.numeric(&value),
            None => Ok(default),
        }
    }
    fn numeric(&self, text: &str) -> Result<f64> {
        let value = match self.evaluate(text)? {
            Value::Number(v) => v,
            Value::Integer(v) => v as f64,
            Value::String(v) => v.to_str()?.parse().context("non-numeric Qwilight value")?,
            _ => bail!("non-numeric Qwilight value: {text}"),
        };
        ensure!(
            value.is_finite() && value.abs() <= 1_000_000.0,
            "Qwilight value out of range: {text}"
        );
        Ok(value)
    }
    fn list(&self, section: &str, key: &str) -> Result<Vec<String>> {
        self.section_phase(section, key);
        let Some(text) = self.definition.text(section, key) else {
            return Ok(Vec::new());
        };
        let mut depth = 0;
        let mut start = 0;
        let mut values = Vec::new();
        for (i, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    values.push(text[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            }
        }
        values.push(text[start..].trim().to_string());
        if values.len() == 1 && text.contains('(') {
            values = match self.evaluate(&text)? {
                Value::String(value) => {
                    ensure!(
                        value.as_bytes().len() <= 2 * 1024 * 1024,
                        "Qwilight list exceeds size limit"
                    );
                    value.to_str()?.split(',').take(513).map(|s| s.trim().to_string()).collect()
                }
                Value::Number(value) => vec![value.to_string()],
                Value::Integer(value) => vec![value.to_string()],
                _ => bail!("invalid Qwilight list value: {text}"),
            };
        }
        ensure!(values.len() <= 512, "too many Qwilight values");
        Ok(values)
    }
    fn lane_value(&self, key: &str, drawing: usize, default: f64) -> Result<f64> {
        let values = self.list("point", key)?;
        let mut last = None;
        for i in 0..drawing {
            if let Some(value) = values.get(i).filter(|v| v.as_str() != "~" && !v.is_empty()) {
                last = Some(value);
            }
        }
        last.map_or(Ok(default), |value| self.numeric(value))
    }
    fn prefix(&self, name: &str, arguments: &[usize], default: &str) -> Result<String> {
        self.signed_prefix(
            name,
            &arguments.iter().map(|value| *value as i64).collect::<Vec<_>>(),
            default,
        )
    }
    fn signed_prefix(&self, name: &str, arguments: &[i64], default: &str) -> Result<String> {
        self.use_phase(false);
        let value: Value = self.lua.globals().get(name)?;
        if let Value::Function(function) = value {
            let args = self.lua.create_sequence_from(arguments.iter().copied())?;
            let value = function.call::<mlua::String>(args)?;
            ensure!(value.as_bytes().len() <= 4096, "Qwilight material prefix is too long");
            Ok(value.to_str()?.to_string())
        } else {
            Ok(default.to_string())
        }
    }
}
