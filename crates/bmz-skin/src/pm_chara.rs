use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use bmz_skin_document::{
    SkinPmCharaFrameDef, SkinPmCharaMotionLayerDef, SkinPmCharaRuntimeDef, SkinSourceDef,
};
use encoding_rs::SHIFT_JIS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPmChara {
    pub runtime: SkinPmCharaRuntimeDef,
    pub sources: Vec<SkinSourceDef>,
}

/// Loads the `.chp` selected by a beatoraja `pmchara` source.
///
/// `source_path` may point directly to a `.chp` file or to the selected
/// character directory. Image paths are returned as ordinary skin sources so
/// the app's existing sandboxed image decode/upload pipeline remains in charge
/// of filesystem access and GPU resources.
pub fn load_pm_chara(
    source_path: &Path,
    source_id_prefix: &str,
    chara_type: i32,
    color: i32,
) -> Result<LoadedPmChara> {
    if !(0..=15).contains(&chara_type) {
        bail!("unsupported PMchara type: {chara_type}");
    }
    let chp_path = find_chp_file(source_path)?;
    let parsed = ParsedChp::load(&chp_path)?;
    let mut builder =
        RuntimeBuilder::new(source_id_prefix, chp_path.parent().unwrap_or(Path::new(".")));

    let mut runtime = SkinPmCharaRuntimeDef {
        canvas_width: parsed.size[0].max(1),
        canvas_height: parsed.size[1].max(1),
        motions: Vec::new(),
    };
    match chara_type {
        0 => builder.append_animation_layers(&parsed, color, None, &mut runtime)?,
        1 => builder.append_static_layer(&parsed, color, StaticPart::Background, &mut runtime)?,
        2 => builder.append_static_layer(&parsed, color, StaticPart::Name, &mut runtime)?,
        3 => builder.append_static_layer(&parsed, color, StaticPart::FaceUpper, &mut runtime)?,
        4 => builder.append_static_layer(&parsed, color, StaticPart::FaceAll, &mut runtime)?,
        5 => builder.append_static_layer(&parsed, color, StaticPart::SelectCg, &mut runtime)?,
        6..=15 => {
            let motion = motion_for_type(chara_type)
                .ok_or_else(|| anyhow!("PMchara type {chara_type} has no motion"))?;
            builder.append_animation_layers(&parsed, color, Some(motion), &mut runtime)?;
        }
        _ => unreachable!(),
    }
    if runtime.motions.is_empty() {
        bail!("PMchara contains no renderable data: {}", chp_path.display());
    }

    Ok(LoadedPmChara { runtime, sources: builder.sources })
}

fn find_chp_file(source_path: &Path) -> Result<PathBuf> {
    if source_path.is_file()
        && source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("chp"))
    {
        return Ok(source_path.to_path_buf());
    }
    let mut candidates = fs::read_dir(source_path)
        .with_context(|| format!("failed to read PMchara directory: {}", source_path.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("chp"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("PMchara .chp not found below {}", source_path.display()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct IntRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerKind {
    Pattern,
    Texture,
    Layer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawLayer {
    kind: LayerKind,
    motion: i32,
    regions: String,
    destinations: String,
    alpha: String,
    angle: String,
}

#[derive(Debug, Default)]
struct ParsedChp {
    bmp: [Option<String>; 2],
    texture: [Option<String>; 2],
    face: [Option<String>; 2],
    select_cg: [Option<String>; 2],
    face_upper: IntRect,
    face_all: IntRect,
    size: [i32; 2],
    anime: i32,
    frame_ms: BTreeMap<i32, i32>,
    loop_index: BTreeMap<i32, i32>,
    regions: BTreeMap<usize, IntRect>,
    layers: Vec<RawLayer>,
}

impl ParsedChp {
    fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read PMchara file: {}", path.display()))?;
        let (decoded, _, _) = SHIFT_JIS.decode(&bytes);
        let mut parsed = Self {
            face_upper: IntRect { x: 0, y: 0, w: 256, h: 256 },
            face_all: IntRect { x: 320, y: 0, w: 320, h: 480 },
            anime: 100,
            ..Self::default()
        };

        for raw_line in decoded.lines() {
            let line = raw_line.split_once("//").map_or(raw_line, |(line, _)| line).trim();
            if line.is_empty() || !line.starts_with('#') {
                continue;
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let Some(command) = fields.first().copied() else { continue };
            let normalized = command.to_ascii_lowercase();
            match normalized.as_str() {
                "#charbmp" => parsed.bmp[0] = field(&fields, 1),
                "#charbmp2p" => parsed.bmp[1] = field(&fields, 1),
                "#chartex" => parsed.texture[0] = field(&fields, 1),
                "#chartex2p" => parsed.texture[1] = field(&fields, 1),
                "#charface" => parsed.face[0] = field(&fields, 1),
                "#charface2p" => parsed.face[1] = field(&fields, 1),
                "#selectcg" => parsed.select_cg[0] = field(&fields, 1),
                "#selectcg2p" => parsed.select_cg[1] = field(&fields, 1),
                "#anime" => parsed.anime = decimal_field(&fields, 1).unwrap_or(100),
                "#size" => {
                    parsed.size = [
                        decimal_field(&fields, 1).unwrap_or(0),
                        decimal_field(&fields, 2).unwrap_or(0),
                    ];
                }
                "#charfaceuppersize" => parsed.face_upper = rect_fields(&fields),
                "#charfaceallsize" => parsed.face_all = rect_fields(&fields),
                "#frame" | "#flame" => {
                    if let (Some(motion), Some(value)) =
                        (decimal_field(&fields, 1), decimal_field(&fields, 2))
                    {
                        parsed.frame_ms.insert(motion, value);
                    }
                }
                "#loop" => {
                    if let (Some(motion), Some(value)) =
                        (decimal_field(&fields, 1), decimal_field(&fields, 2))
                    {
                        parsed.loop_index.insert(motion, value);
                    }
                }
                "#pattern" | "#patern" | "#texture" | "#layer" => {
                    let Some(motion) = decimal_field(&fields, 1) else { continue };
                    let Some(regions) = fields.get(2) else { continue };
                    let kind = match normalized.as_str() {
                        "#texture" => LayerKind::Texture,
                        "#layer" => LayerKind::Layer,
                        _ => LayerKind::Pattern,
                    };
                    parsed.layers.push(RawLayer {
                        kind,
                        motion,
                        regions: clean_pairs(regions),
                        destinations: fields
                            .get(3)
                            .map_or_else(String::new, |value| clean_pairs(value)),
                        alpha: fields.get(4).map_or_else(String::new, |value| clean_pairs(value)),
                        angle: fields.get(5).map_or_else(String::new, |value| clean_pairs(value)),
                    });
                }
                _ => {
                    if command.len() == 3
                        && let Some(index) = parse_pair(&command[1..], 36)
                        && fields.len() >= 5
                    {
                        parsed.regions.insert(index, rect_fields(&fields));
                    }
                }
            }
        }
        Ok(parsed)
    }
}

fn field(fields: &[&str], index: usize) -> Option<String> {
    fields.get(index).filter(|value| !value.is_empty()).map(|value| (*value).to_string())
}

fn decimal_field(fields: &[&str], index: usize) -> Option<i32> {
    let digits = fields
        .get(index)?
        .chars()
        .filter(|character| character.is_ascii_digit() || *character == '-')
        .collect::<String>();
    digits.parse().ok()
}

fn rect_fields(fields: &[&str]) -> IntRect {
    IntRect {
        x: decimal_field(fields, 1).unwrap_or(0),
        y: decimal_field(fields, 2).unwrap_or(0),
        w: decimal_field(fields, 3).unwrap_or(0),
        h: decimal_field(fields, 4).unwrap_or(0),
    }
}

fn clean_pairs(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect()
}

fn parse_pair(pair: &str, radix: u32) -> Option<usize> {
    (pair.len() == 2).then_some(())?;
    usize::from_str_radix(pair, radix).ok()
}

fn pairs(value: &str) -> impl Iterator<Item = &str> {
    value.as_bytes().as_chunks::<2>().0.iter().filter_map(|chunk| std::str::from_utf8(chunk).ok())
}

struct RuntimeBuilder<'a> {
    prefix: &'a str,
    base_dir: &'a Path,
    source_ids: HashMap<PathBuf, String>,
    sources: Vec<SkinSourceDef>,
}

impl<'a> RuntimeBuilder<'a> {
    fn new(prefix: &'a str, base_dir: &'a Path) -> Self {
        Self { prefix, base_dir, source_ids: HashMap::new(), sources: Vec::new() }
    }

    fn append_animation_layers(
        &mut self,
        parsed: &ParsedChp,
        color: i32,
        selected_motion: Option<i32>,
        runtime: &mut SkinPmCharaRuntimeDef,
    ) -> Result<()> {
        for layer in &parsed.layers {
            if selected_motion.is_some_and(|motion| motion != layer.motion) {
                continue;
            }
            let source_path = match layer.kind {
                LayerKind::Texture => selected_path(&parsed.texture, color),
                LayerKind::Pattern | LayerKind::Layer => selected_path(&parsed.bmp, color),
            };
            let Some(source_path) = source_path else { continue };
            let source_id = self.source_id(source_path);
            let frames = build_frames(layer, parsed, runtime.canvas_width, runtime.canvas_height);
            if frames.is_empty() {
                continue;
            }
            let frame_ms =
                parsed.frame_ms.get(&layer.motion).copied().unwrap_or(parsed.anime).max(1);
            let count = frames.len();
            let mut loop_index = parsed.loop_index.get(&layer.motion).copied().unwrap_or(-1);
            if loop_index >= count as i32 - 1 {
                loop_index = count as i32 - 2;
            } else if loop_index < -1 {
                loop_index = -1;
            }
            runtime.motions.push(SkinPmCharaMotionLayerDef {
                motion: layer.motion,
                source_id,
                frame_ms,
                loop_start: (loop_index + 1).max(0) as usize,
                frames,
            });
        }
        Ok(())
    }

    fn append_static_layer(
        &mut self,
        parsed: &ParsedChp,
        color: i32,
        part: StaticPart,
        runtime: &mut SkinPmCharaRuntimeDef,
    ) -> Result<()> {
        let (path, region) = match part {
            StaticPart::Background => {
                (selected_path(&parsed.bmp, color), parsed.regions.get(&1).copied())
            }
            StaticPart::Name => {
                (selected_path(&parsed.bmp, color), parsed.regions.get(&0).copied())
            }
            StaticPart::FaceUpper => (selected_path(&parsed.face, color), Some(parsed.face_upper)),
            StaticPart::FaceAll => (selected_path(&parsed.face, color), Some(parsed.face_all)),
            StaticPart::SelectCg => (selected_path(&parsed.select_cg, color), None),
        };
        let path = path.ok_or_else(|| anyhow!("PMchara image path is missing for type"))?;
        let region = region.unwrap_or(IntRect { x: 0, y: 0, w: -1, h: -1 });
        let source_id = self.source_id(path);
        runtime.canvas_width = region.w.max(1);
        runtime.canvas_height = region.h.max(1);
        runtime.motions.push(SkinPmCharaMotionLayerDef {
            motion: 0,
            source_id,
            frame_ms: 1,
            loop_start: 0,
            frames: vec![SkinPmCharaFrameDef {
                source_x: region.x,
                source_y: region.y,
                source_w: region.w,
                source_h: region.h,
                destination_x: 0,
                destination_y: 0,
                destination_w: runtime.canvas_width,
                destination_h: runtime.canvas_height,
                alpha: 255,
                angle: 0,
            }],
        });
        Ok(())
    }

    fn source_id(&mut self, relative_path: &str) -> String {
        let path = self.base_dir.join(relative_path.replace('\\', "/"));
        if let Some(id) = self.source_ids.get(&path) {
            return id.clone();
        }
        let id = format!("{}:source:{}", self.prefix, self.sources.len());
        self.source_ids.insert(path.clone(), id.clone());
        self.sources
            .push(SkinSourceDef { id: id.clone(), path: path.to_string_lossy().to_string() });
        id
    }
}

#[derive(Debug, Clone, Copy)]
enum StaticPart {
    Background,
    Name,
    FaceUpper,
    FaceAll,
    SelectCg,
}

fn selected_path(paths: &[Option<String>; 2], color: i32) -> Option<&str> {
    if color == 2 { paths[1].as_deref().or(paths[0].as_deref()) } else { paths[0].as_deref() }
}

fn build_frames(
    layer: &RawLayer,
    parsed: &ParsedChp,
    canvas_width: i32,
    canvas_height: i32,
) -> Vec<SkinPmCharaFrameDef> {
    let region_codes = pairs(&layer.regions).collect::<Vec<_>>();
    if region_codes.is_empty() {
        return Vec::new();
    }
    let destination_codes = pairs(&layer.destinations).collect::<Vec<_>>();
    let alpha_codes = pairs(&layer.alpha).collect::<Vec<_>>();
    let angle_codes = pairs(&layer.angle).collect::<Vec<_>>();
    region_codes
        .iter()
        .enumerate()
        .map(|(index, code)| {
            let source = parse_pair(code, 36)
                .and_then(|code| parsed.regions.get(&code).copied())
                .unwrap_or_default();
            let destination = destination_codes
                .get(index)
                .and_then(|code| parse_pair(code, 36))
                .and_then(|code| parsed.regions.get(&code).copied())
                .unwrap_or(IntRect { x: 0, y: 0, w: canvas_width, h: canvas_height });
            let alpha = alpha_codes
                .get(index)
                .and_then(|code| parse_pair(code, 16))
                .map_or(255, |value| value.min(255) as i32);
            let angle = angle_codes
                .get(index)
                .and_then(|code| parse_pair(code, 16))
                .map_or(0, |value| (value.min(255) as f32 * 360.0 / 256.0).round() as i32);
            SkinPmCharaFrameDef {
                source_x: source.x,
                source_y: source.y,
                source_w: source.w,
                source_h: source.h,
                destination_x: destination.x,
                destination_y: destination.y,
                destination_w: destination.w,
                destination_h: destination.h,
                alpha,
                angle,
            }
        })
        .collect()
}

fn motion_for_type(chara_type: i32) -> Option<i32> {
    match chara_type {
        6 => Some(1),
        7 => Some(6),
        8 => Some(7),
        9 => Some(8),
        10 => Some(10),
        11 => Some(17),
        12 => Some(15),
        13 => Some(16),
        14 => Some(3),
        15 => Some(14),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn loads_pm_chara_pattern_motions_and_loop_boundary() {
        let root = test_dir("bmz-pmchara-pattern");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("character.png"), []).unwrap();
        fs::write(
            root.join("sample.chp"),
            b"#CharBMP character.png\n#Anime 160\n#Size 20 30\n#00 0 0 10 30\n#01 10 0 10 30\n#Loop 01 01\n#Pattern 01 000100\n",
        )
        .unwrap();

        let loaded = load_pm_chara(&root, "pm", 0, 1).unwrap();
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.runtime.canvas_width, 20);
        assert_eq!(loaded.runtime.canvas_height, 30);
        assert_eq!(loaded.runtime.motions.len(), 1);
        let motion = &loaded.runtime.motions[0];
        assert_eq!(motion.motion, 1);
        assert_eq!(motion.frame_ms, 160);
        assert_eq!(motion.loop_start, 2);
        assert_eq!(motion.frames.len(), 3);
        assert_eq!(motion.frames[1].source_x, 10);
    }

    #[test]
    fn loads_pm_chara_background_as_static_part() {
        let root = test_dir("bmz-pmchara-background");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("character.png"), []).unwrap();
        fs::write(root.join("sample.chp"), b"#CharBMP character.png\n#01 4 5 60 70\n").unwrap();

        let loaded = load_pm_chara(&root, "pm", 1, 1).unwrap();
        let frame = loaded.runtime.motions[0].frames[0];
        assert_eq!(
            (frame.source_x, frame.source_y, frame.source_w, frame.source_h),
            (4, 5, 60, 70)
        );
        assert_eq!((loaded.runtime.canvas_width, loaded.runtime.canvas_height), (60, 70));
    }
}
