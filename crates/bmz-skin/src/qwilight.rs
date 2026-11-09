//! Qwilight play-skin adapter. Independently implements the YAML/Lua asset contract;
//! no Qwilight application source or third-party skin assets are included.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use bmz_skin_document::{SKIN_REF_BMZ_KEY_MODE, SkinDocument};
use mlua::{ChunkMode, Function, Lua, MultiValue, Value};
use serde_json::{Value as Json, json};
use serde_yaml_ng::Value as Yaml;

use crate::{
    LoadedSkinDocument, LuaLoadRuntimeState, SkinLoadDependencies, SkinLoadWarning, SkinPathContext,
};

mod layout;
mod objects;
mod resources;
mod syntax;
#[cfg(test)]
mod tests;

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
            _ => None,
        }
    }
    fn property_name(&self, i: usize) -> String {
        self.text("lambda", &format!("configure-comment-{i}-1041"))
            .or_else(|| self.text("lambda", &format!("configure-comment-{i}")))
            .unwrap_or_else(|| format!("Qwilight configure {i}"))
    }
    fn choices(&self, i: usize) -> Vec<String> {
        self.text("lambda", &format!("configure-{i}"))
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default()
    }
    fn header(&self) -> Json {
        let properties: Vec<_> = (0..16)
            .filter_map(|i| {
                let choices = self.choices(i);
                (!choices.is_empty()).then(|| {
                    json!({
                        "name": self.property_name(i), "def": choices[0],
                        "item": choices.iter().take(128).enumerate().map(|(j, name)| json!({
                            "name": name, "op": 30000 + i * 128 + j
                        })).collect::<Vec<_>>()
                    })
                })
            })
            .collect();
        json!({"type": 0,
            "name": self.text("format", "title-1041").or_else(|| self.text("format", "title")).unwrap_or_else(|| "Qwilight play skin".into()),
            "property": properties, "source": [], "image": [], "destination": [],
            "value": [], "text": [], "panel": [], "judge": [], "graph": [], "sourceFrames": {}
        })
    }
}

pub fn load_qwilight_play_header(context: &SkinPathContext) -> Result<SkinDocument> {
    Ok(serde_json::from_value(Definition::read(context)?.header())?)
}

pub fn load_qwilight_play_skin(
    context: &SkinPathContext,
    options: &BTreeMap<String, String>,
    state: &LuaLoadRuntimeState,
) -> Result<LoadedSkinDocument> {
    let definition = Definition::read(context)?;
    let mode = *state.number_values.get(&SKIN_REF_BMZ_KEY_MODE).unwrap_or(&7);
    ensure!([4, 5, 6, 7, 8, 9, 10, 14].contains(&mode), "unsupported Qwilight key mode {mode}");
    let mut eval = Evaluator::new(definition, context, options)?;
    let layout = layout::Layout::build(&mut eval, mode)?;
    let resources = resources::Resources::new(context, &eval)?;
    let mut builder = objects::Builder::new(eval, layout, resources)?;
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
}
impl Evaluator {
    fn new(
        definition: Definition,
        context: &SkinPathContext,
        options: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let lua = crate::lua::new_layout_lua()?;
        let choices = lua.create_table()?;
        for i in 0..16 {
            let items = definition.choices(i);
            let selected = options.get(&definition.property_name(i));
            choices.set(
                i + 1,
                selected.and_then(|value| items.iter().position(|item| item == value)).unwrap_or(0),
            )?;
        }
        lua.globals().set("configures", choices)?;
        for name in [
            "mainPosition",
            "noteLength",
            "noteHeight",
            "bandPosition",
            "judgmentPaintPosition",
            "judgmentVisualizerPosition",
            "hitNotePaintArea",
            "judgmentMainPositionV3",
            "inputMapping",
        ] {
            lua.globals().set(name, 0)?;
        }
        lua.globals().set("judgmentMainPosition", 600)?;
        lua.globals().set("judgmentMainPositionV2", 1)?;
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
        Ok(Self { definition, lua })
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
