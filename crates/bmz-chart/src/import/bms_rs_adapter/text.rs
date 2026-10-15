//! Source-derived `#TEXTxx` / `#SONGxx` definitions and channel #99 messages.
//!
//! bms-rs keys text events only by time and stops a message at its first
//! undefined object, so BMZ reads the random-resolved source text instead,
//! like the BGA channels. Object IDs are resolved after `#BASE` is known.
//!
//! `#SONGxx` is the obsolete form of `#TEXTxx`: for the same ID a `#TEXTxx`
//! definition wins regardless of order, even when its value is empty.
//! `#TEXT00` is nanasi's miss message and never becomes a channel #99 event.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;

#[derive(Debug, Clone)]
pub(super) struct TextMessage {
    measure: u64,
    object_count: u64,
    objects: Vec<SparseBmsObject>,
}

#[derive(Debug, Clone)]
struct TextDefinitionLine {
    id: String,
    value: Arc<str>,
    /// `#SONGxx`.
    legacy: bool,
}

/// Text commands found in one random-resolved BMS source.
#[derive(Debug, Clone, Default)]
pub(super) struct SourceTexts {
    definitions: Vec<TextDefinitionLine>,
    messages: Vec<TextMessage>,
}

pub(super) fn extract_source_texts(text: &str) -> SourceTexts {
    let mut source = SourceTexts::default();
    for line in text.lines() {
        if let Some((id, value, legacy)) = text_definition_line(line) {
            source.definitions.push(TextDefinitionLine {
                id: id.to_string(),
                value: value.into(),
                legacy,
            });
            continue;
        }
        let Some((channel, measure, object_count, objects)) = extract_object_message_line(line)
        else {
            continue;
        };
        if channel == "99" {
            source.messages.push(TextMessage { measure, object_count, objects });
        }
    }
    source
}

/// `#TEXTxx value` / `#SONGxx value` and whether it is `#SONGxx`.
/// One pair of enclosing double quotes is removed.
fn text_definition_line(line: &str) -> Option<(&str, &str, bool)> {
    let body = line.trim().strip_prefix('#')?.trim_start();
    let (name, value) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    let prefix = name.get(..4)?;
    let legacy = prefix.eq_ignore_ascii_case("SONG");
    if !(legacy || prefix.eq_ignore_ascii_case("TEXT")) {
        return None;
    }
    let id = &name[4..];
    (id.len() == 2 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| (id, unquote_text(value.trim()), legacy))
}

fn unquote_text(value: &str) -> &str {
    value.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')).unwrap_or(value)
}

/// Channel #99 objects and the `#TEXT00` miss message of one source.
#[derive(Debug, Clone, Default)]
pub(super) struct SourceTextObjects {
    pub(super) objects: Vec<IntermediateObject>,
    pub(super) miss_text: Option<Arc<str>>,
}

/// Builds channel #99 objects in source order. `00` is an empty slot. A
/// reference to an undefined ID clears the text, matching a `""` definition.
pub(super) fn source_text_objects(
    source: &SourceTexts,
    base62_obj_ids: bool,
    warnings: &mut Vec<ImportWarning>,
) -> SourceTextObjects {
    let mut definitions: HashMap<u16, (Arc<str>, bool)> = HashMap::new();
    let mut miss_text = None;
    for definition in &source.definitions {
        let Ok(id) = ObjId::try_from(&definition.id, base62_obj_ids) else {
            continue;
        };
        if id.as_u16() == 0 {
            // `#SONG00` is outside the compatible `01..ZZ` range.
            if !definition.legacy {
                miss_text = Some(definition.value.clone());
            }
            continue;
        }
        // Later definitions of the same kind win, like bms-rs `AlwaysWarnAndUseNewer`
        // for other headers. `#SONGxx` never replaces a `#TEXTxx` definition.
        match definitions.get(&id.as_u16()) {
            Some((_, false)) if definition.legacy => {}
            _ => {
                definitions.insert(id.as_u16(), (definition.value.clone(), definition.legacy));
            }
        }
    }

    let empty: Arc<str> = Arc::from("");
    let mut objects = Vec::new();
    for message in &source.messages {
        for object in &message.objects {
            let Some(time) = ObjTime::new(message.measure, object.index, message.object_count)
            else {
                continue;
            };
            let Ok(obj_id) = ObjId::try_from(&object.id, base62_obj_ids) else {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "InvalidTextObjectId".to_string(),
                    message: format!("channel 99 has invalid object id {:?}", object.id),
                });
                continue;
            };
            if obj_id.as_u16() == 0 {
                continue;
            }
            let text = definitions.get(&obj_id.as_u16()).map_or(&empty, |(text, _)| text).clone();
            objects.push(IntermediateObject {
                measure: track_of(time),
                position_num: time.numerator() as u32,
                position_den: time.denominator().get() as u32,
                kind: IntermediateObjectKind::SetText { text },
            });
        }
    }
    SourceTextObjects { objects, miss_text }
}
