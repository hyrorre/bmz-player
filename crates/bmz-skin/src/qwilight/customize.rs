//! Qwilight's player-side customization mapped onto BMZ skin settings.
//!
//! Qwilight keeps lane/note/judgment adjustments in its own configuration and
//! exposes them to the skin's Lua as globals. BMZ stores them as named offsets
//! of the play slot, so they follow the existing save, history and reload paths.
use super::*;

/// Folder below a skin library root that holds lane cover images added in BMZ.
pub const QWILIGHT_LANE_COVER_DIR: &str = "qwilight-veil";
/// BMZ-defined option selecting the lane cover (Qwilight "veil") image.
pub const QWILIGHT_LANE_COVER_PROPERTY: &str = "Lane cover image";
const LANE_COVER_SKIN: &str = "Skin";
const LANE_COVER_RANDOM: &str = "Random";
/// Leaves room below the per-property op range used by the configure options.
const LANE_COVER_LIMIT: usize = 120;
const LANE_COVER_OP: usize = 30000 + CONFIGURE_COUNT * 128;

pub fn is_qwilight_lane_cover_image_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        ["png", "jpg", "jpeg", "bmp", "webp"].contains(&ext.to_ascii_lowercase().as_str())
    })
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    /// BMZ offsets move up for positive y; Qwilight's adjustments move down.
    Y,
    W,
    H,
}

struct Adjustment {
    flag: &'static str,
    name: &'static str,
    id: i32,
    global: &'static str,
    axis: Axis,
}

/// IDs are outside beatoraja's runtime offset range, so only the load-time
/// Lua globals use them.
const ADJUSTMENTS: [Adjustment; 7] = [
    Adjustment {
        flag: "setMainPosition",
        name: "Qwilight lane position",
        id: 1001,
        global: "mainPosition",
        axis: Axis::X,
    },
    Adjustment {
        flag: "setNoteLength",
        name: "Qwilight note width",
        id: 1002,
        global: "noteLength",
        axis: Axis::W,
    },
    Adjustment {
        flag: "setNoteHeight",
        name: "Qwilight note height",
        id: 1003,
        global: "noteHeight",
        axis: Axis::H,
    },
    Adjustment {
        flag: "setHitNotePaintArea",
        name: "Qwilight hit effect size",
        id: 1004,
        global: "hitNotePaintArea",
        axis: Axis::W,
    },
    Adjustment {
        flag: "setBandPosition",
        name: "Qwilight combo position",
        id: 1006,
        global: "bandPosition",
        axis: Axis::Y,
    },
    Adjustment {
        flag: "setJudgmentPaintPosition",
        name: "Qwilight judgment position",
        id: 1007,
        global: "judgmentPaintPosition",
        axis: Axis::Y,
    },
    Adjustment {
        flag: "setJudgmentVisualizerPosition",
        name: "Qwilight judgment visualizer position",
        id: 1008,
        global: "judgmentVisualizerPosition",
        axis: Axis::Y,
    },
];

/// Only adjustments that the skin declares with its `set*` flag are shown.
pub(super) fn adjustment_defs(definition: &Definition) -> Vec<Json> {
    ADJUSTMENTS
        .iter()
        .filter(|adjustment| definition.flag(adjustment.flag))
        .map(|adjustment| {
            let axis = adjustment.axis;
            json!({
                "category": "Qwilight", "name": adjustment.name, "id": adjustment.id,
                "x": matches!(axis, Axis::X), "y": matches!(axis, Axis::Y),
                "w": matches!(axis, Axis::W), "h": matches!(axis, Axis::H),
                "r": false, "a": false
            })
        })
        .collect()
}

/// Sets the globals that Qwilight's ComputedState passes to the skin's Lua.
/// An undeclared adjustment reads 0, and values keep Qwilight's ±1000 px range.
pub(super) fn set_adjustment_globals(
    lua: &Lua,
    definition: &Definition,
    offsets: &BTreeMap<String, LuaSkinOffsetValue>,
) -> Result<()> {
    let globals = lua.globals();
    // The judgement line is moved by BMZ's live LIFT offset. Ignore the old
    // saved skin adjustment, including when a profile still contains it.
    globals.set("judgmentMainPosition", 600.0)?;
    globals.set("judgmentMainPositionV2", 1.0)?;
    globals.set("judgmentMainPositionV3", 0.0)?;
    for adjustment in &ADJUSTMENTS {
        let value = definition
            .flag(adjustment.flag)
            .then(|| offsets.get(adjustment.name))
            .flatten()
            .map_or(0, |offset| match adjustment.axis {
                Axis::X => offset.x,
                Axis::Y => offset.y.saturating_neg(),
                Axis::W => offset.w,
                Axis::H => offset.h,
            });
        let value = f64::from(value.clamp(-1000, 1000));
        globals.set(adjustment.global, value)?;
    }
    Ok(())
}

/// Image files added through BMZ, by file name. Every library root may hold the folder.
pub(super) fn lane_cover_names(context: &SkinPathContext) -> Vec<String> {
    let mut names = Vec::new();
    for root in context.library_roots() {
        let Ok(entries) = fs::read_dir(root.join(QWILIGHT_LANE_COVER_DIR)) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_qwilight_lane_cover_image_path(&path) {
                continue;
            }
            if context.resolve_asset(&path.to_string_lossy()).is_err() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names.truncate(LANE_COVER_LIMIT);
    names
}

pub(super) fn lane_cover_property(names: &[String]) -> Json {
    let mut items = vec![LANE_COVER_SKIN.to_string()];
    if !names.is_empty() {
        items.push(LANE_COVER_RANDOM.to_string());
        items.extend(names.iter().cloned());
    }
    json!({
        "category": "BMZ", "name": QWILIGHT_LANE_COVER_PROPERTY, "def": LANE_COVER_SKIN,
        "item": items.iter().enumerate().map(|(j, name)| json!({
            "name": name, "op": LANE_COVER_OP + j
        })).collect::<Vec<_>>()
    })
}

/// Qwilight prefers the player's own images and falls back to the skin's.
pub(super) enum LaneCoverChoice {
    Skin,
    Random,
    File(String),
}

impl LaneCoverChoice {
    pub(super) fn from_options(options: &BTreeMap<String, String>) -> Self {
        match options.get(QWILIGHT_LANE_COVER_PROPERTY).map(String::as_str) {
            None | Some(LANE_COVER_SKIN) => Self::Skin,
            Some(LANE_COVER_RANDOM) => Self::Random,
            Some(name) => Self::File(name.to_string()),
        }
    }

    /// The added image to draw, or `None` for the skin's own lane cover.
    pub(super) fn user_image(
        &self,
        context: &SkinPathContext,
        warnings: &mut Vec<String>,
    ) -> Option<String> {
        let name = match self {
            Self::Skin => return None,
            Self::Random => {
                let names = lane_cover_names(context);
                let index = crate::random_file::random_file_index(names.len()).ok()?;
                names.into_iter().nth(index)?
            }
            Self::File(name) => name.clone(),
        };
        // Saved values are file names only; anything else is not a BMZ-added image.
        let plain = Path::new(&name).file_name().and_then(|n| n.to_str()) == Some(name.as_str());
        let resolved = plain
            .then(|| {
                context.library_roots().iter().find_map(|root| {
                    let path = root.join(QWILIGHT_LANE_COVER_DIR).join(&name);
                    path.is_file()
                        .then(|| context.resolve_asset(&path.to_string_lossy()).ok())
                        .flatten()
                })
            })
            .flatten();
        if resolved.is_none() {
            warnings.push(format!("Qwilight lane cover image {name} is missing; using the skin's"));
        }
        resolved.map(|path| path.to_string_lossy().into_owned())
    }
}

/// Width and height from an image header, without decoding pixels.
pub(super) fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let u16_be = |at: usize| Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u16_le = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_be = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let u24_le = |at: usize| {
        let b = bytes.get(at..at + 3)?;
        Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
    };
    let size = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        (u32_be(16)?, u32_be(20)?)
    } else if bytes.starts_with(b"GIF8") {
        (u32::from(u16_le(6)?), u32::from(u16_le(8)?))
    } else if bytes.starts_with(b"BM") {
        let w = i32::from_le_bytes(bytes.get(18..22)?.try_into().ok()?);
        let h = i32::from_le_bytes(bytes.get(22..26)?.try_into().ok()?);
        (w.unsigned_abs(), h.unsigned_abs())
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        match bytes.get(12..16)? {
            b"VP8 " => (u32::from(u16_le(26)? & 0x3fff), u32::from(u16_le(28)? & 0x3fff)),
            b"VP8L" => {
                let b = bytes.get(21..25)?;
                let w = 1 + (u32::from(b[0]) | (u32::from(b[1]) & 0x3f) << 8);
                let h = 1
                    + (u32::from(b[1]) >> 6 | u32::from(b[2]) << 2 | (u32::from(b[3]) & 0xf) << 10);
                (w, h)
            }
            b"VP8X" => (1 + u24_le(24)?, 1 + u24_le(27)?),
            _ => return None,
        }
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let mut at = 2;
        loop {
            while bytes.get(at) == Some(&0xff) && bytes.get(at + 1) == Some(&0xff) {
                at += 1;
            }
            if bytes.get(at) != Some(&0xff) {
                return None;
            }
            let marker = *bytes.get(at + 1)?;
            if matches!(marker, 0x01 | 0xd0..=0xd8) {
                at += 2;
                continue;
            }
            let length = usize::from(u16_be(at + 2)?);
            if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                break (u32::from(u16_be(at + 7)?), u32::from(u16_be(at + 5)?));
            }
            at += 2 + length;
        }
    } else {
        return None;
    };
    (size.0 > 0 && size.1 > 0 && size.0 <= 65535 && size.1 <= 65535).then_some(size)
}
